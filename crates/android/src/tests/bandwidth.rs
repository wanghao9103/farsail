//! Real QUIC transport acceptance under an externally shaped loopback link.
//! Synthetic pixels only. Encoding is measured separately, not counted as FPS.
use super::*;
use std::sync::atomic::AtomicU64;

struct ResultSet {
    fps: Vec<f64>,
    age_p95: Vec<u64>,
    rtt_p95: Vec<u64>,
    closed: usize,
}
fn percentile(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) * 95 / 100)
        .copied()
        .unwrap_or(u64::MAX)
}
async fn phase(adaptive: bool, rgba: &[u8], source: (u32, u32)) -> ResultSet {
    let mut images = Vec::new();
    for quality in 80..=90 {
        let settings = capture::Settings {
            profile: Host::native_profile(),
            edge: adaptive.then_some(704),
            quality,
        };
        images.push(
            capture::encode_rgba_profile(
                rgba,
                source.0,
                source.1,
                source.0 as usize * 4,
                1,
                settings,
            )
            .unwrap(),
        );
        if !adaptive {
            break;
        }
    }
    // Native mode always uses its original Q90; the adaptive phase can follow
    // the same delivery feedback that JNI capture reads on a real phone.
    if !adaptive {
        images[0] =
            capture::encode_rgba(rgba, source.0, source.1, source.0 as usize * 4, 1).unwrap();
    }
    let desktop = Transport::bind([140; 32], Config::default()).await.unwrap();
    let mut hosts = Vec::new();
    let mut targets = Vec::new();
    let mut viewers = Vec::new();
    let mut servers = Vec::new();
    let mut tasks = Vec::new();
    let mut counts = Vec::new();
    let mut ages = Vec::new();
    let mut rtts = Vec::new();
    let measuring = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicU64::new(0));
    for index in 0..4u8 {
        let (h, server) = host(141 + index).await;
        h.geometry(source.0, source.1);
        let target = Transport::bind([150 + index; 32], Config::default())
            .await
            .unwrap();
        let (viewer, peer) = connect(
            &desktop,
            &target,
            &format!("shaped-{adaptive}-{index}"),
            RemotePermission::View,
        )
        .await;
        h.local.lock().unwrap().session = Some(peer.clone());
        tasks.push(tokio::spawn({
            let h = h.clone();
            async move { h.run(peer).await }
        }));
        packet(&viewer, b"FSZ0").await;
        if adaptive {
            viewer
                .send(Frame {
                    channel: Channel::Media,
                    bytes: ViewportRequest {
                        generation: 10,
                        edge: 704,
                    }
                    .packet(b"FSZ1"),
                })
                .await
                .unwrap();
            packet(&viewer, b"FSZ2").await;
        }
        let count = Arc::new(AtomicU64::new(0));
        let age = Arc::new(Mutex::new(Vec::new()));
        let rtt = Arc::new(Mutex::new(Vec::new()));
        counts.push(count.clone());
        ages.push(age.clone());
        rtts.push(rtt.clone());
        tasks.push(tokio::spawn({
            let viewer = viewer.clone();
            let measuring = measuring.clone();
            let closed = closed.clone();
            async move {
                loop {
                    let frame = match viewer.receive().await {
                        Ok(f) => f,
                        Err(farsail_transport::Error::Timeout) => continue,
                        Err(_) => {
                            closed.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                    };
                    if let Ok(image) = JpegFrame::from_wire(&frame.bytes) {
                        let mut bytes = b"FSA1".to_vec();
                        bytes.extend(image.meta.sequence.to_be_bytes());
                        if viewer
                            .send(Frame {
                                channel: Channel::Media,
                                bytes,
                            })
                            .await
                            .is_err()
                        {
                            closed.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                        if measuring.load(Ordering::SeqCst) {
                            count.fetch_add(1, Ordering::SeqCst);
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            age.lock()
                                .unwrap()
                                .push(now.saturating_sub(image.meta.captured_ms));
                            if let Some(value) = viewer.path().1 {
                                rtt.lock().unwrap().push(value);
                            }
                        }
                    }
                }
            }
        }));
        hosts.push(h);
        targets.push(target);
        viewers.push(viewer);
        servers.push(server);
    }
    let producer = tokio::spawn({
        let hosts = hosts.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                for h in &hosts {
                    let settings = Host::settings(&h.local.lock().unwrap());
                    let index = if adaptive {
                        (settings.quality - 80) as usize
                    } else {
                        0
                    };
                    // A concurrent quality update may invalidate one encode. The next
                    // tick publishes the freshest settings instead of queueing a retry.
                    let _ = h.publish_accessibility_encoded(
                        images[index].clone(),
                        source.0,
                        source.1,
                        0,
                        settings,
                    );
                }
            }
        }
    });
    tokio::time::sleep(Duration::from_secs(if adaptive { 4 } else { 1 })).await;
    measuring.store(true, Ordering::SeqCst);
    let duration = if adaptive { 12 } else { 5 };
    tokio::time::sleep(Duration::from_secs(duration)).await;
    measuring.store(false, Ordering::SeqCst);
    let result = ResultSet {
        fps: counts
            .iter()
            .map(|v| v.load(Ordering::SeqCst) as f64 / duration as f64)
            .collect(),
        age_p95: ages
            .iter()
            .map(|v| percentile(v.lock().unwrap().clone()))
            .collect(),
        rtt_p95: rtts
            .iter()
            .map(|v| percentile(v.lock().unwrap().clone()))
            .collect(),
        closed: closed.load(Ordering::SeqCst) as usize,
    };
    producer.abort();
    for task in tasks {
        task.abort();
    }
    for viewer in viewers {
        viewer.close();
    }
    for h in hosts {
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
    result
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated 35-Mbit/s loopback shaping; never alter a user's real network"]
async fn four_streams_share_35mbit_without_reconnects_and_with_bounded_frame_latency() {
    assert_eq!(std::env::var("FARSAIL_SHAPED_TEST").as_deref(), Ok("1"));
    let source = (2340, 1080);
    let rgba = textured_rgba(source.0, source.1);
    let native = phase(false, &rgba, source).await;
    let adaptive = phase(true, &rgba, source).await;
    eprintln!(
        "{}",
        json!({"synthetic_transport_only":true,"native_fps":native.fps,"adaptive_fps":adaptive.fps,"adaptive_frame_age_p95_ms":adaptive.age_p95,"adaptive_rtt_p95_ms":adaptive.rtt_p95,"adaptive_reconnections":adaptive.closed})
    );
    assert_eq!(
        adaptive.closed, 0,
        "all four original connections must survive"
    );
    for i in 0..4 {
        assert!(
            adaptive.fps[i] >= 7.0,
            "window {i} did not sustain 7 FPS: {}",
            adaptive.fps[i]
        );
        assert!(
            adaptive.fps[i] >= native.fps[i] * 2.0,
            "window {i} did not improve over full JPEG"
        );
        assert!(adaptive.age_p95[i] < 700, "window {i} queued stale frames");
        assert!(
            adaptive.rtt_p95[i] < 220,
            "window {i} still queued the network excessively"
        );
    }
}
