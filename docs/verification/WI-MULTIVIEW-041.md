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

## Per-frame rendering fix for visible viewers

Further auditing of baseline `216072b` found that every decoded JPEG calls `setPicture`, re-executing the entire Viewer toolbar, menus, and input components at frame cadence. Four/five visible windows multiply this cost. `sameFrameGeometry` now checks display, layout generation, and dimensions: only changes to these update React picture state; same-geometry pixels update the current image node. Decoding and live/display gates still complete before presentation. Quality, source cadence, network budgets, and authorization are unchanged.

Mounts and unrelated UI refreshes restore the latest URL reference instead of resurrecting an already revoked metadata-frame URL; React no longer declares the stale image src. Geometry changes still commit together with input handlers, preserving coordinate, rotation, and terminal gates. The new `UI_ONLY_FRAME_HOTPATH` scope verifies that one waiting decoder neither queues work nor blocks another page, that decoding completion updates the image, and that stopping synthetic frames across status/FPS refreshes retains the usable latest image rather than an old URL.

Final production build, complete Chromium/WebKit viewer regressions, and the focused scope in both engines pass. Evidence: build-final, viewer-_-final, and hotpath-_ logs in `.local/multiview-render-fix-20261106/`. An initial external-subscription implementation regressed under pressure and was discarded. The first profiler omitted an observation variable and failed; its evidence remains. Old fixture failures counting startup decode overlap are audited separately from formal measurement, without lowering thresholds.

Performance comparisons use five independent visible pages, the same changing texture JPEGs, and a 30-FPS synthetic source. Main-thread JavaScript execution time is not total machine CPU utilization. Input-handler identity changes are only a per-rAF proxy for parent updates, not exhaustive commit counts. Different Chrome versions are not combined as a controlled before/after pair; browser versions, source hashes, and raw logs remain available. No user Wi-Fi or physical Android capture/encoding is measured. Software JPEG decoding, composition, and tail waits remain, so this does not establish that all five-phone field stuttering is solved. This source change is not yet published in a new installer.

In the same Chrome 157.0.8103.38 five-stream observations, pre-fix / same-hot-path candidate main-thread JavaScript execution totals are 41.54 / 11.11 seconds, about 73.26% lower. Observed handler changes per window fall from 417–476 to 66–68. Formal frame measurement lasts 20 seconds; CDP snapshots additionally include roughly one preceding second with the same collection logic on both sides. This is not a whole-machine CPU reduction. Per-stream decoding changes from 18.60–21.40 to 20.70–23.15 FPS; p95 does not improve for every stream. `comparison.json` preserves these observations without combining later load conditions into a larger claimed benefit.

The final source separately runs five streams in the same browser, 720×1600, a 30-FPS source, and a 20-second interval. A's injected decode hold yields 27.04 FPS; the others reach 29.49–29.64 FPS with continued updates and isolated input. Software rendering and host load still affect absolute rates; this later sample is not combined with the earlier baseline to calculate improvement. Original final/baseline reports retaining whole-lifetime decode overlap of two remain failed. `formal-concurrency.json` independently reconstructs raw intervals and confirms maximum one throughout formal measurement, without lowering thresholds. Complete regressions and focused latest-frame checks actually pass; overall performance and field coverage remain partial.

## Runtime busy loop on a mismatching cached frame

Further inspection found that Android `send_frames` directly continues when the cached JPEG geometry/layout or current session does not match, bypassing the asynchronous wait at the loop bottom. A persistent rejected value repeatedly consumes a runtime worker and may delay other tasks. The new branch waits for frames/ACK/demand changes, closure, or a 50-ms fallback wake, without discarding a concurrent fresh frame or reopening the session.

The new single-thread authenticated real-QUIC regression `stale_cached_geometry_yields_to_runtime_timers` injects a valid JPEG with a mismatching layout. The old implementation exits 124 under an external 8-second timeout because the internal 200-ms timer cannot run. After the fix, the timer fires and publishing a valid frame delivers its current layout on the same connection, confirming recovery without reconnecting. Final Android library results are 24 passed / 2 ignored, with all-target Clippy warnings denied and formatting checks passing. Evidence: `.local/stale-frame-yield-20261106/`.

