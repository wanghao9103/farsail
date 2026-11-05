//! Loopback-only, synthetic owned-emulator peer. Never injects into the host OS.
use axum::{
    Json, Router,
    extract::{Request, State},
};
use farsail_core::RemotePermission;
use farsail_media::{FrameMeta, JpegFrame};
use farsail_transport::{Authority, Channel, Claims, Config, Frame, Session, Transport};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
const SOURCE: &str = "10000000-0000-4000-8000-000000000001";
const DESKTOP: &str = "20000000-0000-4000-8000-000000000001";
const PHONE: &str = "20000000-0000-4000-8000-000000000002";
const PENDING: &str = "20000000-0000-4000-8000-000000000003";
struct Remote {
    permission: RemotePermission,
    phone: bool,
    approved: bool,
    revoked: bool,
    open: bool,
    inputs: Vec<Value>,
    acks: u64,
}
struct Fixture {
    source: Mutex<String>,
    target: String,
    address: Value,
    requests: Mutex<HashMap<String, Remote>>,
    next: AtomicU64,
}
impl Authority for Fixture {
    async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
        let rows = self.requests.lock().unwrap();
        let r = rows.get(id).ok_or(farsail_transport::Error::Denied)?;
        if token != "synthetic-grant" || !r.approved || r.revoked {
            return Err(farsail_transport::Error::Denied);
        }
        Ok(Claims {
            session_id: id.into(),
            permission: r.permission,
            source_public_key: self.source.lock().unwrap().clone(),
            target_public_key: self.target.clone(),
            nonce: "ab".repeat(32),
            expires_in: 25,
            checked_at: Instant::now(),
        })
    }
    async fn issue(&self, _: &str, _: bool) -> farsail_transport::Result<String> {
        Ok("synthetic-grant".into())
    }
}
fn login() -> Value {
    json!({"access_token":"synthetic-access","refresh_token":"synthetic-refresh","session_id":"synthetic-session","access_expires_in":3600})
}
async fn answer(State(f): State<Arc<Fixture>>, request: Request) -> Json<Value> {
    let path = request.uri().path().to_owned();
    let bytes = axum::body::to_bytes(request.into_body(), 8192)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let value = match path.as_str() {
        "/v1/auth/login" | "/v1/auth/refresh" => login(),
        "/v1/me" => {
            json!({"id":"synthetic-owner","role":"user","verified":true,"email":"fixture@example.invalid"})
        }
        "/v1/devices/challenge" => {
            *f.source.lock().unwrap() = v["public_key"].as_str().unwrap().into();
            json!({"challenge_id":"synthetic-challenge","message":"synthetic-binding-challenge"})
        }
        "/v1/devices/bind" => json!({"id":SOURCE,"device_token":"synthetic-device"}),
        "/v1/devices/heartbeat" => json!({"generation":1,"expires_in":60}),
        "/v1/devices" => json!([
            {"id":SOURCE,"name":"Owned controller","platform":"android","enabled":true,"online":true,"can_host":false,"can_files":false,"last_seen_at":null},
            {"id":DESKTOP,"name":"Synthetic desktop","platform":"windows","enabled":true,"online":true,"can_host":true,"can_files":false,"last_seen_at":null},
            {"id":PHONE,"name":"Synthetic phone","platform":"android","enabled":true,"online":true,"can_host":true,"can_files":false,"last_seen_at":null},
            {"id":PENDING,"name":"Pending phone","platform":"android","enabled":true,"online":true,"can_host":true,"can_files":false,"last_seen_at":null}]),
        "/v1/remote/request" => {
            assert_eq!(v["source_device_id"], SOURCE);
            let target = v["target_device_id"].as_str().unwrap();
            assert!([DESKTOP, PHONE, PENDING].contains(&target));
            let permission: RemotePermission =
                serde_json::from_value(v["permission"].clone()).unwrap();
            assert!(permission != RemotePermission::Files);
            let n = f.next.fetch_add(1, Ordering::SeqCst) + 1;
            let id = format!("30000000-0000-4000-8000-{n:012}");
            f.requests.lock().unwrap().insert(
                id.clone(),
                Remote {
                    permission,
                    phone: target != DESKTOP,
                    approved: target != PENDING,
                    revoked: false,
                    open: false,
                    inputs: vec![],
                    acks: 0,
                },
            );
            json!({"id":id})
        }
        "/fixture/report" => {
            let rows = f.requests.lock().unwrap();
            json!({"requests":rows.iter().map(|(id,r)|json!({"id":id,"permission":r.permission,"phone":r.phone,"approved":r.approved,"revoked":r.revoked,"open":r.open,"inputs":r.inputs,"acks":r.acks})).collect::<Vec<_>>()})
        }
        _ => {
            let id = path.split('/').nth(3).unwrap_or("");
            if path.ends_with("/peer-address") {
                // Keep the real viewer in Connecting across several UI polls.
                // Pending connections have no media consumer yet.
                tokio::time::sleep(Duration::from_millis(450)).await;
                json!({"endpoint_addr":f.address})
            } else if path.ends_with("/transport-grant") {
                json!(f.inspect(id, "synthetic-grant").await.unwrap())
            } else if path.ends_with("/revoke") {
                if let Some(r) = f.requests.lock().unwrap().get_mut(id) {
                    r.revoked = true;
                }
                json!({})
            } else if path.starts_with("/v1/remote/") {
                let rows = f.requests.lock().unwrap();
                let r = rows.get(id).unwrap();
                json!({"id":id,"state":if r.revoked{"revoked"}else if r.approved{"approved"}else{"pending"}})
            } else {
                json!({})
            }
        }
    };
    Json(value)
}
async fn run(f: Arc<Fixture>, s: Session) {
    let (phone, permission) = {
        let mut rows = f.requests.lock().unwrap();
        let r = rows.get_mut(s.id()).unwrap();
        r.open = true;
        (r.phone, r.permission)
    };
    let (w, h) = if phone { (320, 640) } else { (640, 360) };
    let display = if phone { 1 } else { 7 };
    if phone {
        s.send(Frame {
            channel: Channel::Media,
            bytes: b"FSH1android".to_vec(),
        })
        .await
        .unwrap();
    }
    let mut list = b"FSL1".to_vec();
    list.extend(serde_json::to_vec(&json!([{"id":display,"name":"Synthetic display","x":0,"y":0,"width":w,"height":h,"dpi":160,"rotation":0}])).unwrap());
    s.send(Frame {
        channel: Channel::Media,
        bytes: list,
    })
    .await
    .unwrap();
    let mut rgb = vec![0; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let color = if x < w / 2 {
                if y < h / 2 {
                    [200, 73, 67]
                } else {
                    [227, 185, 79]
                }
            } else if y < h / 2 {
                [65, 141, 224]
            } else {
                [160, 95, 211]
            };
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&color);
        }
    }
    let image = JpegFrame::encode_rgb(
        FrameMeta {
            monitor: display,
            layout: 55,
            sequence: 1,
            captured_ms: 0,
            width: w as u32,
            height: h as u32,
            origin_x: 0,
            origin_y: 0,
        },
        &rgb,
        70,
    )
    .unwrap();
    let session = s.clone();
    let sent = Arc::new(AtomicU64::new(0));
    let ack = Arc::new(AtomicU64::new(0));
    let acked = ack.clone();
    let sent_clone = sent.clone();
    let sender = tokio::spawn(async move {
        loop {
            if !session.is_open_now() {
                break;
            }
            let seq = sent_clone.load(Ordering::SeqCst);
            if (!phone || seq == 0) && seq - acked.load(Ordering::SeqCst) < 2 {
                let mut image = image.clone();
                image.meta.sequence = seq + 1;
                sent_clone.store(seq + 1, Ordering::SeqCst);
                if session
                    .send(Frame {
                        channel: Channel::Media,
                        bytes: image.to_wire().unwrap(),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(160)).await;
        }
    });
    let mut last = 0;
    while s.is_open_now() {
        let frame = match s.receive().await {
            Ok(f) => f,
            Err(farsail_transport::Error::Timeout) => continue,
            Err(_) => break,
        };
        if frame.channel == Channel::Media
            && frame.bytes.len() == 12
            && frame.bytes.starts_with(b"FSA1")
        {
            let seq = u64::from_be_bytes(frame.bytes[4..].try_into().unwrap());
            assert!(seq <= sent.load(Ordering::SeqCst));
            ack.fetch_max(seq, Ordering::SeqCst);
            f.requests.lock().unwrap().get_mut(s.id()).unwrap().acks += 1;
        } else if frame.channel == Channel::Control {
            assert_eq!(permission, RemotePermission::Control);
            assert!(frame.bytes.starts_with(b"FSI1") && frame.bytes.len() >= 16);
            let seq = u64::from_be_bytes(frame.bytes[4..12].try_into().unwrap());
            assert_eq!(seq, last + 1);
            last = seq;
            let raw = &frame.bytes[12..];
            if raw == b"FSR1" || raw == b"FSR2" || raw == b"FSR3" {
                continue;
            }
            let value: Value = serde_json::from_slice(raw).unwrap();
            let _: farsail_windows::Input = serde_json::from_value(value.clone()).unwrap();
            f.requests
                .lock()
                .unwrap()
                .get_mut(s.id())
                .unwrap()
                .inputs
                .push(value);
        }
    }
    sender.abort();
    s.close();
    f.requests.lock().unwrap().get_mut(s.id()).unwrap().open = false;
}
#[tokio::main]
async fn main() {
    let transport = Transport::bind([73; 32], Config::default()).await.unwrap();
    let mut addr = serde_json::to_value(transport.addr()).unwrap();
    // The emulator initiates UDP towards the host; no reverse UDP forwarding is needed.
    let port = addr["addrs"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|a| {
            a["Ip"]
                .as_str()
                .filter(|ip| !ip.starts_with('['))
                .and_then(|ip| ip.rsplit(':').next()?.parse::<u16>().ok())
        })
        .unwrap();
    addr["addrs"] = json!([{"Ip":format!("10.0.2.2:{port}")}]);
    let fixture = Arc::new(Fixture {
        source: Mutex::new(String::new()),
        target: hex::encode(transport.id().as_bytes()),
        address: addr,
        requests: Mutex::new(HashMap::new()),
        next: AtomicU64::new(0),
    });
    let peer = transport.clone();
    let f = fixture.clone();
    tokio::spawn(async move {
        while let Ok(s) = peer.accept(f.clone()).await {
            let f = f.clone();
            tokio::spawn(async move {
                run(f, s).await;
            });
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!(
        "{}",
        json!({"fixturePort":listener.local_addr().unwrap().port()})
    );
    axum::serve(listener, Router::new().fallback(answer).with_state(fixture))
        .await
        .unwrap();
}
