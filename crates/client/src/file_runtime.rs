//! Native actors transfer files over exact-permission, authenticated QUIC.
//! Picker paths stay native; public status contains no paths or credentials.
use crate::{
    NativeClient,
    files::{self, Message, Offer, Receiver, Sender},
};
use farsail_core::RemotePermission;
use farsail_transport::{Channel, Frame, Session};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, broadcast, mpsc, oneshot};
const HISTORY: usize = 32;
const RATE: u64 = 4 * 1024 * 1024;
const HUMAN_TIMEOUT: Duration = Duration::from_secs(300);
const DATA_TIMEOUT: Duration = Duration::from_secs(30);
type Reply = oneshot::Sender<Result<(), String>>;
enum Command {
    Send(Sender, Reply),
    Accept(String, Receiver, Reply),
    Reject(String, Reply),
    Cancel(String, Reply),
}

#[derive(Default)]
struct TransferLive {
    cancelled: AtomicBool,
    published: AtomicBool,
    finish_sent: AtomicBool,
}
struct Live {
    session: Session,
    host: bool,
    cancelled: Arc<AtomicBool>,
    lifecycle: Arc<StdMutex<()>>,
    transfers: StdMutex<Vec<Value>>,
    tokens: StdMutex<HashMap<String, Arc<TransferLive>>>,
    error: StdMutex<Option<String>>,
    tx: mpsc::Sender<Command>,
}
impl Live {
    fn stop(&self) {
        let _gate = self.lifecycle.lock().unwrap();
        self.cancelled.store(true, Ordering::SeqCst);
        self.session.close();
    }
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::SeqCst) && self.session.is_open_now()
    }
    fn token(&self, id: &str) -> Option<Arc<TransferLive>> {
        self.tokens.lock().unwrap().get(id).cloned()
    }
    fn terminal(&self, id: &str) -> bool {
        self.transfers.lock().unwrap().iter().any(|r| {
            r["id"] == id && !matches!(r["state"].as_str(), Some("offered" | "transferring"))
        })
    }
    fn status(&self) -> Value {
        let path = self.session.path().0;
        json!({"id":self.session.id(),"host":self.host,"state":if self.active(){"connected"}else{"closed"},"path":if matches!(path,"direct"|"relay"){path}else{"none"},"error":*self.error.lock().unwrap(),"transfers":*self.transfers.lock().unwrap()})
    }
    fn record(&self, offer: &Offer, direction: &str) -> Result<Arc<TransferLive>, String> {
        let mut rows = self.transfers.lock().unwrap();
        if rows.iter().any(|r| r["id"] == offer.id) {
            return Err("重复的文件任务。".into());
        }
        if rows.len() >= HISTORY {
            let Some(index) = rows
                .iter()
                .position(|r| !matches!(r["state"].as_str(), Some("offered" | "transferring")))
            else {
                return Err("文件任务较多，请稍后再试。".into());
            };
            let old = rows.remove(index);
            if let Some(id) = old["id"].as_str() {
                self.tokens.lock().unwrap().remove(id);
            }
        }
        let token = Arc::new(TransferLive::default());
        self.tokens
            .lock()
            .unwrap()
            .insert(offer.id.clone(), token.clone());
        rows.push(json!({"id":offer.id,"name":offer.name,"size":offer.size,"transferred":0,"direction":direction,"state":"offered"}));
        Ok(token)
    }
    fn update(&self, id: &str, state: &str, n: Option<u64>, error: Option<String>) {
        if let Some(row) = self
            .transfers
            .lock()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == id)
        {
            row["state"] = json!(state);
            if let Some(n) = n {
                row["transferred"] = json!(n);
            }
            row["error"] = json!(error);
        }
    }
}
enum SendPhase {
    Accept,
    ChunkAck(u64),
    Complete,
}
enum ActorEvent {
    Closed,
    Tick,
    Command(Option<Box<Command>>),
    Frame(Option<farsail_transport::Result<Frame>>),
}
async fn next_event(
    live: &Live,
    tick: &mut tokio::time::Interval,
    commands: &mut mpsc::Receiver<Command>,
    frames: &mut mpsc::Receiver<farsail_transport::Result<Frame>>,
) -> ActorEvent {
    tokio::select! {
        _ = live.session.wait_closed() => ActorEvent::Closed,
        _ = tick.tick() => ActorEvent::Tick,
        command = commands.recv() => ActorEvent::Command(command.map(Box::new)),
        frame = frames.recv() => ActorEvent::Frame(frame),
    }
}
struct Outgoing {
    sender: Sender,
    phase: SendPhase,
    at: Instant,
    token: Arc<TransferLive>,
}
struct Incoming {
    offer: Offer,
    receiver: Option<Receiver>,
    at: Instant,
    token: Arc<TransferLive>,
}
fn file_error(error: files::Error) -> String {
    match error {
        files::Error::Exists => "文件已存在，请选择新名称。",
        files::Error::Integrity => "文件校验失败，请重新发送。",
        files::Error::Cancelled => "文件任务已取消。",
        files::Error::Io(_) => "无法读写文件，请检查权限、磁盘空间及文件系统支持。",
        files::Error::Invalid => "文件或文件名不受支持，请选择普通文件。",
        files::Error::Limit => "文件或数据超过传输限制。",
        files::Error::Sequence => "文件数据顺序异常，请重新发送。",
    }
    .into()
}
pub struct NativeFiles {
    client: Arc<NativeClient>,
    events: Mutex<Option<broadcast::Receiver<(Session, bool)>>>,
    sessions: StdMutex<HashMap<String, Arc<Live>>>,
    offers: StdMutex<HashMap<(String, String), Offer>>,
    io: Arc<Semaphore>,
    rate: Mutex<Instant>,
    updating: AtomicBool,
}
impl NativeFiles {
    pub fn new(client: Arc<NativeClient>) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Some(client.subscribe_sessions())),
            client,
            sessions: StdMutex::new(HashMap::new()),
            offers: StdMutex::new(HashMap::new()),
            io: Arc::new(Semaphore::new(2)),
            rate: Mutex::new(Instant::now()),
            updating: AtomicBool::new(false),
        })
    }
    pub async fn listen(self: Arc<Self>) {
        let Some(mut events) = self.events.lock().await.take() else {
            return;
        };
        loop {
            match events.recv().await {
                Ok((session, host)) if session.permission() == RemotePermission::Files => {
                    self.attach(session, host)
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
    fn attach(self: &Arc<Self>, session: Session, host: bool) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(existing) = sessions.get(session.id()) {
            if existing.session.stable_id() != session.stable_id() {
                session.close();
            }
            return;
        }
        if self.updating.load(Ordering::SeqCst) || (host && !self.client.files_enabled()) {
            session.close();
            return;
        }
        sessions.retain(|_, s| s.active());
        if sessions.len() >= 16 {
            session.close();
            return;
        }
        let (tx, rx) = mpsc::channel(8);
        let live = Arc::new(Live {
            session,
            host,
            cancelled: Arc::new(AtomicBool::new(false)),
            lifecycle: Arc::new(StdMutex::new(())),
            transfers: StdMutex::new(Vec::new()),
            tokens: StdMutex::new(HashMap::new()),
            error: StdMutex::new(None),
            tx,
        });
        sessions.insert(live.session.id().into(), live.clone());
        tokio::spawn(self.clone().actor(live, rx));
    }
    pub fn status(&self, id: Option<&str>) -> Result<Value, String> {
        let sessions = self.sessions.lock().unwrap();
        if let Some(id) = id {
            return sessions
                .get(id)
                .map(|s| s.status())
                .ok_or_else(|| "文件连接尚未就绪或已结束。".into());
        }
        let mut values: Vec<_> = sessions.values().map(|s| s.status()).collect();
        values.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(json!({"enabled":self.client.files_enabled(),"sessions":values}))
    }
    fn allowed(&self, live: &Live) -> bool {
        live.active() && (!live.host || self.client.files_enabled())
    }
    fn live(&self, id: &str) -> Result<Arc<Live>, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .filter(|s| self.allowed(s))
            .cloned()
            .ok_or_else(|| "文件连接已结束，请重新申请连接。".into())
    }
    pub fn incoming_offer(&self, id: &str, transfer_id: &str) -> Result<Offer, String> {
        let live = self.live(id)?;
        let rows = live.transfers.lock().unwrap();
        if !rows.iter().any(|r| {
            r["id"] == transfer_id && r["direction"] == "receive" && r["state"] == "offered"
        }) || live
            .token(transfer_id)
            .is_none_or(|t| t.cancelled.load(Ordering::SeqCst))
        {
            return Err("文件申请已取消或已处理。".into());
        }
        self.offers
            .lock()
            .unwrap()
            .get(&(id.into(), transfer_id.into()))
            .cloned()
            .ok_or_else(|| "文件申请已取消或已处理。".into())
    }
    async fn permit(&self, live: &Live) -> Result<OwnedSemaphorePermit, String> {
        tokio::select! {p=self.io.clone().acquire_owned()=>p.map_err(|_|"文件任务已取消。".into()),_=live.session.wait_closed()=>Err("文件连接已结束。".into())}
    }
    pub async fn send_path(&self, id: &str, path: PathBuf) -> Result<(), String> {
        let live = self.live(id)?;
        let permit = self.permit(&live).await?;
        let cancelled = live.cancelled.clone();
        let sender = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            Sender::open_cancellable(path, &cancelled)
        })
        .await
        .map_err(|_| "无法读取文件。")?
        .map_err(file_error)?;
        if !self.allowed(&live) {
            return Err("文件连接已结束。".into());
        }
        let (tx, rx) = oneshot::channel();
        live.tx
            .send(Command::Send(sender, tx))
            .await
            .map_err(|_| "文件连接已结束。")?;
        rx.await.map_err(|_| "文件连接已结束。")?
    }
    pub async fn accept_path(
        &self,
        id: &str,
        transfer_id: &str,
        path: PathBuf,
    ) -> Result<(), String> {
        let live = self.live(id)?;
        self.incoming_offer(id, transfer_id)?;
        let permit = self.permit(&live).await?;
        // A save chooser can outlive withdrawal, timeout or another chooser.
        let offer = self.incoming_offer(id, transfer_id)?;
        let token = live.token(transfer_id).ok_or("文件申请已取消或已处理。")?;
        let client = self.client.clone();
        let worker_live = live.clone();
        let transfer_id = transfer_id.to_owned();
        let worker_id = transfer_id.clone();
        let receiver = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if token.cancelled.load(Ordering::SeqCst)
                || !worker_live.active()
                || (worker_live.host && !client.files_enabled())
            {
                return Err(files::Error::Cancelled);
            }
            if !worker_live
                .transfers
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["id"] == worker_id && r["state"] == "offered")
            {
                return Err(files::Error::Cancelled);
            }
            Receiver::accept(offer, path)
        })
        .await
        .map_err(|_| "无法保存文件。")?
        .map_err(file_error)?;
        if !self.allowed(&live) {
            self.discard(receiver).await;
            return Err("文件连接已结束。".into());
        }
        let (tx, rx) = oneshot::channel();
        if let Err(error) = live
            .tx
            .send(Command::Accept(transfer_id, receiver, tx))
            .await
        {
            if let Command::Accept(_, receiver, _) = error.0 {
                self.discard(receiver).await;
            }
            return Err("文件连接已结束。".into());
        }
        rx.await.map_err(|_| "文件连接已结束。")?
    }
    pub async fn reject(&self, id: &str, transfer_id: &str) -> Result<(), String> {
        self.action(id, transfer_id, false).await
    }
    pub async fn cancel(&self, id: &str, transfer_id: &str) -> Result<(), String> {
        self.action(id, transfer_id, true).await
    }
    async fn action(&self, id: &str, transfer_id: &str, cancel: bool) -> Result<(), String> {
        let live = self.live(id)?;
        {
            let _gate = live.lifecycle.lock().unwrap();
            let rows = live.transfers.lock().unwrap();
            let row = rows
                .iter()
                .find(|r| r["id"] == transfer_id)
                .ok_or("文件任务已结束。")?;
            let token = live.token(transfer_id).ok_or("文件任务已结束。")?;
            if row["state"] == "cancelled" && cancel {
                return Ok(());
            }
            if token.published.load(Ordering::SeqCst) {
                return Err("文件已完成，不能取消。".into());
            }
            if token.finish_sent.load(Ordering::SeqCst) {
                return Err("文件已发送，正在完成保存确认，无法撤销已送达的数据。".into());
            }
            if !matches!(row["state"].as_str(), Some("offered" | "transferring"))
                || (!cancel && (row["direction"] != "receive" || row["state"] != "offered"))
            {
                return Err("文件任务已结束或已获准接收。".into());
            }
            // Mark before queuing so a blocked writer/flush sees cancellation.
            token.cancelled.store(true, Ordering::SeqCst);
        }
        let (tx, rx) = oneshot::channel();
        let command = if cancel {
            Command::Cancel(transfer_id.into(), tx)
        } else {
            Command::Reject(transfer_id.into(), tx)
        };
        live.tx
            .send(command)
            .await
            .map_err(|_| "文件连接已结束。")?;
        rx.await.map_err(|_| "文件连接已结束。")?
    }
    pub fn stop(&self, id: &str) {
        if let Some(live) = self.sessions.lock().unwrap().get(id) {
            live.stop();
        }
    }
    pub fn stop_hosts(&self) {
        for live in self.sessions.lock().unwrap().values().filter(|s| s.host) {
            live.stop();
        }
    }
    pub fn stop_all(&self) {
        for live in self.sessions.lock().unwrap().values() {
            live.stop();
        }
    }
    pub fn begin_update(&self) -> bool {
        let sessions = self.sessions.lock().unwrap();
        if sessions.values().any(|s| s.active()) {
            return false;
        }
        self.updating.store(true, Ordering::SeqCst);
        true
    }
    pub fn finish_update(&self) {
        self.updating.store(false, Ordering::SeqCst);
    }
    async fn discard(&self, receiver: Receiver) {
        if let Ok(permit) = self.io.clone().acquire_owned().await {
            let _ = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                drop(receiver);
            })
            .await;
        }
    }
    async fn clear_incoming(&self, live: &Live, incoming: Option<Incoming>) {
        if let Some(incoming) = incoming {
            self.offers
                .lock()
                .unwrap()
                .remove(&(live.session.id().into(), incoming.offer.id));
            if let Some(receiver) = incoming.receiver {
                self.discard(receiver).await;
            }
        }
    }
    async fn wire(&self, live: &Live, message: Message) -> Result<(), String> {
        if !self.allowed(live) {
            return Err("文件连接已结束。".into());
        }
        let bytes = message.encode().map_err(file_error)?;
        if matches!(message, Message::Chunk { .. }) {
            let wait = {
                let mut next = self.rate.lock().await;
                let now = Instant::now();
                if *next < now {
                    *next = now;
                }
                let wait = next.saturating_duration_since(now);
                *next += Duration::from_secs_f64(bytes.len() as f64 / RATE as f64);
                wait
            };
            tokio::select! {_=tokio::time::sleep(wait)=>(),_=live.session.wait_closed()=>return Err("文件连接已结束。".into())}
            if live
                .token(message.id())
                .is_some_and(|t| t.cancelled.load(Ordering::SeqCst))
            {
                return Err("文件任务已取消。".into());
            }
        }
        if !self.allowed(live) {
            return Err("文件连接已结束。".into());
        }
        live.session
            .send(Frame {
                channel: Channel::File,
                bytes,
            })
            .await
            .map_err(|_| "文件连接失败，请重新申请连接。".into())
    }
    async fn cancel_work(&self, live: &Live, id: &str) {
        live.update(id, "cancelled", None, None);
        self.offers
            .lock()
            .unwrap()
            .remove(&(live.session.id().into(), id.into()));
        if self.allowed(live) {
            let _ = self.wire(live, Message::Cancel { id: id.into() }).await;
        }
    }
    async fn advance(
        &self,
        live: &Live,
        sender: Sender,
        token: Arc<TransferLive>,
    ) -> Result<Option<Outgoing>, String> {
        let id = sender.offer().id.clone();
        let permit = self.permit(live).await?;
        let cancelled = live.cancelled.clone();
        let session = live.session.clone();
        let worker_token = token.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut sender = sender;
            if cancelled.load(Ordering::SeqCst)
                || worker_token.cancelled.load(Ordering::SeqCst)
                || !session.is_open_now()
            {
                return Err(files::Error::Cancelled);
            }
            let message = sender.next_chunk()?;
            if worker_token.cancelled.load(Ordering::SeqCst) {
                return Err(files::Error::Cancelled);
            }
            Ok((sender, message))
        })
        .await
        .map_err(|_| "无法读取文件。")?;
        let (sender, message) = match result {
            Ok(result) => result,
            Err(files::Error::Cancelled)
                if token.cancelled.load(Ordering::SeqCst) && self.allowed(live) =>
            {
                self.cancel_work(live, &id).await;
                return Ok(None);
            }
            Err(error) => return Err(file_error(error)),
        };
        let phase = if message.is_none() {
            let was_cancelled = {
                let _gate = live.lifecycle.lock().unwrap();
                let cancelled = token.cancelled.load(Ordering::SeqCst);
                if !cancelled {
                    if !self.allowed(live) {
                        return Err("文件连接已结束。".into());
                    }
                    token.finish_sent.store(true, Ordering::SeqCst);
                }
                cancelled
            };
            if was_cancelled {
                self.cancel_work(live, &id).await;
                return Ok(None);
            }
            SendPhase::Complete
        } else {
            SendPhase::ChunkAck(sender.transferred())
        };
        if let Err(error) = self
            .wire(
                live,
                message.unwrap_or_else(|| Message::Finish { id: id.clone() }),
            )
            .await
        {
            if token.cancelled.load(Ordering::SeqCst) && self.allowed(live) {
                self.cancel_work(live, &id).await;
                return Ok(None);
            }
            return Err(error);
        }
        Ok(Some(Outgoing {
            sender,
            phase,
            at: Instant::now(),
            token,
        }))
    }
    async fn run_actor(
        &self,
        live: &Arc<Live>,
        commands: &mut mpsc::Receiver<Command>,
        frames: &mut mpsc::Receiver<farsail_transport::Result<Frame>>,
        outgoing: &mut Option<Outgoing>,
        incoming: &mut Option<Incoming>,
    ) -> Result<(), String> {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            let event = next_event(live, &mut tick, commands, frames).await;
            match event {
                ActorEvent::Closed => break,
                ActorEvent::Tick => {
                    if !self.allowed(live) {
                        break;
                    }
                    if outgoing.as_ref().is_some_and(|o| {
                        o.at.elapsed()
                            > if matches!(o.phase, SendPhase::Accept) {
                                HUMAN_TIMEOUT
                            } else {
                                DATA_TIMEOUT
                            }
                    }) {
                        let old = outgoing.take().unwrap();
                        let id = old.sender.offer().id.clone();
                        old.token.cancelled.store(true, Ordering::SeqCst);
                        live.update(&id, "failed", None, Some("对方未响应，请重新发送。".into()));
                        self.wire(live, Message::Cancel { id }).await?;
                    }
                    if incoming.as_ref().is_some_and(|i| {
                        i.at.elapsed()
                            > if i.receiver.is_none() {
                                HUMAN_TIMEOUT
                            } else {
                                DATA_TIMEOUT
                            }
                    }) {
                        let old = incoming.take().unwrap();
                        let id = old.offer.id.clone();
                        old.token.cancelled.store(true, Ordering::SeqCst);
                        live.update(&id, "failed", None, Some("文件任务已超时。".into()));
                        self.clear_incoming(live, Some(old)).await;
                        self.wire(live, Message::Cancel { id }).await?;
                    }
                }
                ActorEvent::Command(command) => {
                    let Some(command) = command else {
                        break;
                    };
                    match *command {
                        Command::Send(sender, reply) => {
                            if outgoing.is_some() {
                                let _ = reply.send(Err("请等待当前发送完成。".into()));
                                continue;
                            }
                            let offer = sender.offer().clone();
                            let token = match live.record(&offer, "send") {
                                Ok(t) => t,
                                Err(e) => {
                                    let _ = reply.send(Err(e));
                                    continue;
                                }
                            };
                            self.wire(live, Message::Offer(offer)).await?;
                            *outgoing = Some(Outgoing {
                                sender,
                                phase: SendPhase::Accept,
                                at: Instant::now(),
                                token,
                            });
                            let _ = reply.send(Ok(()));
                        }
                        Command::Accept(id, receiver, reply) => {
                            if incoming.as_ref().is_none_or(|i| {
                                i.offer.id != id
                                    || i.receiver.is_some()
                                    || i.token.cancelled.load(Ordering::SeqCst)
                            }) || !self.allowed(live)
                            {
                                self.discard(receiver).await;
                                let _ = reply.send(Err("文件申请已取消或已处理。".into()));
                                continue;
                            }
                            self.offers
                                .lock()
                                .unwrap()
                                .remove(&(live.session.id().into(), id.clone()));
                            let item = incoming.as_mut().unwrap();
                            item.receiver = Some(receiver);
                            item.at = Instant::now();
                            live.update(&id, "transferring", None, None);
                            self.wire(live, Message::Accept { id }).await?;
                            let _ = reply.send(Ok(()));
                        }
                        Command::Reject(id, reply) => {
                            if incoming
                                .as_ref()
                                .is_none_or(|i| i.offer.id != id || i.receiver.is_some())
                            {
                                let _ = reply.send(Err("文件申请已取消或已处理。".into()));
                                continue;
                            }
                            self.clear_incoming(live, incoming.take()).await;
                            live.update(&id, "rejected", None, None);
                            self.wire(
                                live,
                                Message::Reject {
                                    id,
                                    reason: files::RejectReason::Declined,
                                },
                            )
                            .await?;
                            let _ = reply.send(Ok(()));
                        }
                        Command::Cancel(id, reply) => {
                            let mut found = false;
                            if incoming.as_ref().is_some_and(|i| i.offer.id == id) {
                                self.clear_incoming(live, incoming.take()).await;
                                found = true;
                            }
                            if outgoing.as_ref().is_some_and(|o| o.sender.offer().id == id) {
                                *outgoing = None;
                                found = true;
                            }
                            if !found {
                                if live
                                    .token(&id)
                                    .is_some_and(|t| t.cancelled.load(Ordering::SeqCst))
                                    && live.terminal(&id)
                                {
                                    let _ = reply.send(Ok(()));
                                } else {
                                    let _ = reply.send(Err("文件任务已结束。".into()));
                                }
                                continue;
                            }
                            live.update(&id, "cancelled", None, None);
                            self.wire(live, Message::Cancel { id }).await?;
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
                ActorEvent::Frame(frame) => {
                    let Some(frame) = frame else {
                        break;
                    };
                    let frame = frame.map_err(|_| "文件连接失败，请重新申请连接。")?;
                    if frame.channel != Channel::File {
                        return Err("文件通道收到不兼容的数据。".into());
                    }
                    let message = Message::decode(&frame.bytes).map_err(file_error)?;
                    // Known late terminal frames must not write or kill a reusable
                    // connection. Unknown IDs still fail the strict state machine.
                    if !matches!(message, Message::Offer(_))
                        && (live.terminal(message.id())
                            || live
                                .token(message.id())
                                .is_some_and(|t| t.cancelled.load(Ordering::SeqCst)))
                    {
                        continue;
                    }
                    match message {
                        Message::Offer(offer) => {
                            if incoming.is_some() {
                                self.wire(
                                    live,
                                    Message::Reject {
                                        id: offer.id,
                                        reason: files::RejectReason::Busy,
                                    },
                                )
                                .await?;
                                continue;
                            }
                            let token = match live.record(&offer, "receive") {
                                Ok(t) => t,
                                Err(_) => {
                                    self.wire(
                                        live,
                                        Message::Reject {
                                            id: offer.id,
                                            reason: files::RejectReason::Busy,
                                        },
                                    )
                                    .await?;
                                    continue;
                                }
                            };
                            self.offers.lock().unwrap().insert(
                                (live.session.id().into(), offer.id.clone()),
                                offer.clone(),
                            );
                            *incoming = Some(Incoming {
                                offer,
                                receiver: None,
                                at: Instant::now(),
                                token,
                            });
                        }
                        Message::Accept { id } => {
                            let Some(old) = outgoing.take() else {
                                return Err("无效的接收确认。".into());
                            };
                            if old.sender.offer().id != id
                                || !matches!(old.phase, SendPhase::Accept)
                            {
                                return Err("接收确认顺序异常。".into());
                            }
                            live.update(&id, "transferring", None, None);
                            *outgoing = self.advance(live, old.sender, old.token).await?;
                        }
                        Message::ChunkAck { id, offset } => {
                            let Some(old) = outgoing.take() else {
                                return Err("无效的文件确认。".into());
                            };
                            if old.sender.offer().id != id
                                || !matches!(old.phase,SendPhase::ChunkAck(expected)if expected==offset)
                            {
                                return Err("文件进度校验失败。".into());
                            }
                            live.update(&id, "transferring", Some(offset), None);
                            *outgoing = self.advance(live, old.sender, old.token).await?;
                        }
                        Message::Chunk { id, offset, data } => {
                            let permit = self.permit(live).await?;
                            let Some(mut old) = incoming.take() else {
                                return Err("文件尚未获准接收。".into());
                            };
                            if old.offer.id != id {
                                self.clear_incoming(live, Some(old)).await;
                                return Err("文件任务不匹配。".into());
                            }
                            let Some(mut receiver) = old.receiver.take() else {
                                return Err("文件尚未获准接收。".into());
                            };
                            let worker_live = Arc::clone(live);
                            let token = old.token.clone();
                            let client = self.client.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                if token.cancelled.load(Ordering::SeqCst)
                                    || !worker_live.active()
                                    || (worker_live.host && !client.files_enabled())
                                {
                                    return Err(files::Error::Cancelled);
                                }
                                let n = receiver.write_chunk(&id, offset, &data)?;
                                if token.cancelled.load(Ordering::SeqCst) {
                                    return Err(files::Error::Cancelled);
                                }
                                Ok((receiver, n))
                            })
                            .await
                            .map_err(|_| "文件写入失败。")?;
                            let (receiver, n) = match result {
                                Ok(r) => r,
                                Err(files::Error::Cancelled)
                                    if old.token.cancelled.load(Ordering::SeqCst)
                                        && self.allowed(live) =>
                                {
                                    self.cancel_work(live, &old.offer.id).await;
                                    continue;
                                }
                                Err(error) => return Err(file_error(error)),
                            };
                            old.receiver = Some(receiver);
                            old.at = Instant::now();
                            let ack_id = old.offer.id.clone();
                            live.update(&ack_id, "transferring", Some(n), None);
                            *incoming = Some(old);
                            self.wire(
                                live,
                                Message::ChunkAck {
                                    id: ack_id,
                                    offset: n,
                                },
                            )
                            .await?;
                        }
                        Message::Finish { id } => {
                            let permit = self.permit(live).await?;
                            let Some(mut old) = incoming.take() else {
                                return Err("无效的文件结束确认。".into());
                            };
                            if old.offer.id != id {
                                self.clear_incoming(live, Some(old)).await;
                                return Err("文件任务不匹配。".into());
                            }
                            let Some(receiver) = old.receiver.take() else {
                                return Err("文件尚未获准保存。".into());
                            };
                            let worker_live = Arc::clone(live);
                            let token = old.token.clone();
                            let published = old.token.clone();
                            let client = self.client.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                receiver.finish_guarded_with(
                                    || {
                                        let guard = worker_live.lifecycle.lock().unwrap();
                                        if token.cancelled.load(Ordering::SeqCst)
                                            || !worker_live.active()
                                            || (worker_live.host && !client.files_enabled())
                                        {
                                            return Err(files::Error::Cancelled);
                                        }
                                        Ok(guard)
                                    },
                                    || published.published.store(true, Ordering::SeqCst),
                                )
                            })
                            .await
                            .map_err(|_| "文件校验或保存失败。")?;
                            match result {
                                Ok(_) => (),
                                Err(files::Error::Cancelled)
                                    if old.token.cancelled.load(Ordering::SeqCst)
                                        && self.allowed(live) =>
                                {
                                    self.cancel_work(live, &old.offer.id).await;
                                    continue;
                                }
                                Err(error) => return Err(file_error(error)),
                            }
                            live.update(&old.offer.id, "completed", Some(old.offer.size), None);
                            self.offers
                                .lock()
                                .unwrap()
                                .remove(&(live.session.id().into(), old.offer.id.clone()));
                            self.wire(live, Message::Complete { id: old.offer.id })
                                .await?;
                        }
                        Message::Complete { id } => {
                            let Some(old) = outgoing.take() else {
                                return Err("无效的保存确认。".into());
                            };
                            if old.sender.offer().id != id
                                || !matches!(old.phase, SendPhase::Complete)
                            {
                                return Err("保存确认顺序异常。".into());
                            }
                            old.token.published.store(true, Ordering::SeqCst);
                            live.update(&id, "completed", Some(old.sender.offer().size), None);
                        }
                        Message::Reject { id, .. } => {
                            let Some(old) = outgoing.take() else {
                                return Err("无效的拒绝确认。".into());
                            };
                            if old.sender.offer().id != id
                                || !matches!(old.phase, SendPhase::Accept)
                            {
                                return Err("拒绝确认顺序异常。".into());
                            }
                            old.token.cancelled.store(true, Ordering::SeqCst);
                            live.update(&id, "rejected", None, None);
                        }
                        Message::Cancel { id } => {
                            let mut found = false;
                            if incoming.as_ref().is_some_and(|i| i.offer.id == id) {
                                self.clear_incoming(live, incoming.take()).await;
                                found = true;
                            }
                            if outgoing.as_ref().is_some_and(|o| o.sender.offer().id == id) {
                                *outgoing = None;
                                found = true;
                            }
                            if !found {
                                return Err("无效的取消确认。".into());
                            }
                            if let Some(token) = live.token(&id) {
                                token.cancelled.store(true, Ordering::SeqCst);
                            }
                            self.offers
                                .lock()
                                .unwrap()
                                .remove(&(live.session.id().into(), id.clone()));
                            live.update(&id, "cancelled", None, None);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    async fn actor(self: Arc<Self>, live: Arc<Live>, mut commands: mpsc::Receiver<Command>) {
        let (tx, mut frames) = mpsc::channel(4);
        let reader = live.session.clone();
        let receive = tokio::spawn(async move {
            loop {
                let frame = tokio::select! {r=reader.receive()=>r,_=reader.wait_closed()=>break};
                if matches!(frame, Err(farsail_transport::Error::Timeout)) && reader.is_open_now() {
                    continue;
                }
                let failed = frame.is_err();
                if tx.send(frame).await.is_err() || failed {
                    break;
                }
            }
        });
        let mut outgoing: Option<Outgoing> = None;
        let mut incoming: Option<Incoming> = None;
        let result = self
            .run_actor(
                &live,
                &mut commands,
                &mut frames,
                &mut outgoing,
                &mut incoming,
            )
            .await;
        receive.abort();
        live.stop();
        let error = result.err();
        *live.error.lock().unwrap() = error.clone();
        for row in live.transfers.lock().unwrap().iter_mut() {
            if matches!(row["state"].as_str(), Some("offered" | "transferring")) {
                row["state"] = json!(if error.is_some() {
                    "failed"
                } else {
                    "cancelled"
                });
                row["error"] = json!(error);
            }
        }
        self.offers
            .lock()
            .unwrap()
            .retain(|(id, _), _| id != live.session.id());
        self.clear_incoming(&live, incoming.take()).await;
        drop(outgoing);
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Accept(_, receiver, reply) => {
                    self.discard(receiver).await;
                    let _ = reply.send(Err("文件连接已结束。".into()));
                }
                Command::Send(_, reply) | Command::Reject(_, reply) | Command::Cancel(_, reply) => {
                    let _ = reply.send(Err("文件连接已结束。".into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecureStore;
    use farsail_transport::{Authority, Claims, Config, Transport};

    struct EmptyStore;
    impl SecureStore for EmptyStore {
        fn read(&self, _: &str) -> crate::Result<Option<Vec<u8>>> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &[u8]) -> crate::Result<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> crate::Result<()> {
            Ok(())
        }
    }

    struct FileAuthority {
        source: String,
        target: String,
    }
    impl Authority for FileAuthority {
        async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
            if id != "files-actor-test" || token != "files-actor-grant" {
                return Err(farsail_transport::Error::Denied);
            }
            Ok(Claims {
                session_id: id.into(),
                permission: RemotePermission::Files,
                source_public_key: self.source.clone(),
                target_public_key: self.target.clone(),
                nonce: "09".repeat(32),
                expires_in: 30,
                checked_at: Instant::now(),
            })
        }
        async fn issue(&self, id: &str, _: bool) -> farsail_transport::Result<String> {
            if id != "files-actor-test" {
                return Err(farsail_transport::Error::Denied);
            }
            Ok("files-actor-grant".into())
        }
    }

    async fn reject_out_of_order_confirmation(complete_before_finish: bool) {
        let source_endpoint = Transport::bind([91; 32], Config::default()).await.unwrap();
        let target_endpoint = Transport::bind([92; 32], Config::default()).await.unwrap();
        let authority = Arc::new(FileAuthority {
            source: hex::encode(source_endpoint.id().as_bytes()),
            target: hex::encode(target_endpoint.id().as_bytes()),
        });
        let accept = tokio::spawn({
            let target_endpoint = target_endpoint.clone();
            let authority = authority.clone();
            async move { target_endpoint.accept(authority).await.unwrap() }
        });
        let session = source_endpoint
            .connect(
                target_endpoint.addr(),
                "files-actor-test",
                RemotePermission::Files,
                authority,
            )
            .await
            .unwrap();
        let peer = accept.await.unwrap();
        let runtime = NativeFiles::new(Arc::new(NativeClient::new(Arc::new(EmptyStore)).unwrap()));
        runtime.attach(session, false);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.bin");
        std::fs::write(&source, vec![19; files::CHUNK_SIZE * 2 + 1]).unwrap();
        runtime.send_path("files-actor-test", source).await.unwrap();
        let frame = peer.receive().await.unwrap();
        let Message::Offer(offer) = Message::decode(&frame.bytes).unwrap() else {
            panic!("expected offer")
        };
        let confirmation = if complete_before_finish {
            peer.send(Frame {
                channel: Channel::File,
                bytes: Message::Accept {
                    id: offer.id.clone(),
                }
                .encode()
                .unwrap(),
            })
            .await
            .unwrap();
            let chunk = peer.receive().await.unwrap();
            assert!(matches!(
                Message::decode(&chunk.bytes).unwrap(),
                Message::Chunk { .. }
            ));
            Message::Complete {
                id: offer.id.clone(),
            }
        } else {
            Message::ChunkAck {
                id: offer.id.clone(),
                offset: 0,
            }
        };
        peer.send(Frame {
            channel: Channel::File,
            bytes: confirmation.encode().unwrap(),
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let status = runtime.status(Some("files-actor-test")).unwrap();
                if status["state"] == "closed" && status["transfers"][0]["state"] == "failed" {
                    assert_ne!(status["transfers"][0]["state"], "completed");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        runtime.stop_all();
    }

    #[tokio::test]
    async fn chunk_ack_without_receiver_acceptance_is_rejected() {
        reject_out_of_order_confirmation(false).await;
    }
    #[tokio::test]
    async fn complete_before_finish_is_rejected() {
        reject_out_of_order_confirmation(true).await;
    }
}
