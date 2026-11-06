**English** | [简体中文](WI-MULTIVIEW-041.zh-CN.md)

# Four and five Android targets from one controller

Date: 2026-11-05. Product baseline: desktop 0.1.36 / Android 0.1.12. Evidence: `.local/four-five-phones-20261105/`.

## Results

Four-target and five-target runs passed both with independent owners and with all devices signed in to the same account. One real NativeClient controller used a local HTTP coordinator, isolated PostgreSQL schemas and authenticated encrypted QUIC to four/five independent synthetic Android protocol targets. Each target exchanged 64 KiB media packets and marked control packets across lease rotations for about 81 seconds. Restarting and then closing one target left the others connected; survivor reconnects were zero. Same-account runs exchanged 2,732 and 3,290 packets respectively (179,044,352 and 215,613,440 media bytes). Reports: `fanout-4-same-account.json` and `fanout-5-same-account.json`; separate-owner reports omit the suffix.

Chromium tab runs passed 60 switches and six recoveries isolated to target A, with target B status delayed by 1,800 ms. Each session retained at most one status request in flight; input did not leak between targets. Independent visible-window runs passed at four and five windows: a deliberately stalled decoder in A did not stop the other windows, reconnect and close affected only A, and marked input reached only its intended target. Reports: `4/` and `5/`, `tabs-result.json` and `windows-result.json`.

The original coordinator integration and both separate-owner additions passed together (3 tests). Both same-account tests passed together (2 tests). Client integration Clippy with denied warnings, workspace formatting and diff checks passed.

## Scope

Android peer metadata, media payloads and frontend IPC are synthetic. These runs exercise the real coordinator and transport separately from the real frontend renderer; they are not a five-device end-to-end hardware test. They do not measure Android capture, physical decoder/GPU/thermal load, game FPS, WAN conditions or overnight survival. The browser fixture's roughly 6.4 synthetic frames per second is determined by its 150 ms source interval and must not be presented as real-phone performance. No production coordinator or physical input was used.

The reusable native tests are in `crates/client/tests/coordinator_flow.rs`; they require `FARSAIL_TEST_DATABASE_URL`, and optionally write JSON to `FARSAIL_FANOUT_REPORT_DIR`. Without a database they explicitly skip execution. See [viewer behavior](WI-VIEWER-TABS-039.md).

## Four/five-stream bandwidth and decoding follow-up (2026-11-06)

Live production source is `7ff536801d6e2bf2b426fab80ec9b6c9d805f9bc`, desktop 0.1.42 / Android protocol implementation 0.1.14. The user reported four direct same-Wi-Fi sessions stalling yesterday, before installing today's phone fixes. One phone had been read as App 0.1.12; the others were user estimates, not measured versions. This run did not operate physical phones. Evidence: `.local/multiview-sim-20261106/`.

### Isolated shared network

`unshare -Urn` creates a fresh loopback-only network namespace. TBF limits total bandwidth, with netem adding 15 ms one-way delay and 0.1% loss, before actual authenticated QUIC and the current Android Host run. Namespace and initial/final qdisc statistics are logged; the host network is untouched. The synthetic 2340×1080 texture source publishes pre-encoded JPEG at 10 FPS. Current automatic mode negotiates 704×324 with quality 80–90. Android capture and encoding time are excluded.

The user subsequently emphasized same-Wi-Fi LAN direct connections. The 35 / 20 Mbps limits were deliberately imposed, not measurements of the user's Wi-Fi throughput, signal, or hardware. Restricted-link tests alone cannot identify that network as the field cause. An additional local QUIC comparison enables only lo with noqueue and no added delay, loss, or rate limit; each group measures 30 seconds:

| Unshaped local simulation / streams | Received FPS per stream (10-FPS source) | RTT p95 / ms | Publication-to-receipt age p95 / ms | Total media Mbps |
| ----------------------------------- | --------------------------------------- | ------------ | ----------------------------------- | ---------------- |
| Automatic 704×324 / 4               | 10.00                                   | 1            | 13–14                               | 56.41            |
| Automatic 704×324 / 5               | 10.00                                   | 1            | 16–17                               | 70.51            |
| Explicit native 2340×1080 / 4       | 2.63–2.67                               | 12–17        | 202–212                             | 191.37           |
| Explicit native 2340×1080 / 5       | 2.63–2.67                               | 16–27        | 238–261                             | 238.76           |

All original connections remain open. The native Q90 texture JPEG is 2,249,601 bytes per frame. Current code and the App 0.1.12 tag both set `BASELINE_PAYLOAD_BPS` to `6_000_000` bytes/second/session, about 48 Mbps. Four/five native streams approach the combined software budgets of 192 / 240 Mbps. Thus large full-size frames still hit a software sending budget despite no artificial local network cap and low RTT. This cannot all be attributed to Wi-Fi, and simply removing flow control could flood a real link. The high-entropy texture is not a measurement of the user's frames, and explicit native mode is not current automatic mode; this does not prove the unique field cause. Report: `unshaped-4-5-viewport-native.json`.

The following deliberately restricted links describe pressure boundaries, not the user's Wi-Fi:

Ranges below cover per-stream minimum and maximum metrics. RTT is the selected QUIC path's smoothed round trip; frame age is synthetic publication to native receipt/metadata validation using one machine's clock. Neither includes WebView decoding or display completion. The harness never reconnects; closure is measured before intentional cleanup.