This establishes a fault-injected error path, not an observed field layout failure. Normal viewport negotiation already clears old frames; it does not establish that every rotation triggers busy looping or that this uniquely caused all visible-window stuttering. Encoding, screenshot cadence, network budgets, authorization, and public installers are unchanged.

## Geometry commit and terminal-frame lifetime follow-up

Further auditing of the unpublished fast path reproduced a concurrency boundary: a frame advances requested geometry before React commits its input handlers, letting a second same-geometry frame publish pixels early. Holding React's queued work while decoding two 450×800 frames exposes new dimensions with the old input callback. Geometry changes/initial mounting now use `flushSync` to commit pixels and handlers together; ordinary same-geometry frames retain the fast path. The focused test additionally changes layout generation and checks that clicks carry the current layout.

Another reproduced gap follows fatal input-send failure. A decoder is held, input RPC fails, and native close remains pending; releasing the decoder previously restored pixels already cleared by the terminal UI. A separate media-running marker now stops reads, fences late frames, and clears the pending image source/unpublished URL immediately. It is separate from the window lifetime lease used by manual recovery. Native termination and unmount also share cleanup. New assertions require revocation before close RPC completes, no resurrected pixels after decode, and no further media reads.

The JPEG payload now uses a `subarray` view for Blob construction, removing one payload-sized intermediate copy per frame without changing content or byte bounds. Its CPU/latency benefit is not separately quantified. Evidence in `.local/frame-geometry-20261106/`: before/after inject the geometry-commit race; late-before/late-after inject delayed decoding while close remains pending. Failed logs/JSON remain preserved.

These are pre-release source and synthetic-IPC reproductions, not captures from the user's four physical phones. Wi-Fi is unmeasured and no new whole-machine FPS/latency guarantee is made. The sweep also checks capture epoch/profile rejection, pause/resume, reconnect component identity, input layout ownership, and resource cleanup without speculative protocol/capture-cadence changes.

Final production build, full Chromium/WebKit viewer regressions, and focused fault-injection checks in both engines pass, together with 30 state-policy tests. Final logs: build-terminal, final-chromium, final-webkit, terminal-chromium, terminal-webkit, and policies-final. No additional reproducible defect was found in the inspected remote-picture/input/lifecycle paths. This is not proof of physical-device, wireless, or global bug absence; no new installer is published.

## Complete JPEG size-limit fallback

Further inspection of Android `encode_rgba_profile` reproduced frame loss with deterministic 1080×2560 high-entropy RGBA. The old path returns None after Q90/Q85 exceed the bound although Q80 encodes to 3,855,888 bytes, below the existing 4,000,000-byte limit. The encoder now makes at most three attempts within the current 80–90 quality range, reaching the legal floor. It never drops below 80 or changes full-color 4:4:4, dimensions, or byte budgets. Frames exceeding the bound at all legal fallback qualities remain rejected.

`oversized_native_frame_retries_the_supported_quality_floor` actually fails before the fix. Afterward it matches the legal Q80 reference byte-for-byte and retains 1080×2560 dimensions. Final Android results are 25 passed / 2 ignored; all-target Clippy with warnings denied and formatting checks pass. Evidence: `.local/jpeg-fallback-20261106/`. This is a synthetic-content encoding reproduction, not a phone-field diagnosis; worst-case three-attempt CPU cost on phones is not separately measured. First-attempt success costs no additional encodes, and screenshot caching still reuses static content.

## Frame-counter range and repeated reads

Sequence/layout headers are u64, while desktop IPC cursors/input use JavaScript number. Injected sequence 9007199254740993 is rounded to 9007199254740992 by the old UI. Native latest sequence remains larger than that rounded cursor, potentially returning the same frame repeatedly while the UI discards its rounded, non-increasing sequence. The old browser reproduction receives five injections without closing and retains an image. This is abnormal semantic data, not a Wi-Fi throughput or radio-error measurement.

Before allocating the JPEG Blob or advancing the cursor, the UI now rejects non-safe-integer sequence/layout values and non-positive sequences, terminates local media, and requests closure of this session. This prevents imprecise input layouts and rereading without changing ordinary counters or the native u64 protocol itself. Both engines' focused tests cover sequence and layout overflow, requiring one injection, one close, no subsequent reads, and an intact original page. The repaired local control closes and clears pixels after one invalid frame. Evidence: `.local/frame-counter-20261106/`. It does not imply that ordinary short-running phones naturally reach these counters.
