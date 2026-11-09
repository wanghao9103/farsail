//! Synthetic emulator-only driver. Fixed test identity/grant, never real credentials.
use farsail_core::RemotePermission;
use farsail_media::JpegFrame;
use farsail_transport::{Authority, Channel, Claims, Config, Frame, Session, Transport};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn wait_for_owned_input_fixture(port: u16) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(mut socket) = tokio::net::TcpStream::connect(std::net::SocketAddr::from(([127, 0, 0, 1], port))).await {
                let request = b"GET /probe/input-ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
                if socket.write_all(request).await.is_ok() {
                    let mut response = Vec::new();
                    if socket.take(4096).read_to_end(&mut response).await.is_ok() {
                        let response = String::from_utf8_lossy(&response);
                        if response.split("\r\n\r\n").nth(1).and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).is_some_and(|v| v["ready"] == true) { break; }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }).await.expect("owned input fixture did not become ready");
}
struct Synthetic {
    source: String,
    target: String,
}
const ID: &str = "50000000-0000-4000-8000-000000000001";
impl Authority for Synthetic {
    async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
        if id != ID || token != "synthetic-grant" {
            return Err(farsail_transport::Error::Denied);
        }
        Ok(Claims {
            session_id: ID.into(),
            permission: RemotePermission::Control,
            source_public_key: self.source.clone(),
            target_public_key: self.target.clone(),
            nonce: "ab".repeat(32),
            expires_in: 25,
            checked_at: Instant::now(),
        })
    }
    async fn issue(&self, _: &str, _: bool) -> farsail_transport::Result<String> {
        unreachable!()
    }
}
async fn input(session: &Session, seq: u64, v: serde_json::Value) {
    let mut bytes = b"FSI1".to_vec();
    bytes.extend(seq.to_be_bytes());
    bytes.extend(serde_json::to_vec(&v).unwrap());
    session
        .send(Frame {
            channel: Channel::Control,
            bytes,
        })
        .await
        .unwrap();
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let secret = iroh::SecretKey::from_bytes(&[91; 32]);
    if args.get(1).is_some_and(|s| s == "--key") {
        println!("{}", hex::encode(secret.public().as_bytes()));
        return;
    }
    let target: iroh::EndpointId = args[1].parse().unwrap();
    let port: u16 = args[2].parse().unwrap();
    let source = Transport::bind([91; 32], Config::default()).await.unwrap();
    let authority = Arc::new(Synthetic {
        source: hex::encode(source.id().as_bytes()),
        target: hex::encode(target.as_bytes()),
    });
    let addr = iroh::EndpointAddr::new(target)
        .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
    let session = source
        .connect(addr, ID, RemotePermission::Control, authority)
        .await
        .unwrap();
    let receive = session.clone();
    let (first_tx, first_rx) = tokio::sync::oneshot::channel();
    let reader = tokio::spawn(async move {
        let mut frames = 0;
        let mut first_tx = Some(first_tx);
        loop {
            let f = receive.receive().await.unwrap();
            eprintln!(
                "packet {:?} {} {:?}",
                f.channel,
                f.bytes.len(),
                &f.bytes[..f.bytes.len().min(8)]
            );
            if f.bytes.starts_with(b"FSM1")
                && let Err(error) = JpegFrame::from_wire(&f.bytes)
            {
                eprintln!("invalid image: {error}");
            }

            if let Ok(image) = JpegFrame::from_wire(&f.bytes) {
                frames += 1;
                if let Some(tx) = first_tx.take() {
                    let _ = tx.send(image.meta.layout);
                }
                if frames == 1 {
                    let decoded = image
                        .decode_rgb()
                        .expect("complete Android JPEG must decode");
                    assert_eq!(
                        decoded.len(),
                        image.meta.width as usize * image.meta.height as usize * 3
                    );
                    println!(
                        "{}",
                        json!({"first_frame":true,"decoded":true,"width":image.meta.width,"height":image.meta.height,"layout":image.meta.layout,"code":receive.verification_code().unwrap()})
                    );
                }
                let mut bytes = b"FSA1".to_vec();
                bytes.extend(image.meta.sequence.to_be_bytes());
                receive
                    .send(Frame {
                        channel: Channel::Media,
                        bytes,
                    })
                    .await
                    .unwrap();
            }
        }
    });
    tokio::time::sleep(Duration::from_secs(2)).await;
    let layout = tokio::time::timeout(Duration::from_secs(5), first_rx)
        .await
        .expect("no Android frame")
        .unwrap();
    let wake_first = args.get(5).is_some_and(|s| s == "--wake-first");
    if wake_first {
        input(&session, 1, json!({"kind":"button","display":1,"layout":layout,"x":0.5,"y":0.5,"button":"left","down":true})).await;
        input(&session, 2, json!({"kind":"button","display":1,"layout":layout,"x":0.5,"y":0.5,"button":"left","down":false})).await;
        // The owned Android fixture verifies wake without dismissal, then unlocks
        // only its own synthetic credential before the normal input checks.
        if let Some(port) = args.get(6) {
            wait_for_owned_input_fixture(port.parse().unwrap()).await;
        } else {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
    if args.get(5).is_some_and(|s| s == "--connection-wake") {
        // No control input before Android verifies connection-only wake.
        wait_for_owned_input_fixture(args.get(6).unwrap().parse().unwrap()).await;
    }
    let first_input = if wake_first { 3 } else { 1 };
    input(
        &session,
        first_input,
        json!({"kind":"text","text":"desktop-android-smoke"}),
    )
    .await;
    let x: f64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0.5);
    let y: f64 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(0.5);
    input(&session, first_input + 1, json!({"kind":"button","display":1,"layout":layout,"x":x,"y":y,"button":"left","down":true})).await;
    input(&session, first_input + 2, json!({"kind":"button","display":1,"layout":layout,"x":x,"y":y,"button":"left","down":false})).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    println!(
        "{}",
        json!({"input_sent":true,"still_open":session.is_open_now()})
    );
    assert!(session.is_open_now());
    reader.abort();
    session.close();
    source.close().await;
}
