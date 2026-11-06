//! Real QUIC transport acceptance under an externally shaped loopback link.
//! Synthetic pixels only. Encoding is measured separately, not counted as FPS.
use super::*;
use std::sync::atomic::AtomicU64;

#[derive(serde::Serialize)]
struct ResultSet {
    mode: String,
    streams: usize,
    duration_seconds: u64,
    source_fps: u64,
    requested_ack_delay_ms: u64,
    output_size: (u32, u32),
    jpeg_bytes_by_quality: Vec<(u8, usize)>,
    rtt_p50: Vec<u64>,
    ack_p50: Vec<u64>,
    ack_p95: Vec<u64>,
    send_p95: Vec<u64>,
    publish_to_send_p95: Vec<u64>,
    quality_range: Vec<(u8, u8)>,
    received_payload_mbps: f64,
    direct: Vec<bool>,
    close_reasons: Vec<Option<String>>,
    reconnect_attempts: usize,
    fps: Vec<f64>,
    age_p95: Vec<u64>,
    rtt_p95: Vec<u64>,
    closed: usize,
}
fn quantile(mut values: Vec<u64>, percent: usize) -> u64 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) * percent / 100)
        .copied()
        .unwrap_or(u64::MAX)
}
fn percentile(values: Vec<u64>) -> u64 {
    quantile(values, 95)
}
#[derive(Clone, Copy, Debug)]
enum Mode {
    Viewport,
    Native,
    Legacy,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Viewport => "viewport",
            Self::Native => "native",
            Self::Legacy => "legacy_no_viewport",
        }
    }
    fn adaptive(self) -> bool {
        matches!(self, Self::Viewport)
    }
}
#[derive(Clone, Copy)]
struct Phase {
    mode: Mode,
    streams: u8,
    seconds: u64,
    warmup: u64,
    ack_delay: Duration,
}
#[derive(Default)]
struct HostSamples {
    ack: Vec<u64>,
    send: Vec<u64>,
    publish_to_send: Vec<u64>,
    quality: Vec<u8>,
    last_ack: u64,
}
async fn phase(config: Phase, rgba: &[u8], source: (u32, u32)) -> ResultSet {
    let adaptive = config.mode.adaptive();
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
    let output_size = (images[0].meta.width, images[0].meta.height);
    let jpeg_bytes_by_quality: Vec<_> = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            (
                if adaptive { 80 + index as u8 } else { 90 },
                image.jpeg.len(),
            )
        })
        .collect();
    let desktop = Transport::bind([140; 32], Config::default()).await.unwrap();
    let mut hosts = Vec::new();
    let mut targets = Vec::new();
    let mut viewers = Vec::new();
    let mut servers = Vec::new();
    let mut tasks = Vec::new();
    let mut counts = Vec::new();
    let mut ages = Vec::new();
    let mut rtts = Vec::new();
    let mut bytes_received = Vec::new();
    let mut host_samples = Vec::new();
    let measuring = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicU64::new(0));
    for index in 0..config.streams {
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
        if matches!(config.mode, Mode::Native) {
            viewer
                .send(Frame {
                    channel: Channel::Media,
                    bytes: VideoProfile {
                        profile: 2,
                        generation: 10,
                    }
                    .packet(b"FSQ1"),
                })
                .await
                .unwrap();
            packet(&viewer, b"FSP1").await;
        }
        let samples = Arc::new(Mutex::new(HostSamples::default()));
        host_samples.push(samples);
        let received_bytes = Arc::new(AtomicU64::new(0));
        bytes_received.push(received_bytes.clone());
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
                        let received_at = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64;
                        if measuring.load(Ordering::SeqCst) {
                            count.fetch_add(1, Ordering::SeqCst);
                            received_bytes.fetch_add(frame.bytes.len() as u64, Ordering::SeqCst);
                            age.lock()
                                .unwrap()
                                .push(received_at.saturating_sub(image.meta.captured_ms));
                            if let Some(value) = viewer.path().1 {
                                rtt.lock().unwrap().push(value);
                            }
                        }
                        // This controlled receiver delay is separate from QUIC RTT.
                        // It models a slow application ACK, without claiming decoding.
                        tokio::time::sleep(config.ack_delay).await;
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
    let sampler = tokio::spawn({
        let hosts = hosts.clone();
        let samples = host_samples.clone();
        let measuring = measuring.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tick.tick().await;
                if !measuring.load(Ordering::SeqCst) {
                    continue;
                }
                for (host, samples) in hosts.iter().zip(&samples) {
                    let local = host.local.lock().unwrap();
                    let mut sample = samples.lock().unwrap();
                    sample.quality.push(Host::settings(&local).quality);
                    if local.media.acknowledged != sample.last_ack {
                        sample.last_ack = local.media.acknowledged;
                        sample.ack.push(local.media.ack_ms);
                        sample.send.push(local.media.send_ms);
                        sample.publish_to_send.push(local.media.publish_to_send_ms);
                    }
                }
            }
        }
    });
    tokio::time::sleep(Duration::from_secs(config.warmup)).await;
    measuring.store(true, Ordering::SeqCst);
    // Cover several real authorization renewals and many more than the
    // 15-second no-progress deadline on the original four connections.
    let duration = config.seconds;
    tokio::time::sleep(Duration::from_secs(duration)).await;
    measuring.store(false, Ordering::SeqCst);
    let close_reasons: Vec<_> = viewers
        .iter()
        .map(|viewer| (!viewer.is_open_now()).then(|| viewer.end_reason().to_string()))
        .collect();
    let result = ResultSet {
        mode: config.mode.name().to_owned(),
        streams: usize::from(config.streams),
        duration_seconds: duration,
        source_fps: 10,
        requested_ack_delay_ms: config.ack_delay.as_millis() as u64,
        output_size,
        jpeg_bytes_by_quality,
        rtt_p50: rtts
            .iter()
            .map(|v| quantile(v.lock().unwrap().clone(), 50))
            .collect(),
        ack_p50: host_samples
            .iter()
            .map(|v| quantile(v.lock().unwrap().ack.clone(), 50))
            .collect(),
        ack_p95: host_samples
            .iter()
            .map(|v| percentile(v.lock().unwrap().ack.clone()))
            .collect(),
        send_p95: host_samples
            .iter()
            .map(|v| percentile(v.lock().unwrap().send.clone()))
            .collect(),
        publish_to_send_p95: host_samples
            .iter()
            .map(|v| percentile(v.lock().unwrap().publish_to_send.clone()))
            .collect(),
        quality_range: host_samples
            .iter()
            .map(|v| {
                let v = v.lock().unwrap();
                (
                    *v.quality.iter().min().unwrap_or(&0),
                    *v.quality.iter().max().unwrap_or(&0),
                )
            })
            .collect(),
        received_payload_mbps: bytes_received
            .iter()
            .map(|v| v.load(Ordering::SeqCst))
            .sum::<u64>() as f64
            * 8.0
            / duration as f64
            / 1_000_000.0,
        direct: viewers.iter().map(|v| v.path().0 == "direct").collect(),
        closed: close_reasons
            .iter()
            .filter(|v| v.is_some())
            .count()
            .max(closed.load(Ordering::SeqCst) as usize),
        close_reasons,
        reconnect_attempts: 0,
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
    };
    producer.abort();
    sampler.abort();
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
    let native = phase(
        Phase {
            mode: Mode::Native,
            streams: 4,
            seconds: 5,
            warmup: 1,
            ack_delay: Duration::ZERO,
        },
        &rgba,
        source,
    )
    .await;
    let adaptive = phase(
        Phase {
            mode: Mode::Viewport,
            streams: 4,
            seconds: 120,
            warmup: 4,
            ack_delay: Duration::ZERO,
        },
        &rgba,
        source,
    )
    .await;
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

