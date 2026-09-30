//! Device-bound encrypted QUIC with coordinator-validated short leases.
use farsail_core::RemotePermission;
use iroh::{
    Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey, Watcher,
    endpoint::{Connection, Incoming, PortmapperConfig, presets},
};
use iroh_relay::tls::CaTlsConfig;
use rustls_pki_types::CertificateDer;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    net::SocketAddr,
    sync::{
        Arc, Mutex as StdMutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub const ALPN: &[u8] = b"farsail/session/1";
const TIMEOUT: Duration = Duration::from_secs(10);
const MEDIA_TIMEOUT: Duration = Duration::from_millis(400);
fn media_timeout(bytes: usize) -> Duration {
    MEDIA_TIMEOUT + Duration::from_millis((bytes.saturating_sub(1_000_000) / 1000).min(3600) as u64)
}
const LIMIT: usize = 4096;
const DISCOVERY_COOLDOWN: Duration = Duration::from_secs(30);
const DISCOVERY_WINDOW: Duration = Duration::from_secs(8);

#[derive(Default)]
struct Discovery {
    attempted: Option<Instant>,
    failed: bool,
}
#[cfg(test)]
const LEASE_POLL: Duration = Duration::from_secs(1);
#[cfg(not(test))]
const LEASE_POLL: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport: {0}")]
    Io(String),
    #[error("grant, identity or permission rejected")]
    Denied,
    #[error("session closed or expired")]
    Closed,
    #[error("invalid frame")]
    Frame,
    #[error("transport timeout")]
    Timeout,
}
pub type Result<T> = std::result::Result<T, Error>;
fn io(e: impl std::fmt::Display) -> Error {
    Error::Io(e.to_string())
}
async fn within<T>(f: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(TIMEOUT, f)
        .await
        .map_err(|_| Error::Timeout)?
}

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub relay: Option<RelayUrl>,
    pub force_relay: bool,
    /// DER root certificates for a private TLS relay. Empty uses normal WebPKI.
    pub relay_ca_der: Vec<Vec<u8>>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:0".parse().unwrap(),
            relay: None,
            force_relay: false,
            relay_ca_der: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub session_id: String,
    pub permission: RemotePermission,
    pub source_public_key: String,
    pub target_public_key: String,
    pub nonce: String,
    pub expires_in: i64,
    #[serde(skip, default = "Instant::now")]
    pub checked_at: Instant,
}
impl Claims {
    fn deadline(&self) -> Result<Instant> {
        if self.expires_in <= 0 || self.expires_in > 30 {
            return Err(Error::Denied);
        }
        let deadline = self.checked_at + Duration::from_secs(self.expires_in as u64);
        if deadline <= Instant::now() {
            return Err(Error::Closed);
        }
        Ok(deadline)
    }
}
/// The native client implements this using device credentials. No token is sent to JS.
pub trait Authority: Send + Sync + 'static {
    fn inspect(
        &self,
        id: &str,
        token: &str,
    ) -> impl std::future::Future<Output = Result<Claims>> + Send;
    fn issue(
        &self,
        id: &str,
        renew: bool,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
    fn abandon(&self, _id: &str) -> impl std::future::Future<Output = ()> + Send {
        async {}
    }
}

#[derive(Default)]
struct Replay {
    set: HashSet<String>,
    queue: VecDeque<String>,
}
impl Replay {
    fn insert(&mut self, id: &str) -> bool {
        if self.set.contains(id) {
            return false;
        }
        if self.queue.len() == 4096
            && let Some(old) = self.queue.pop_front()
        {
            self.set.remove(&old);
        }
        self.queue.push_back(id.to_owned());
        self.set.insert(id.to_owned());
        true
    }
}

