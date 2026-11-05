use super::*;
use farsail_client::SecureStore;
use farsail_transport::{Authority, Claims, Config, Transport};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

struct Memory(Mutex<HashMap<String, Vec<u8>>>);
impl SecureStore for Memory {
    fn read(&self, k: &str) -> farsail_client::Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(k).cloned())
    }
    fn write(&self, k: &str, v: &[u8]) -> farsail_client::Result<()> {
        self.0.lock().unwrap().insert(k.into(), v.into());
        Ok(())
    }
    fn delete(&self, k: &str) -> farsail_client::Result<()> {
        self.0.lock().unwrap().remove(k);
        Ok(())
    }
}
struct TestAuthority {
    source: String,
    target: String,
    id: String,
    permission: RemotePermission,
    revoked: AtomicBool,
}
impl Authority for TestAuthority {
    async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
        if id != self.id || token != "synthetic-grant" || self.revoked.load(Ordering::SeqCst) {
            return Err(farsail_transport::Error::Denied);
        }
        Ok(Claims {
            session_id: id.into(),
            permission: self.permission,
            source_public_key: self.source.clone(),
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
async fn host(seed: u8) -> (Arc<Host>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new()
        .route(
            "/v1/devices/heartbeat",
            axum::routing::post(|| async { axum::Json(json!({"generation":1,"expires_in":60})) }),
        )
        .route(
            "/v1/devices/capability",
            axum::routing::post(|| async { axum::Json(json!({})) }),
        )
        .route(
            "/v1/devices/endpoint-address",
            axum::routing::post(|| async { axum::Json(json!({})) }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let store = Arc::new(Memory(Mutex::new(HashMap::new())));
    store.write("server", base.as_bytes()).unwrap();
    store.write("device-token",serde_json::to_vec(&json!({"id":"synthetic-device","device_token":"synthetic-token","owner_id":"synthetic-owner","session_id":"synthetic-session"})).unwrap().as_slice()).unwrap();
    store
        .write(
            &format!(
                "identity-{}-synthetic-owner",
                hex::encode(Sha256::digest(base.as_bytes()))
            ),
            &[seed; 32],
        )
        .unwrap();
    let client = Arc::new(NativeClient::new(store).unwrap());
    client.call("heartbeat", json!({})).await.unwrap();
    client.start_transport(Config::default()).await.unwrap();
    client.set_host_capability(true).await.unwrap();
    let h = Host::new(client);
    {
        let mut l = h.local.lock().unwrap();
        l.capture = true;
        l.control = true;
    }
    h.geometry(16, 16);
    (h, server)
}
async fn connect(
    source: &Transport,
    target: &Transport,
    id: &str,
    permission: RemotePermission,
) -> (Session, Session) {
    let authority = Arc::new(TestAuthority {
        source: hex::encode(source.id().as_bytes()),
        target: hex::encode(target.id().as_bytes()),
        id: id.into(),
        permission,
        revoked: AtomicBool::new(false),
    });
    let acceptor = target.clone();
    let a = authority.clone();
    let accept = tokio::spawn(async move { acceptor.accept(a).await.unwrap() });
    let s = source
        .connect(target.addr(), id, permission, authority)
        .await
        .unwrap();
    (s, accept.await.unwrap())
}
fn image(host: &Host) -> Vec<u8> {
    let layout = host.local.lock().unwrap().layout;
    let meta = FrameMeta {
        monitor: 1,
        layout,
        sequence: 1,
        captured_ms: 0,
        width: 16,
        height: 16,
        origin_x: 0,
        origin_y: 0,
    };
    JpegFrame::encode_rgb(meta, &[80; 16 * 16 * 3], 65)
        .unwrap()
        .jpeg
}
#[tokio::test]
async fn two_android_hosts_receive_isolated_frames_and_input_and_stop_independently() {
    let (a, sa) = host(11).await;
    let (b, sb) = host(12).await;
    let desktop = Transport::bind([21; 32], Config::default()).await.unwrap();
    let ta = Transport::bind([22; 32], Config::default()).await.unwrap();
    let tb = Transport::bind([23; 32], Config::default()).await.unwrap();
    let (va, ha) = connect(&desktop, &ta, "phone-a", RemotePermission::Control).await;
    let (vb, hb) = connect(&desktop, &tb, "phone-b", RemotePermission::Control).await;
    a.local.lock().unwrap().session = Some(ha.clone());
    b.local.lock().unwrap().session = Some(hb.clone());
    let aa = a.clone();
    let ab = b.clone();
    let runa = tokio::spawn(async move { aa.run(ha).await });
    let runb = tokio::spawn(async move { ab.run(hb).await });
    let layout = a.local.lock().unwrap().layout;
    assert!(a.publish(image(&a), 16, 16, layout));
    assert!(b.publish(image(&b), 16, 16, layout));
    for viewer in [&va, &vb] {
        let image = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let f = viewer.receive().await.unwrap();
                if let Ok(j) = JpegFrame::from_wire(&f.bytes) {
                    break j;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!((image.meta.width, image.meta.height), (16, 16));
        let mut bytes = b"FSA1".to_vec();
        bytes.extend(image.meta.sequence.to_be_bytes());
        viewer
            .send(Frame {
                channel: Channel::Media,
                bytes,
            })
            .await
            .unwrap();
    }
    let payload=serde_json::to_vec(&json!({"kind":"button","display":1,"layout":layout,"x":0.5,"y":0.5,"button":"left","down":true})).unwrap();
    let mut bytes = b"FSI1".to_vec();
    bytes.extend(1u64.to_be_bytes());
    bytes.extend(payload);
    va.send(Frame {
        channel: Channel::Control,
        bytes,
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !a.local.lock().unwrap().inputs.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let command = a.drain()[0].clone();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let d = a.call("diagnostics", json!({})).await.unwrap();
            if d["acknowledged"].as_u64().unwrap() > 0 {
                assert_eq!(d["path"], "direct");
                assert!(d["frameBytes"].as_u64().unwrap() > 49);
                assert_eq!(d["pendingFrames"], 0);
                assert!(d.get("session").is_none() && d.get("id").is_none());
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(a.input_valid("phone-a", command["epoch"].as_u64().unwrap(), layout));
    assert!(!b.input_valid("phone-a", command["epoch"].as_u64().unwrap(), layout));
    assert_eq!(b.drain(), json!([]));
    // Hidden tabs stop on the sender, not just at the desktop decoder. Keep
    // another authenticated phone sending throughout a pause longer than the
    // ordinary 15-second ACK deadline; neither connection may be recreated.
    let connection = va.stable_id();
    va.send(Frame {
        channel: Channel::Media,
        bytes: StreamDemand {
            generation: 100,
            active: false,
        }
        .packet(),
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while a.video_requested() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (a_frames, mut received_a) = tokio::sync::mpsc::channel(8);
    let (b_frames, mut received_b) = tokio::sync::mpsc::channel(64);
    let mut readers = Vec::new();
    for (viewer, output) in [(va.clone(), a_frames), (vb.clone(), b_frames)] {
        readers.push(tokio::spawn(async move {
            loop {
                let frame = match viewer.receive().await {
                    Ok(frame) => frame,
                    Err(farsail_transport::Error::Timeout) => continue,
                    Err(_) => break,
                };
                if let Ok(image) = JpegFrame::from_wire(&frame.bytes) {
                    let mut ack = b"FSA1".to_vec();
                    ack.extend_from_slice(&image.meta.sequence.to_be_bytes());
                    viewer
                        .send(Frame {
                            channel: Channel::Media,
                            bytes: ack,
                        })
                        .await
                        .unwrap();
                    if output.send(image).await.is_err() {
                        break;
                    }
                }
            }
        }));
    }
    for _ in 0..34 {
        assert!(a.publish(image(&a), 16, 16, layout));
        assert!(b.publish(image(&b), 16, 16, layout));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(matches!(
        received_a.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    let mut foreground_frames = 0;
    while received_b.try_recv().is_ok() {
        foreground_frames += 1;
    }
    assert!(
        foreground_frames >= 30,
        "visible independent phone kept moving"
    );
    assert!(va.is_open_now() && vb.is_open_now());
    va.send(Frame {
        channel: Channel::Media,
        bytes: StreamDemand {
            generation: 102,
            active: true,
        }
        .packet(),
    })
    .await
    .unwrap();
    va.send(Frame {
        channel: Channel::Media,
        bytes: StreamDemand {
            generation: 101,
            active: false,
        }
        .packet(),
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !a.video_requested() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    a.publish(image(&a), 16, 16, layout);
    let resumed = tokio::time::timeout(Duration::from_secs(3), received_a.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resumed.jpeg,
        image(&a),
        "restoring a tab must preserve image quality"
    );
    assert_eq!(
        va.stable_id(),
        connection,
        "no transport reconnect on tab activation"
    );
    for reader in readers {
        reader.abort();
    }
    a.stop_local();
    assert!(!a.input_valid("phone-a", command["epoch"].as_u64().unwrap(), layout));
    assert!(vb.is_open_now());
    assert!(!a.publish(image(&a), 16, 16, layout));
    b.geometry(32, 16);
    assert!(!b.input_valid("phone-b", 0, layout));
    b.stop_local();
    runa.abort();
    runb.abort();
    sa.abort();
    sb.abort();
    a.client.stop_transport().await;
    b.client.stop_transport().await;
    desktop.close().await;
    ta.close().await;
    tb.close().await;
}

#[tokio::test]
async fn delayed_ack_keeps_capture_alive_but_a_stalled_peer_still_expires() {
    let (h, server) = host(93).await;
    let source = Transport::bind([94; 32], Config::default()).await.unwrap();
    let target = Transport::bind([95; 32], Config::default()).await.unwrap();
    let (viewer, peer) = connect(&source, &target, "delayed-ack", RemotePermission::View).await;
    h.local.lock().unwrap().session = Some(peer.clone());
    let run = tokio::spawn({
        let h = h.clone();
        async move { h.run(peer).await }
    });
    let layout = h.local.lock().unwrap().layout;
    assert!(h.publish(image(&h), 16, 16, layout));
    let first = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let frame = viewer.receive().await.unwrap();
            if let Ok(image) = JpegFrame::from_wire(&frame.bytes) {
                break image;
            }
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(
        viewer.is_open_now(),
        "a brief multi-view network stall must not end sharing"
    );
    let mut ack = b"FSA1".to_vec();
    ack.extend(first.meta.sequence.to_be_bytes());
    viewer
        .send(Frame {
            channel: Channel::Media,
            bytes: ack,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if h.call("diagnostics", json!({})).await.unwrap()["acknowledged"]
                == first.meta.sequence
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(h.publish(image(&h), 16, 16, layout));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let next = viewer.receive().await.unwrap();
            if JpegFrame::from_wire(&next.bytes)
                .is_ok_and(|image| image.meta.sequence > first.meta.sequence)
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(18), viewer.wait_closed())
        .await
        .unwrap();
    assert_eq!(
        viewer.end_reason(),
        "media_timeout",
        "lost ACKs must still end a stalled session within a hard bound"
    );
    h.stop_local();
    run.abort();
    server.abort();
    h.client.stop_transport().await;
    source.close().await;
    target.close().await;
}

#[tokio::test]
async fn four_android_hosts_recover_dropped_windows_without_reconnecting_or_stalling_others() {
    // Use large synthetic, full-color frames, not tiny solid-color images.
    // Two viewers discard both initial frames as a read timeout would. The
    // other two must keep moving throughout recovery on the shared endpoint.
    let (width, height) = (1280, 720);
    let mut state = 17u32;
    let rgb: Vec<u8> = (0..width * height * 3)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 127) as u8
        })
        .collect();
    let jpeg = JpegFrame::encode_rgb(
        FrameMeta {
            monitor: 1,
            layout: 1,
            sequence: 1,
            captured_ms: 0,
            width,
            height,
            origin_x: 0,
            origin_y: 0,
        },
        &rgb,
        90,
    )
    .unwrap()
    .jpeg;
    assert!(jpeg.len() > 500_000);
    let desktop = Transport::bind([101; 32], Config::default()).await.unwrap();
    let mut hosts = Vec::new();
    let mut targets = Vec::new();
    let mut viewers = Vec::new();
    let mut servers = Vec::new();
    let mut tasks = Vec::new();
    let mut counts = Vec::new();
    for index in 0..4u8 {
        let (h, server) = host(102 + index).await;
        let layout = h.geometry(width, height);
        let target = Transport::bind([110 + index; 32], Config::default())
            .await
            .unwrap();
        let (viewer, peer) = connect(
            &desktop,
            &target,
            &format!("four-phone-{index}"),
            RemotePermission::View,
        )
        .await;
        h.local.lock().unwrap().session = Some(peer.clone());
        tasks.push(tokio::spawn({
            let h = h.clone();
            async move { h.run(peer).await }
        }));
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        counts.push(count.clone());
        tasks.push(tokio::spawn({
            let viewer = viewer.clone();
            let expected = jpeg.clone();
            async move {
                loop {
                    let frame = match viewer.receive().await {
                        Ok(frame) => frame,
                        Err(farsail_transport::Error::Timeout) => continue,
                        Err(_) => break,
                    };
                    if let Ok(image) = JpegFrame::from_wire(&frame.bytes) {
                        assert_eq!((image.meta.width, image.meta.height), (width, height));
                        assert_eq!(image.jpeg, expected, "recovery must preserve image quality");
                        let received = count.fetch_add(1, Ordering::SeqCst) + 1;
                        if index % 2 == 0 && received <= 2 {
                            continue;
                        }
                        let mut bytes = b"FSA1".to_vec();
                        bytes.extend(image.meta.sequence.to_be_bytes());
                        viewer
                            .send(Frame {
                                channel: Channel::Media,
                                bytes,
                            })
                            .await
                            .unwrap();
                    }
                }
            }
        }));
        hosts.push((h, layout));
        targets.push(target);
        viewers.push((viewer.clone(), viewer.stable_id()));
        servers.push(server);
    }
    let producer = tokio::spawn({
        let hosts = hosts.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                for (h, layout) in &hosts {
                    assert!(h.publish(jpeg.clone(), width, height, *layout));
                }
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if counts.iter().all(|count| count.load(Ordering::SeqCst) >= 2) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(counts[0].load(Ordering::SeqCst), 2);
    assert_eq!(counts[2].load(Ordering::SeqCst), 2);
    for index in [1, 3] {
        assert!(
            counts[index].load(Ordering::SeqCst) >= 3,
            "healthy viewer kept moving"
        );
    }
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            if counts.iter().all(|count| count.load(Ordering::SeqCst) >= 5) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for (viewer, original) in viewers {
        assert!(viewer.is_open_now());
        assert_eq!(viewer.stable_id(), original);
    }
    producer.abort();
    for task in tasks {
        task.abort();
    }
    for (h, _) in hosts {
        h.stop_local();
        h.client.stop_transport().await;
    }
    for server in servers {
        server.abort();
    }
    for target in targets {
        target.close().await;
    }
    desktop.close().await;
}
#[tokio::test]
async fn view_permission_rejects_control_frames_on_real_quic() {
    let source = Transport::bind([31; 32], Config::default()).await.unwrap();
    let target = Transport::bind([32; 32], Config::default()).await.unwrap();
    let (viewer, host) = connect(&source, &target, "view", RemotePermission::View).await;
    assert!(
        viewer
            .send(Frame {
                channel: Channel::Control,
                bytes: b"FSI1".to_vec()
            })
            .await
            .is_err()
    );
    host.close();
    viewer.close();
    source.close().await;
    target.close().await;
}
#[tokio::test]
async fn projection_stop_invalidates_delayed_enable_and_old_geometry() {
    let (h, s) = host(41).await;
    let old = h.begin();
    h.stop_local();
    assert!(h.call("share", json!({"epoch":old})).await.is_err());
    let next = h.begin();
    assert!(next > old);
    let l = h.geometry(16, 16);
    assert_eq!(h.rgba_demand(16, 16, l), Some(true));
    h.local.lock().unwrap().video_demand.active = false;
    assert_eq!(h.rgba_demand(16, 16, l), Some(false));
    let jpeg = image(&h);
    h.geometry(32, 16);
    assert!(
        h.rgba_demand(16, 16, l).is_none(),
        "pausing must not accept stale capture geometry"
    );
    assert!(!h.publish(jpeg, 16, 16, l));
    h.stop_local();
    assert!(h.screenshot_demand(next).is_none());
    h.client.stop_transport().await;
    s.abort();
}

#[tokio::test]
async fn stale_accessibility_frames_cannot_change_new_capture_geometry() {
    let (h, server) = host(42).await;
    let old = h.begin();
    let jpeg = image(&h);
    h.stop_local();
    let next = h.begin();
    assert!(h.screenshot_demand(old).is_none());
    h.local.lock().unwrap().video_demand.active = false;
    assert_eq!(h.screenshot_demand(next), Some(false));
    h.geometry(32, 16);
    assert!(!h.publish_accessibility(jpeg.clone(), 16, 16, old));
    {
        let local = h.local.lock().unwrap();
        assert_eq!((local.width, local.height), (32, 16));
    }
    assert!(h.frames.borrow().is_none());
    assert!(h.publish_accessibility(jpeg.clone(), 16, 16, next));
    assert!(h.frames.borrow().is_some());
    h.stop_local();
    assert!(!h.publish_accessibility(jpeg, 16, 16, next));
    assert!(h.frames.borrow().is_none());
    h.client.stop_transport().await;
    server.abort();
}

#[tokio::test]
async fn same_account_auto_approval_stops_with_control_or_capture() {
    let (h, server) = host(39).await;
    h.control(true);
    assert_eq!(h.client.public_state().await["remoteWatch"], true);
    h.control(false);
    assert_eq!(h.client.public_state().await["remoteWatch"], false);
    h.control(true);
    h.stop_local();
    assert_eq!(h.client.public_state().await["remoteWatch"], false);
    h.control(true);
    assert_eq!(h.client.public_state().await["remoteWatch"], false);
    server.abort();
}

#[tokio::test]
async fn presence_keeps_endpoint_without_restoring_capture_or_approval() {
    let (h, server) = host(40).await;
    h.stop_local();
    h.control(false);
    for _ in 0..2 {
        let state = h.call("presence", json!({})).await.unwrap();
        assert_eq!(state["transportRunning"], true);
        assert_eq!(state["sharing"], false);
        assert_eq!(state["remoteWatch"], false);
        assert!(!h.local.lock().unwrap().capture);
    }
    assert!(h.call("share", json!({"epoch": 0})).await.is_err());
    h.client.stop_transport().await;
    server.abort();
}

#[tokio::test]
async fn presence_without_device_credentials_cannot_start_an_endpoint() {
    let client = Arc::new(NativeClient::new(Arc::new(Memory(Mutex::new(HashMap::new())))).unwrap());
    let h = Host::new(client);
    assert!(h.call("presence", json!({})).await.is_err());
    assert!(!h.client.transport_running().await);
    assert!(!h.local.lock().unwrap().capture);
}