/// Externally shape only a newly created network namespace. Build this test
/// before entering it, because the namespace deliberately has no internet.
/// Example: FARSAIL_MATRIX_STREAMS=4,5 FARSAIL_MATRIX_MODES=viewport
/// FARSAIL_MATRIX_SECONDS=120 FARSAIL_MATRIX_LABEL=35mbit-30ms-loss0.1
/// <test-binary> shared_link_matrix --ignored --nocapture --test-threads=1
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit real-QUIC synthetic matrix; optional shaping must be isolated"]
async fn shared_link_matrix_reports_four_and_five_stream_delivery() {
    let env =
        |key: &str, fallback: &str| std::env::var(key).unwrap_or_else(|_| fallback.to_owned());
    let streams: Vec<u8> = env("FARSAIL_MATRIX_STREAMS", "4,5")
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    assert!(streams.iter().all(|v| (1..=5).contains(v)));
    let modes: Vec<Mode> = env("FARSAIL_MATRIX_MODES", "viewport,native,legacy")
        .split(',')
        .map(|v| match v {
            "viewport" => Mode::Viewport,
            "native" => Mode::Native,
            "legacy" => Mode::Legacy,
            _ => panic!("unknown mode {v}"),
        })
        .collect();
    let seconds: u64 = env("FARSAIL_MATRIX_SECONDS", "120").parse().unwrap();
    assert!((1..=600).contains(&seconds));
    let warmup = env("FARSAIL_MATRIX_WARMUP", "4").parse().unwrap();
    let ack_delay = Duration::from_millis(env("FARSAIL_MATRIX_ACK_DELAY_MS", "0").parse().unwrap());
    let source = (2340, 1080);
    let rgba = textured_rgba(source.0, source.1);
    let mut results = Vec::new();
    for streams in streams {
        for &mode in &modes {
            let result = phase(
                Phase {
                    mode,
                    streams,
                    seconds,
                    warmup,
                    ack_delay,
                },
                &rgba,
                source,
            )
            .await;
            eprintln!(
                "MULTIVIEW_PHASE {}",
                serde_json::to_string(&result).unwrap()
            );
            results.push(result);
        }
    }
    let report = json!({
        "schema": 1,
        "synthetic_transport_only": true,
        "source_size": source,
        "source_fps_limit": 10,
        "label": env("FARSAIL_MATRIX_LABEL", "unshaped"),
        "frame_age_boundary": "publish synthetic pre-encoded JPEG to native receive/metadata validation; excludes capture, encoding, JPEG decode, WebView paint and screen-to-screen latency",
        "rtt_boundary": "selected QUIC path smoothed RTT, not application ACK or paint",
        "ack_boundary": "host send start to received FSA1; controlled receiver ACK delay is included",
        "legacy_boundary": "current host with no FSZ1 viewport negotiation; does not run old APK or any Android OS",
        "reconnect_boundary": "harness never reconnects; closed_sessions are measured, reconnect_attempts stays zero",
        "missing_samples": u64::MAX,
        "results": results,
    });
    eprintln!("MULTIVIEW_REPORT {}", report);
    if let Ok(path) = std::env::var("FARSAIL_MATRIX_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    if env("FARSAIL_MATRIX_REQUIRE_OPEN", "1") == "1" {
        for result in &results {
            assert_eq!(
                result.closed, 0,
                "{} {} streams closed: {:?}",
                result.mode, result.streams, result.close_reasons
            );
            assert!(
                result.direct.iter().all(|v| *v),
                "all measured sessions must stay direct"
            );
            assert!(
                result.fps.iter().all(|v| *v > 0.0),
                "every stream must deliver real frames"
            );
        }
    }
}