#[derive(Clone)]
pub struct Transport {
    endpoint: Endpoint,
    replay: Arc<Mutex<Replay>>,
    configured_relay: Option<RelayUrl>,
    relay_config: Option<Arc<iroh_relay::RelayConfig>>,
    force_relay: bool,
    discovery: Arc<Mutex<Discovery>>,
}
impl Transport {
    pub async fn bind(secret: [u8; 32], config: Config) -> Result<Self> {
        let relay_config = config
            .relay
            .clone()
            .map(iroh_relay::RelayConfig::from)
            .map(Arc::new);
        Self::bind_with_relay_config(secret, config, relay_config).await
    }
    async fn bind_with_relay_config(
        secret: [u8; 32],
        config: Config,
        relay_config: Option<Arc<iroh_relay::RelayConfig>>,
    ) -> Result<Self> {
        if config.force_relay && config.relay.is_none() {
            return Err(Error::Denied);
        }
        if config.relay.as_ref().is_some_and(|u| {
            u.scheme() != "https"
                || !u.username().is_empty()
                || u.password().is_some()
                || u.query().is_some()
                || u.fragment().is_some()
                || u.path() != "/"
        }) {
            return Err(Error::Denied);
        }
        let configured_relay = config.relay.clone();
        let mut b = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&secret))
            .alpns(vec![ALPN.to_vec()])
            .clear_address_lookup()
            .portmapper_config(PortmapperConfig::Disabled)
            .relay_mode(
                relay_config
                    .clone()
                    .map(|c| RelayMode::Custom([c].into_iter().collect()))
                    .unwrap_or(RelayMode::Disabled),
            );
        if !config.relay_ca_der.is_empty() {
            b = b.ca_tls_config(CaTlsConfig::custom_roots(
                config.relay_ca_der.into_iter().map(CertificateDer::from),
            ));
        }
        if config.force_relay {
            b = b.clear_ip_transports();
        } else {
            b = b.clear_ip_transports().bind_addr(config.bind).map_err(io)?;
        }
        let endpoint = b.bind().await.map_err(io)?;
        Ok(Self {
            endpoint,
            replay: Arc::new(Mutex::new(Replay::default())),
            configured_relay,
            relay_config,
            force_relay: config.force_relay,
            discovery: Arc::new(Mutex::new(Discovery::default())),
        })
    }
    pub fn id(&self) -> EndpointId {
        self.endpoint.id()
    }
    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }
    pub async fn wait_online(&self) {
        self.endpoint.online().await
    }
    pub async fn close(&self) {
        self.endpoint.close().await
    }
    pub async fn wait_closed(&self) {
        self.endpoint.closed().await;
    }
    pub fn watch_addr(&self) -> impl Watcher<Value = EndpointAddr> + use<> {
        self.endpoint.watch_addr()
    }
    /// Schedule a complete QAD report on the existing endpoint. Iroh 1.2.0 treats
    /// re-inserting the identical relay config as a major discovery update.
    /// This does not remove a relay, rotate identity, or recreate connections.
    /// The actor owns the report; success here means scheduled, not UDP success.
    pub async fn refresh_discovery(&self) -> Result<bool> {
        if self.endpoint.is_closed() {
            return Err(Error::Closed);
        }
        let Some(config) = self.relay_config.as_ref().filter(|_| !self.force_relay) else {
            return Ok(false);
        };
        let Ok(mut state) = self.discovery.try_lock() else {
            return Ok(false);
        };
        if state
            .attempted
            .is_some_and(|t| t.elapsed() < DISCOVERY_COOLDOWN)
        {
            return Ok(false);
        }
        state.attempted = Some(Instant::now());
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            self.endpoint
                .insert_relay(config.url.clone(), config.clone()),
        )
        .await;
        state.failed = result.is_err() || self.endpoint.is_closed();
        if state.failed {
            return Err(if self.endpoint.is_closed() {
                Error::Closed
            } else {
                Error::Timeout
            });
        }
        Ok(true)
    }
    /// Bounded public scheduling state; deliberately does not infer NAT/QAD results.
    pub async fn discovery_state(&self, path: &str) -> &'static str {
        if self.force_relay {
            return "forced_relay";
        }
        if path == "direct" {
            return "direct";
        }
        if self.relay_config.is_none() {
            return "unavailable";
        }
        let state = self.discovery.lock().await;
        if state.failed {
            "refresh_failed"
        } else if state
            .attempted
            .is_some_and(|t| t.elapsed() < DISCOVERY_WINDOW)
        {
            "trying_direct"
        } else {
            "relay_fallback"
        }
    }

    pub async fn connect<A: Authority>(
        &self,
        addr: EndpointAddr,
        id: &str,
        permission: RemotePermission,
        authority: Arc<A>,
    ) -> Result<Session> {
        if addr
            .relay_urls()
            .any(|u| Some(u) != self.configured_relay.as_ref())
        {
            return Err(Error::Denied);
        }
        let target = addr.id;
        // Discovery failure must not prevent the encrypted relay fallback.
        let _ = self.refresh_discovery().await;
        let conn = within(async { self.endpoint.connect(addr, ALPN).await.map_err(io) }).await?;
        if conn.remote_id() != target {
            return Err(Error::Denied);
        }
        let (mut tx, mut rx) = within(async { conn.open_bi().await.map_err(io) }).await?;
        let hello = serde_json::to_vec(&Hello {
            id: id.into(),
            permission,
        })
        .map_err(io)?;
        within(async {
            tx.write_all(&hello).await.map_err(io)?;
            tx.finish().map_err(io)
        })
        .await?;
        let bytes = within(async { rx.read_to_end(LIMIT).await.map_err(io) }).await?;
        let offer: Offer = serde_json::from_slice(&bytes).map_err(|_| Error::Denied)?;
        let claims = within(authority.inspect(id, &offer.token)).await?;
        validate(&claims, id, permission, self.id(), target)?;
        let (mut tx, mut rx) = within(async { conn.open_bi().await.map_err(io) }).await?;
        let proof = serde_json::to_vec(&Proof {
            id: id.into(),
            nonce: claims.nonce.clone(),
        })
        .map_err(io)?;
        within(async {
            tx.write_all(&proof).await.map_err(io)?;
            tx.finish().map_err(io)
        })
        .await?;
        if within(async { rx.read_to_end(16).await.map_err(io) }).await? != b"ok" {
            return Err(Error::Denied);
        }
        let session = Session::new(
            conn,
            id.into(),
            permission,
            claims.deadline()?,
            self.id(),
            target,
        )?;
        session.watch_source(authority, offer.token);
        Ok(session)
    }
    pub async fn accept<A: Authority>(&self, authority: Arc<A>) -> Result<Session> {
        let incoming = self.next_incoming().await?;
        self.accept_incoming(incoming, authority).await
    }
    pub async fn next_incoming(&self) -> Result<Incoming> {
        self.endpoint.accept().await.ok_or(Error::Closed)
    }
    pub async fn accept_incoming<A: Authority>(
        &self,
        incoming: Incoming,
        authority: Arc<A>,
    ) -> Result<Session> {
        let _ = self.refresh_discovery().await;
        let conn = within(async { incoming.accept().map_err(io)?.await.map_err(io) }).await?;
        let (mut tx, mut rx) = within(async { conn.accept_bi().await.map_err(io) }).await?;
        let bytes = within(async { rx.read_to_end(LIMIT).await.map_err(io) }).await?;
        let hello: Hello = serde_json::from_slice(&bytes).map_err(|_| Error::Denied)?;
        if hello.id.len() > 64 {
            return Err(Error::Denied);
        }
        let token = within(authority.issue(&hello.id, false)).await?;
        let claims = within(authority.inspect(&hello.id, &token)).await?;
        validate(
            &claims,
            &hello.id,
            hello.permission,
            conn.remote_id(),
            self.id(),
        )?;
        if !self.replay.lock().await.insert(&hello.id) {
            return Err(Error::Denied);
        }
        let session_id = hello.id.clone();
        let result = async {
            let offer = serde_json::to_vec(&Offer {
                token: token.clone(),
            })
            .map_err(io)?;
            within(async {
                tx.write_all(&offer).await.map_err(io)?;
                tx.finish().map_err(io)
            })
            .await?;
            let (mut tx, mut rx) = within(async { conn.accept_bi().await.map_err(io) }).await?;
            let bytes = within(async { rx.read_to_end(LIMIT).await.map_err(io) }).await?;
            let proof: Proof = serde_json::from_slice(&bytes).map_err(|_| Error::Denied)?;
            if proof.id != hello.id || proof.nonce != claims.nonce {
                return Err(Error::Denied);
            }
            within(async {
                tx.write_all(b"ok").await.map_err(io)?;
                tx.finish().map_err(io)
            })
            .await?;
            let source = conn.remote_id();
            Session::new(
                conn,
                hello.id,
                hello.permission,
                claims.deadline()?,
                source,
                self.id(),
            )
        }
        .await;
        if result.is_err() {
            authority.abandon(&session_id).await;
        }
        let session = result?;
        session.watch_host(authority, token);
        Ok(session)
    }
}