| Shared link / streams     | Received FPS per stream (10-FPS source) | RTT p95 / ms | Frame age p95 / ms | Original sessions closed |
| ------------------------- | --------------------------------------- | ------------ | ------------------ | ------------------------ |
| 35 Mbps / 4 / 120 seconds | 8.12–8.50                               | 75–83        | 192–197            | 0                        |
| 35 Mbps / 5 / 120 seconds | 6.53–7.07                               | 94–111       | 206–226            | 0                        |
| 20 Mbps / 4 / 60 seconds  | 4.68–5.03                               | 109–135      | 229–284            | 0                        |
| 20 Mbps / 5 / 60 seconds  | 3.80–4.00                               | 113–158      | 304–330            | 0                        |

Both groups deliver about 32.65 Mbps of media payload. All five streams reach quality 80, indicating shared-link saturation. Three of five streams miss the existing four-stream 7-FPS threshold. Passing matrix connectivity assertions is not full performance acceptance; zero reconnects do not prove smoothness. Thresholds were not lowered to make five streams pass. The 20-Mbps observation groups deliver about 18.86 Mbps of payload: four streams also exceed 100-ms network RTT, while five fall to about four FPS. Reports: `35mbit-4-5-viewport.json`, `20mbit-4-5-viewport.json`.

### Separate frontend decoding

The actual React Viewer runs in four/five independent Chromium contexts/pages reporting visible. Synthetic IPC supplies 60 changing texture JPEGs; each worker measurably emits 30 FPS and actual `Image.decode()` executes. Unique sequence numbers count decoding and complete images visible in the DOM at rAF. No network RTT is fabricated. Generation-to-DOM+rAF timing does not measure physical display completion.

| Streams / JPEG size / measured duration | Decoded FPS per stream | DOM+rAF FPS per stream | Generation-to-DOM+rAF p95 / ms |
| --------------------------------------- | ---------------------- | ---------------------- | ------------------------------ |
| 4 / 720×1600 / 120 seconds              | 29.35–29.82            | 29.34–29.81            | 53.7–58.2                      |
| 5 / 720×1600 / 120 seconds              | 23.92–24.50            | 23.92–24.50            | 118.6–124.5                    |
| 5 / 317×704 / 60 seconds                | 28.63–29.52            | 28.60–29.53            | 67.5–85.4                      |

Holding A's decode promise for 1.8 seconds leaves every other page producing 51–55 unique DOM+rAF updates. No cross-target input, decoding errors, or page errors occur. Media requests, status queries, and decoding each peak at one per page during measured intervals. Five full-size streams do not sustain 30 FPS. The 704 comparison still has some long tasks and non-injected spikes up to about 330 ms, so it is not stutter-free.

The full-size runs capture screenshots around measured second 30, overlapping some long tasks; these cannot simply be attributed to product behavior. The 704 run takes no screenshots inside measurement. Five full-size streams overlap the network run and bring the host near saturation. A subsequent own blank Chrome with matching launch flags reports SwiftShader, software compositing/rasterization, and no JPEG decode acceleration. GPU utilization was not measured; these CPU costs cannot be mapped directly to the user's Windows hardware.

Initial probes incorrectly rejected overlapping provisional-Viewer / StrictMode lifecycle waiters and caused fixture errors; failed evidence remains. The original 704 report also fails a whole-lifetime decode-maximum-of-two check and is unchanged. Reconstructing raw start/done intervals places those overlaps entirely before measurement; every measured interval meets the same threshold of one. Separate `frontend/formal-phase-concurrency-audit.json` preserves original hashes, lifetime maximum two, and measured maximum one. See `frontend/summary.json` and original per-scenario reports.

### Old capture cadence in source

App 0.1.12's normal projection path drops frames less than 100 ms after the preceding frame in `CaptureService.kt`, capping pre-encoding cadence at about 10 FPS. Its accessibility screenshot path without repeated projection confirmation starts at 350 ms in `AccessibilityFrames.kt` and backs off on system rate rejection, nominally at most about 2.86 FPS. Neither is determined by Wi-Fi speed or guarantees that rate on each phone. This run does not know yesterday's actual backend on every device; neither the 10-FPS protocol source nor the 30-FPS frontend stress source replaces physical capture/encoding measurement.

### Source findings and remaining boundaries

`viewer::sync_visibility` and frontend active state currently use only selected-tab state, without native-window minimization. A minimized independent window therefore continues requesting video at application level: a concrete resource-saving omission. This run does not change that behavior or test WebView2 throttling on OS minimization, and does not identify it as yesterday's cause. Source auditing found no application-wide cross-session decode queue. Native latest-frame replacement and ACK are independent of frontend presentation. Diagnostic `frame_age_ms` means time since the latest native receipt; received/delivered counters are not unique displayed frames. Metric boundaries and source locations: `metrics-audit.md`.

The Android library has 23 passing default tests and 2 ignored tests. Both ignored tests were exercised explicitly: the original strict four-stream 35-Mbps acceptance passes (native about 0.4 FPS, automatic 8.05–8.97 FPS), and the new matrix completes under 35 / 20 Mbps and unshaped conditions. The original 7-FPS, twofold improvement, 700-ms age, and 220-ms RTT thresholds remain unchanged. All-target Clippy with warnings denied, Rust formatting, and 70 bilingual documentation pairs pass.

These are separate network and frontend simulations, not additive measurements or proof of end-to-end 30 FPS on five physical phones. Actual phone capture/temperature, Wi-Fi radio contention, hardware Windows WebView2, field gameplay, and overnight behavior remain untested. Complete window-pixel adaptation requires desktop 0.1.42 and Android App 0.1.14; old 0.1.12 cannot inherit the new-protocol results. For capture recovery and stable delivery, see [capture follow-up](WI-CAPTURE-RECOVERY-043.md). Verification remains partial; no test-only installer release is published.
