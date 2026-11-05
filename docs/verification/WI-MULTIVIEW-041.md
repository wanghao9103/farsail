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