#[derive(Serialize, Deserialize)]
struct Hello {
    id: String,
    permission: RemotePermission,
}
#[derive(Serialize, Deserialize)]
struct Offer {
    token: String,
}
#[derive(Serialize, Deserialize)]
struct Proof {
    id: String,
    nonce: String,
}
fn key(s: &str) -> Result<EndpointId> {
    let b = hex::decode(s).map_err(|_| Error::Denied)?;
    EndpointId::try_from(b.as_slice()).map_err(|_| Error::Denied)
}
fn validate(
    c: &Claims,
    id: &str,
    p: RemotePermission,
    source: EndpointId,
    target: EndpointId,
) -> Result<()> {
    if c.session_id != id
        || c.permission != p
        || c.deadline().is_err()
        || c.nonce.len() != 64
        || key(&c.source_public_key)? != source
        || key(&c.target_public_key)? != target
    {
        return Err(Error::Denied);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Control,
    Media,
    File,
}
impl Channel {
    fn tag(self) -> u8 {
        match self {
            Self::Control => 1,
            Self::Media => 2,
            Self::File => 3,
        }
    }
    fn decode(n: u8) -> Result<Self> {
        match n {
            1 => Ok(Self::Control),
            2 => Ok(Self::Media),
            3 => Ok(Self::File),
            _ => Err(Error::Frame),
        }
    }
    fn max(self) -> usize {
        match self {
            Self::Control => 64 * 1024,
            Self::Media => 4_000_000 + 49,
            Self::File => 256 * 1024,
        }
    }
    fn permits(self, p: RemotePermission) -> bool {
        match self {
            Self::Control => p == RemotePermission::Control,
            Self::Media => p != RemotePermission::Files,
            Self::File => p == RemotePermission::Files,
        }
    }
}
#[derive(Debug)]
pub struct Frame {
    pub channel: Channel,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct Session {
    conn: Connection,
    id: String,
    permission: RemotePermission,
    source: EndpointId,
    target: EndpointId,
    deadline: Arc<RwLock<Instant>>,
    closed: Arc<AtomicBool>,
    end_reason: Arc<StdMutex<Option<&'static str>>>,
}
impl Session {
    fn new(
        conn: Connection,
        id: String,
        permission: RemotePermission,
        deadline: Instant,
        source: EndpointId,
        target: EndpointId,
    ) -> Result<Self> {
        if deadline <= Instant::now() {
            return Err(Error::Closed);
        }
        let session = Self {
            conn,
            id,
            permission,
            source,
            target,
            deadline: Arc::new(RwLock::new(deadline)),
            closed: Arc::new(AtomicBool::new(false)),
            end_reason: Arc::new(StdMutex::new(None)),
        };
        session.watch_expiry();
        Ok(session)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn permission(&self) -> RemotePermission {
        self.permission
    }
    pub fn is_host(&self, local: EndpointId) -> bool {
        self.target == local
    }
    pub fn stable_id(&self) -> usize {
        self.conn.stable_id()
    }
    /// Both ends derive the same short comparison code from this TLS connection.
    /// The context includes the approved session and exact permission; no grant is displayed.
    pub fn verification_code(&self) -> Result<String> {
        let context = format!(
            "{}:{}:{}:{}",
            self.id,
            self.permission.as_str(),
            self.source,
            self.target
        );
        let mut out = [0u8; 8];
        self.conn
            .export_keying_material(&mut out, b"farsail/verify/v1", context.as_bytes())
            .map_err(|_| Error::Io("TLS exporter unavailable".into()))?;
        Ok(format!(
            "{:04X}-{:04X}-{:04X}-{:04X}",
            u16::from_be_bytes([out[0], out[1]]),
            u16::from_be_bytes([out[2], out[3]]),
            u16::from_be_bytes([out[4], out[5]]),
            u16::from_be_bytes([out[6], out[7]])
        ))
    }
    pub async fn wait_closed(&self) {
        let _ = self.conn.closed().await;
    }
    pub fn path(&self) -> (&'static str, Option<u64>) {
        self.conn
            .paths()
            .iter()
            .find(|x| x.is_selected())
            .map(|x| {
                (
                    if x.is_relay() {
                        "relay"
                    } else if x.is_ip() {
                        "direct"
                    } else {
                        "connecting"
                    },
                    Some(x.rtt().as_millis() as u64),
                )
            })
            .unwrap_or(("connecting", None))
    }
    pub fn close(&self) {
        // iroh replaces a peer close with LocallyClosed during cleanup. Preserve
        // the bounded peer cause first, otherwise recovery loses its eligibility.
        let reason = self.end_reason();
        self.end_reason.lock().unwrap().get_or_insert(reason);
        self.closed.store(true, Ordering::SeqCst);
        self.conn.close(0u32.into(), b"closed");
    }
    pub fn close_with_reason(&self, reason: &'static str) {
        let mut saved = self.end_reason.lock().unwrap();
        if saved.is_none() {
            *saved = Some(reason);
        }
        self.closed.store(true, Ordering::SeqCst);
        self.conn.close(0u32.into(), reason.as_bytes());
    }
    /// Only bounded public codes cross the UI; never transport internals or tokens.
    pub fn end_reason(&self) -> &'static str {
        if let Some(reason) = *self.end_reason.lock().unwrap() {
            return reason;
        }
        use iroh::endpoint::ConnectionError;
        match self.conn.close_reason() {
            Some(ConnectionError::TimedOut) => "network_timeout",
            Some(ConnectionError::ApplicationClosed(c)) => match c.reason.as_ref() {
                b"media_timeout" => "media_timeout",
                b"input_failed" => "input_failed",
                b"input_sequence" => "input_sequence",
                b"lease_expired" => "lease_expired",
                b"authorization_failed" => "authorization_failed",
                b"renewal_failed" => "renewal_failed",
                b"capture_failed" => "capture_failed",
                b"stopped" => "stopped",
                _ => "session_ended",
            },
            _ => "session_ended",
        }
    }
    pub async fn is_open(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
            && Instant::now() < *self.deadline.read().unwrap()
            && self.conn.close_reason().is_none()
    }
    /// Synchronous gate for system input while holding the local input lock.
    pub fn is_open_now(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
            && self
                .deadline
                .read()
                .is_ok_and(|deadline| Instant::now() < *deadline)
            && self.conn.close_reason().is_none()
    }
    fn watch_expiry(&self) {
        let s = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            loop {
                tick.tick().await;
                if !s.is_open().await {
                    if s.conn.close_reason().is_none() && !s.closed.load(Ordering::SeqCst) {
                        s.close_with_reason("lease_expired");
                    }
                    break;
                }
            }
        });
    }
    async fn guard(&self, c: Channel) -> Result<()> {
        if !c.permits(self.permission) {
            return Err(Error::Denied);
        }
        if !self.is_open().await {
            self.close();
            return Err(Error::Closed);
        }
        Ok(())
    }
    pub async fn send(&self, frame: Frame) -> Result<()> {
        self.guard(frame.channel).await?;
        if frame.bytes.len() > frame.channel.max() {
            return Err(Error::Frame);
        }
        let timeout = if frame.channel == Channel::Media {
            media_timeout(frame.bytes.len())
        } else {
            TIMEOUT
        };
        let mut tx = tokio::time::timeout(timeout, self.conn.open_uni())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(io)?;
        let mut head = [0; 6];
        head[0] = 1;
        head[1] = frame.channel.tag();
        head[2..].copy_from_slice(&(frame.bytes.len() as u32).to_be_bytes());
        let result = tokio::time::timeout(timeout, async {
            tx.write_all(&head).await.map_err(io)?;
            tx.write_all(&frame.bytes).await.map_err(io)?;
            tx.finish().map_err(io)
        })
        .await;
        match result {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                let _ = tx.reset(0u32.into());
                return Err(e);
            }
            Err(_) => {
                let _ = tx.reset(0u32.into());
                return Err(Error::Timeout);
            }
        }
        self.guard(frame.channel).await
    }
    pub async fn receive(&self) -> Result<Frame> {
        if !self.is_open().await {
            self.close();
            return Err(Error::Closed);
        }
        let mut rx = within(async { self.conn.accept_uni().await.map_err(io) }).await?;
        let mut head = [0; 6];
        match tokio::time::timeout(Duration::from_secs(1), rx.read_exact(&mut head)).await {
            Ok(Ok(_)) => (),
            Ok(Err(_)) | Err(_) => return Err(Error::Timeout),
        }
        if head[0] != 1 {
            self.close();
            return Err(Error::Frame);
        }
        let channel = match Channel::decode(head[1]) {
            Ok(channel) => channel,
            Err(error) => {
                self.close();
                return Err(error);
            }
        };
        self.guard(channel).await?;
        let size = u32::from_be_bytes(head[2..].try_into().unwrap()) as usize;
        if size > channel.max() {
            self.close();
            return Err(Error::Frame);
        }
        let mut bytes = vec![0; size];
        let read_timeout = if channel == Channel::Media {
            media_timeout(size)
        } else {
            TIMEOUT
        };
        match tokio::time::timeout(read_timeout, rx.read_exact(&mut bytes)).await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) if channel == Channel::Media => {
                let _ = e;
                return Err(Error::Timeout);
            }
            Ok(Err(e)) => return Err(io(e)),
            Err(_) => {
                let _ = rx.stop(0u32.into());
                return Err(Error::Timeout);
            }
        }
        match tokio::time::timeout(read_timeout, rx.read_to_end(0)).await {
            Ok(Ok(_)) => (),
            Ok(Err(_)) if channel == Channel::Media => return Err(Error::Timeout),
            Ok(Err(_)) => {
                self.close();
                return Err(Error::Frame);
            }
            Err(_) => {
                let _ = rx.stop(0u32.into());
                return Err(Error::Timeout);
            }
        }
        self.guard(channel).await?;
        Ok(Frame { channel, bytes })
    }
    fn watch_source<A: Authority>(&self, authority: Arc<A>, mut token: String) {
        let s = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(LEASE_POLL);
            let mut failed_at: Option<Instant> = None;
            loop {
                if !s.is_open().await {
                    break;
                }
                tokio::select! {
                    biased;
                    incoming = s.conn.accept_bi() => {
                        let Ok((mut tx, mut rx)) = incoming else { break };
                        let Ok(bytes) = within(async { rx.read_to_end(LIMIT).await.map_err(io) }).await else { break };
                        let Ok(offer) = serde_json::from_slice::<Offer>(&bytes) else { break };
                        let Ok(c) = authority.inspect(&s.id, &offer.token).await else { break };
                        if validate(&c, &s.id, s.permission, s.source, s.target).is_err() { break; }
                        let Ok(deadline) = c.deadline() else { break };
                        token = offer.token;
                        *s.deadline.write().unwrap() = deadline;
                        failed_at = None;
                        if tx.write_all(b"ok").await.is_err() || tx.finish().is_err() { break; }
                    }
                    _ = tick.tick() => {
                        match authority.inspect(&s.id, &token).await {
                            Ok(c) if validate(&c,&s.id,s.permission,s.source,s.target).is_ok() => {
                                if let Ok(deadline) = c.deadline() { *s.deadline.write().unwrap() = deadline; failed_at = None; }
                                else { break; }
                            }
                            _ => {
                                // Renewal rotates the grant before its message can arrive.
                                // Do not extend the old local lease during this brief race.
                                match failed_at { None => failed_at = Some(Instant::now()), Some(t) if t.elapsed() >= Duration::from_secs(2) => break, _ => {} }
                            }
                        }
                    }
                }
            }
            if s.is_open().await {
                s.close_with_reason("authorization_failed");
            }
        });
    }
    fn watch_host<A: Authority>(&self, authority: Arc<A>, mut token: String) {
        let s = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(LEASE_POLL);
            let mut n = 0;
            loop {
                tick.tick().await;
                if !s.is_open().await {
                    break;
                }
                n += 1;
                if n >= 3 {
                    n = 0;
                    let Ok(next) = authority.issue(&s.id, true).await else {
                        break;
                    };
                    let Ok((mut tx, mut rx)) = s.conn.open_bi().await else {
                        break;
                    };
                    let bytes = serde_json::to_vec(&Offer {
                        token: next.clone(),
                    })
                    .unwrap();
                    if tx.write_all(&bytes).await.is_err() || tx.finish().is_err() {
                        break;
                    }
                    if within(async { rx.read_to_end(16).await.map_err(io) })
                        .await
                        .ok()
                        .as_deref()
                        != Some(b"ok")
                    {
                        break;
                    }
                    token = next;
                }
                match authority.inspect(&s.id, &token).await {
                    Ok(c) if validate(&c, &s.id, s.permission, s.source, s.target).is_ok() => {
                        if let Ok(deadline) = c.deadline() {
                            *s.deadline.write().unwrap() = deadline;
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            if s.is_open().await {
                s.close_with_reason("renewal_failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestAuthority {
        source: EndpointId,
        target: EndpointId,
        current: Mutex<u32>,
        revoked: AtomicBool,
        stalled: AtomicBool,
    }
    impl Authority for TestAuthority {
        async fn inspect(&self, id: &str, token: &str) -> Result<Claims> {
            if self.stalled.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
            if self.revoked.load(Ordering::SeqCst)
                || id != "test-session"
                || token != format!("grant-{}", *self.current.lock().await)
            {
                return Err(Error::Denied);
            }
            Ok(Claims {
                session_id: id.into(),
                permission: RemotePermission::View,
                source_public_key: hex::encode(self.source.as_bytes()),
                target_public_key: hex::encode(self.target.as_bytes()),
                nonce: "ab".repeat(32),
                expires_in: 8,
                checked_at: Instant::now(),
            })
        }
        async fn issue(&self, _: &str, renew: bool) -> Result<String> {
            let mut n = self.current.lock().await;
            if renew {
                *n += 1;
            }
            Ok(format!("grant-{n}"))
        }
    }
    async fn pair() -> (Transport, Transport, Arc<TestAuthority>) {
        let source = Transport::bind([1; 32], Config::default()).await.unwrap();
        let target = Transport::bind([2; 32], Config::default()).await.unwrap();
        let authority = Arc::new(TestAuthority {
            source: source.id(),
            target: target.id(),
            current: Mutex::new(0),
            revoked: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
        });
        (source, target, authority)
    }
    #[tokio::test]
    async fn qad_failure_recovers_on_same_endpoint_without_interrupting_session() {
        use iroh_relay::{
            RelayConfig as ClientRelayConfig, RelayQuicConfig,
            server::{CertConfig, QuicConfig, RelayConfig, Server, ServerConfig, TlsConfig},
        };
        use rustls_pki_types::PrivatePkcs8KeyDer;
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = certified.cert.der().clone();
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.clone()],
            PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der()).into(),
        )
        .unwrap();
        let mut relay = RelayConfig::new("127.0.0.1:0".parse::<SocketAddr>().unwrap());
        relay.tls = Some(TlsConfig::new(
            "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            CertConfig::Manual {
                server_config: tls.clone(),
            },
        ));
        let mut relay_server_config = ServerConfig::default();
        relay_server_config.relay = Some(relay);
        let server = Server::spawn(relay_server_config).await.unwrap();
        let url: RelayUrl = format!("https://localhost:{}/", server.https_addr().unwrap().port())
            .parse()
            .unwrap();
        // Reserve a UDP black hole, then replace it with a real trusted QAD server.
        let black_hole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let qad_addr = black_hole.local_addr().unwrap();
        let relay_config = Arc::new(ClientRelayConfig::new(
            url.clone(),
            Some(RelayQuicConfig::new(qad_addr.port())),
        ));
        let config = Config {
            relay: Some(url.clone()),
            relay_ca_der: vec![cert.to_vec()],
            ..Config::default()
        };
        let source =
            Transport::bind_with_relay_config([41; 32], config.clone(), Some(relay_config.clone()))
                .await
                .unwrap();
        let direct_config = config.clone();
        let direct_relay_config = relay_config.clone();
        let target = Transport::bind_with_relay_config(
            [42; 32],
            Config {
                force_relay: true,
                ..config
            },
            Some(relay_config),
        )
        .await
        .unwrap();
        let source_id = source.id();
        let mut reports = source.endpoint.net_report();
        let initial = tokio::time::timeout(Duration::from_secs(15), reports.initialized())
            .await
            .unwrap();
        assert!(!initial.udp_v4 && initial.global_v4.is_none());
        // Read the previous report so updated() observes only later reports.
        reports.get();
        let auth = Arc::new(TestAuthority {
            source: source.id(),
            target: target.id(),
            current: Mutex::new(0),
            revoked: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
        });
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(
                EndpointAddr::new(target.id()).with_relay_url(url),
                "test-session",
                RemotePermission::View,
                auth.clone(),
            )
            .await
            .unwrap();
        let dst = host.await.unwrap();
        assert_eq!(src.path().0, "relay");
        let connection_id = src.stable_id();
        let media = tokio::spawn({
            let src = src.clone();
            let dst = dst.clone();
            async move {
                for n in 0..80u8 {
                    src.send(Frame {
                        channel: Channel::Media,
                        bytes: vec![n; 1024],
                    })
                    .await
                    .unwrap();
                    assert_eq!(dst.receive().await.unwrap().bytes, vec![n; 1024]);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        drop(black_hole);
        let mut qad = QuicConfig::new(qad_addr);
        qad.server_config = Some(tls);
        let mut qad_server_config = ServerConfig::default();
        qad_server_config.quic = Some(qad);
        let qad_server = Server::spawn(qad_server_config).await.unwrap();
        // Clear only the test cooldown, not discovery state, identity, or connections.
        source.discovery.lock().await.attempted = None;
        target.discovery.lock().await.attempted = None;
        assert!(source.refresh_discovery().await.unwrap());
        assert!(!target.refresh_discovery().await.unwrap());
        let recovered = tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                let report = reports.updated().await.unwrap().unwrap();
                if report.udp_v4 {
                    break report;
                }
            }
        })
        .await
        .unwrap();
        assert!(recovered.global_v4.is_some());
        media.await.unwrap();
        assert_eq!(
            src.path().0,
            "relay",
            "forced peer retains relay during recovery"
        );
        assert_eq!(source.id(), source_id);
        assert_eq!(src.stable_id(), connection_id);
        assert!(src.is_open_now() && dst.is_open_now());
        assert!(
            *auth.current.lock().await >= 1,
            "lease renewal survived refresh"
        );
        assert!(matches!(
            src.send(Frame {
                channel: Channel::Control,
                bytes: vec![]
            })
            .await,
            Err(Error::Denied)
        ));
        // Concurrent requests share a cooldown; no per-session/poll discovery storm.
        let (a, b) = tokio::join!(source.refresh_discovery(), source.refresh_discovery());
        assert!(!a.unwrap() && !b.unwrap());
        // The refreshed source can establish a real authenticated direct session
        // while the original authenticated relay connection stays alive.
        let direct =
            Transport::bind_with_relay_config([45; 32], direct_config, Some(direct_relay_config))
                .await
                .unwrap();
        let direct_auth = Arc::new(TestAuthority {
            source: source.id(),
            target: direct.id(),
            current: Mutex::new(0),
            revoked: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
        });
        let host = tokio::spawn({
            let direct = direct.clone();
            let auth = direct_auth.clone();
            async move { direct.accept(auth).await.unwrap() }
        });
        let direct_src = source
            .connect(
                direct.addr(),
                "test-session",
                RemotePermission::View,
                direct_auth,
            )
            .await
            .unwrap();
        let direct_dst = host.await.unwrap();
        assert_eq!(direct_src.path().0, "direct");
        let direct_connection_id = direct_src.stable_id();
        source.discovery.lock().await.attempted = None;
        assert!(source.refresh_discovery().await.unwrap());
        direct_src
            .send(Frame {
                channel: Channel::Media,
                bytes: b"direct after recovery".to_vec(),
            })
            .await
            .unwrap();
        assert_eq!(
            direct_dst.receive().await.unwrap().bytes,
            b"direct after recovery"
        );
        assert_eq!(direct_src.stable_id(), direct_connection_id);
        assert_eq!(
            direct_src.path().0,
            "direct",
            "refresh preserves an existing direct path"
        );
        assert!(src.is_open_now() && dst.is_open_now());
        direct_src.close();
        direct_dst.close();
        direct.close().await;
        src.close();
        dst.close();
        source.close().await;
        target.close().await;
        assert!(matches!(
            source.refresh_discovery().await,
            Err(Error::Closed)
        ));
        qad_server.shutdown().await.unwrap();
        server.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn discovery_skips_no_relay_and_forced_relay() {
        let source = Transport::bind([43; 32], Config::default()).await.unwrap();
        assert!(!source.refresh_discovery().await.unwrap());
        assert_eq!(source.discovery_state("relay").await, "unavailable");
        source.close().await;
        let forced = Transport::bind(
            [44; 32],
            Config {
                force_relay: true,
                relay: Some("https://localhost:9/".parse().unwrap()),
                ..Config::default()
            },
        )
        .await
        .unwrap();
        assert!(!forced.refresh_discovery().await.unwrap());
        assert_eq!(forced.discovery_state("relay").await, "forced_relay");
        assert!(forced.discovery.lock().await.attempted.is_none());
        forced.close().await;
    }
    #[tokio::test]
    async fn bounded_4k_payload_can_cross_authenticated_media_channel() {
        let (source, target, auth) = pair().await;
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(target.addr(), "test-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let dst = host.await.unwrap();
        let payload = vec![73; Channel::Media.max()];
        let (sent, received) = tokio::join!(
            src.send(Frame {
                channel: Channel::Media,
                bytes: payload.clone()
            }),
            dst.receive()
        );
        sent.unwrap();
        assert_eq!(received.unwrap().bytes, payload);
        assert!(
            src.send(Frame {
                channel: Channel::Media,
                bytes: vec![0; Channel::Media.max() + 1]
            })
            .await
            .is_err()
        );
        src.close();
        dst.close();
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn direct_authenticated_frames_renewal_and_revocation() {
        let (source, target, auth) = pair().await;
        assert!(target.addr().ip_addrs().all(|a| a.ip().is_loopback()));
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(
                target.addr(),
                "test-session",
                RemotePermission::View,
                auth.clone(),
            )
            .await
            .unwrap();
        let dst = host.await.unwrap();
        let ready = Arc::new(std::sync::Barrier::new(2));
        let writer = std::thread::spawn({
            let ready = ready.clone();
            let deadline = src.deadline.clone();
            move || {
                let mut lease = deadline.write().unwrap();
                ready.wait();
                std::thread::sleep(Duration::from_millis(20));
                *lease += Duration::from_secs(1);
            }
        });
        ready.wait();
        assert!(
            src.is_open_now(),
            "an in-progress renewal is not a closed lease"
        );
        writer.join().unwrap();
        assert_eq!(src.path().0, "direct");
        assert_eq!(
            src.verification_code().unwrap(),
            dst.verification_code().unwrap()
        );
        src.send(Frame {
            channel: Channel::Media,
            bytes: b"screen".to_vec(),
        })
        .await
        .unwrap();
        assert_eq!(dst.receive().await.unwrap().bytes, b"screen");
        dst.send(Frame {
            channel: Channel::Media,
            bytes: b"reply".to_vec(),
        })
        .await
        .unwrap();
        assert_eq!(src.receive().await.unwrap().bytes, b"reply");
        assert!(matches!(
            src.send(Frame {
                channel: Channel::Control,
                bytes: vec![]
            })
            .await,
            Err(Error::Denied)
        ));
        tokio::time::sleep(Duration::from_secs(7)).await;
        assert!(
            *auth.current.lock().await >= 2,
            "at least two grant rotations"
        );
        assert!(src.is_open().await && dst.is_open().await);
        auth.revoked.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!src.is_open().await && !dst.is_open().await);
        assert!(matches!(
            src.send(Frame {
                channel: Channel::Media,
                bytes: vec![]
            })
            .await,
            Err(Error::Closed)
        ));
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn incomplete_earlier_stream_allows_later_frame_and_preserves_close_reason() {
        let (source, target, auth) = pair().await;
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(target.addr(), "test-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let dst = host.await.unwrap();
        let mut earlier = src.conn.open_uni().await.unwrap();
        src.send(Frame {
            channel: Channel::Media,
            bytes: b"sequence-2".to_vec(),
        })
        .await
        .unwrap();
        assert!(matches!(dst.receive().await, Err(Error::Timeout)));
        let _ = earlier.reset(0u32.into());
        assert_eq!(dst.receive().await.unwrap().bytes, b"sequence-2");
        src.close_with_reason("media_timeout");
        dst.wait_closed().await;
        assert_eq!(dst.end_reason(), "media_timeout");
        // Local cleanup must preserve the original actionable cause.
        dst.close();
        assert_eq!(dst.end_reason(), "media_timeout");
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn reset_media_frame_does_not_poison_next_frame() {
        let (source, target, auth) = pair().await;
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(target.addr(), "test-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let dst = host.await.unwrap();
        let mut tx = src.conn.open_uni().await.unwrap();
        tx.write_all(&[1, Channel::Media.tag(), 0, 0, 0, 20, 1, 2, 3])
            .await
            .unwrap();
        let _ = tx.reset(0u32.into());
        let disrupted = dst.receive().await;
        assert!(matches!(disrupted, Err(Error::Timeout)), "{disrupted:?}");
        assert!(dst.is_open().await);
        src.send(Frame {
            channel: Channel::Media,
            bytes: b"fresh".to_vec(),
        })
        .await
        .unwrap();
        assert_eq!(dst.receive().await.unwrap().bytes, b"fresh");
        src.close();
        dst.close();
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn wrong_identity_does_not_consume_valid_grant() {
        let (source, target, auth) = pair().await;
        let wrong = Transport::bind([3; 32], Config::default()).await.unwrap();
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        assert!(
            wrong
                .connect(
                    target.addr(),
                    "test-session",
                    RemotePermission::View,
                    auth.clone()
                )
                .await
                .is_err()
        );
        assert!(host.await.unwrap().is_err());
        assert_eq!(*auth.current.lock().await, 0);
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        assert!(
            source
                .connect(
                    target.addr(),
                    "test-session",
                    RemotePermission::Control,
                    auth.clone()
                )
                .await
                .is_err()
        );
        assert!(host.await.unwrap().is_err());
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        let valid = source
            .connect(
                target.addr(),
                "test-session",
                RemotePermission::View,
                auth.clone(),
            )
            .await
            .unwrap();
        let peer = host.await.unwrap().unwrap();
        assert!(valid.is_open().await && peer.is_open().await);
        valid.close();
        peer.close();
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        assert!(
            source
                .connect(target.addr(), "test-session", RemotePermission::View, auth)
                .await
                .is_err()
        );
        assert!(host.await.unwrap().is_err()); // Replay of an already accepted session.
        wrong.close().await;
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn unapproved_and_invalid_claims_are_rejected() {
        let (source, target, auth) = pair().await;
        auth.revoked.store(true, Ordering::SeqCst);
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        assert!(
            source
                .connect(target.addr(), "test-session", RemotePermission::View, auth)
                .await
                .is_err()
        );
        assert!(host.await.unwrap().is_err());
        let claims = Claims {
            session_id: "x".into(),
            permission: RemotePermission::View,
            source_public_key: hex::encode(source.id().as_bytes()),
            target_public_key: hex::encode(target.id().as_bytes()),
            nonce: "ab".repeat(32),
            expires_in: 30,
            checked_at: Instant::now(),
        };
        assert!(
            validate(
                &claims,
                "x",
                RemotePermission::View,
                source.id(),
                target.id()
            )
            .is_ok()
        );
        let mut wrong = claims.clone();
        wrong.expires_in = 0;
        assert!(
            validate(
                &wrong,
                "x",
                RemotePermission::View,
                source.id(),
                target.id()
            )
            .is_err()
        );
        let mut wrong = claims.clone();
        wrong.nonce = "short".into();
        assert!(
            validate(
                &wrong,
                "x",
                RemotePermission::View,
                source.id(),
                target.id()
            )
            .is_err()
        );
        let mut wrong = claims;
        wrong.permission = RemotePermission::Control;
        assert!(
            validate(
                &wrong,
                "x",
                RemotePermission::View,
                source.id(),
                target.id()
            )
            .is_err()
        );
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn local_deadline_closes_streams_while_authority_is_unreachable() {
        let (source, target, auth) = pair().await;
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(
                target.addr(),
                "test-session",
                RemotePermission::View,
                auth.clone(),
            )
            .await
            .unwrap();
        let dst = host.await.unwrap();
        auth.stalled.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(9)).await;
        assert!(!src.is_open().await && !dst.is_open().await);
        assert!(matches!(
            src.send(Frame {
                channel: Channel::Media,
                bytes: vec![1]
            })
            .await,
            Err(Error::Closed)
        ));
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn malformed_or_oversized_frame_closes_authenticated_session() {
        let (source, target, auth) = pair().await;
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(target.addr(), "test-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let dst = host.await.unwrap();
        let mut raw = src.conn.open_uni().await.unwrap();
        let mut header = vec![1, Channel::Media.tag()];
        header.extend_from_slice(&((Channel::Media.max() + 1) as u32).to_be_bytes());
        raw.write_all(&header).await.unwrap();
        raw.finish().unwrap();
        assert!(matches!(dst.receive().await, Err(Error::Frame)));
        assert!(!dst.is_open().await);
        src.close();
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn forced_tls_relay_carries_authenticated_data() {
        use iroh_relay::server::{CertConfig, RelayConfig, Server, ServerConfig, TlsConfig};
        use rustls_pki_types::PrivatePkcs8KeyDer;
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = certified.cert.der().clone();
        let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key.into())
        .unwrap();
        let mut relay = RelayConfig::new("127.0.0.1:0".parse::<SocketAddr>().unwrap());
        relay.tls = Some(TlsConfig::new(
            "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            CertConfig::Manual { server_config: tls },
        ));
        let mut server_config = ServerConfig::default();
        server_config.relay = Some(relay);
        let server = Server::spawn(server_config).await.unwrap();
        let url: RelayUrl = format!("https://localhost:{}/", server.https_addr().unwrap().port())
            .parse()
            .unwrap();
        let config = Config {
            relay: Some(url),
            force_relay: true,
            relay_ca_der: vec![cert.to_vec()],
            ..Config::default()
        };
        let source = Transport::bind([4; 32], config.clone()).await.unwrap();
        let target = Transport::bind([5; 32], config).await.unwrap();
        tokio::time::timeout(TIMEOUT, source.wait_online())
            .await
            .unwrap();
        tokio::time::timeout(TIMEOUT, target.wait_online())
            .await
            .unwrap();
        assert!(source.addr().ip_addrs().next().is_none());
        assert!(target.addr().ip_addrs().next().is_none());
        let auth = Arc::new(TestAuthority {
            source: source.id(),
            target: target.id(),
            current: Mutex::new(0),
            revoked: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
        });
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let src = source
            .connect(target.addr(), "test-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let dst = host.await.unwrap();
        assert_eq!(src.path().0, "relay");
        src.send(Frame {
            channel: Channel::Media,
            bytes: b"through relay".to_vec(),
        })
        .await
        .unwrap();
        assert_eq!(dst.receive().await.unwrap().bytes, b"through relay");
        src.close();
        dst.close();
        source.close().await;
        target.close().await;
        server.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn silent_handshake_does_not_block_another_connection() {
        let (source, target, auth) = pair().await;
        let silent = Transport::bind([6; 32], Config::default()).await.unwrap();
        let first = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        let silent_conn = silent.endpoint.connect(target.addr(), ALPN).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let second = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await }
        });
        let source_session = tokio::time::timeout(
            Duration::from_secs(5),
            source.connect(target.addr(), "test-session", RemotePermission::View, auth),
        )
        .await
        .unwrap()
        .unwrap();
        let target_session = second.await.unwrap().unwrap();
        assert!(source_session.is_open().await && target_session.is_open().await);
        silent_conn.close(0u32.into(), b"stop");
        assert!(first.await.unwrap().is_err());
        source_session.close();
        target_session.close();
        source.close().await;
        target.close().await;
        silent.close().await;
    }
}
