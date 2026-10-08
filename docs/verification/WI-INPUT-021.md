**English** | [简体中文](WI-INPUT-021.zh-CN.md)

# INPUT-021 — mouse control stops after click

## Baseline and scope

- Root takes over interrupted successors on 2026-10-08; neither is active. SHARE-020 has completed delivery at source be659f287386f5b6ef65d690158da23157904d15 with seven successful CI runs and verified public originals. Current clean baseline 365f2d4a661ae067200d112d22fd3f263d93f812; branch codex/input-click-021.
- User: mouse initially controls, one click then no response, no error; keyboard not tried. Root's current-user installed-version registry says0.1.7, with no running FarSail process found. Second machine version and exact click destination unknown; don't claim a unique physical root cause from source alone.
- Write set: Windows input button release/native tests, viewer pointer tracking/capture/cancellation, synthetic viewer regression, installer version and release/docs. No server/codec/permission widening, primary WIP, production profile, admin/UAC bypass or actual-user input.
- Bootstrap reuses prior media/input cancellation and fixed desktop/fit geometry notes. Still applicable: session/grant/sharing checks, bounded ordered input, owned-button release, true injected-count feedback and explicit pause on OS refusal. Earlier single-click test is insufficient for repeated/drag/layout transitions.
- Validate a mocked no-OS-input stale-layout release reproduction, continued button/movement after release, unowned-up rejection and refusal retaining tracked state. Browser must verify newly emitted balanced input over repeated clicks, drag outside image, frame changes, blur/cancel and existing pause/terminal/view-only/fit/quality checks. Interactive native checks only verified own foreground HWND/PID. Publish exact tested CI originals and compare anonymous public hashes.

## Evidence

- Before the correction, the mocked owned-button UP regression failed with Geometry while the tracked button remained pressed. No OS input was injected. Corrected native release ignores stale positioning only for a button actually injected by this session; valid release coordinates still position the cursor, unowned UP does nothing, and a refused UP remains tracked until cleanup succeeds. Three regressions pass.
- Browser uses mouse Pointer Events and button-mask transitions, pointer capture and release/cancel on blur/lost capture. A gesture must start inside the image; hovering back with an outside-held button cannot re-press the peer. Owned drag positions clamp at the source edge; view-only, input pause, closed sessions and bounded generation cancellation remain.
- Both browser suites and TypeScript/Vite passed. New tests verify 20 alternating left/right clicks (40 new balanced packets), simultaneous left/right, release outside image, new-frame layout metadata, window blur and lost capture, then a fresh click and keyboard input with media still visible. Existing fit/quality/FPS/pause/terminal/readonly checks pass.
- Unified Rust 56 ordinary tests passed, two interactive tests kept separately ignored; format and Clippy -D warnings passed. Explicit real-input test passed: verified foreground HWND/PID, 20 repeated clicks including UP after absent/new layout, no held button left, alternating two owned normal windows and continued keyboard input. No real user app or private pixels operated.
- Actual native viewer isolation passed nine denied IPC/window operations and lifecycle; sharing preference restart fixture independently passed after the change. Release CI/public artifact evidence follows below. This corrects a reproducible failure path; it does not assert the only cause of the user's uninstrumented original physical incident or bypass elevated/secure desktops.

## Published delivery — 2026-10-08

- Source `70d9393342d07d0989fed6a178edb0e3ab057117` pushed to main. [Installer 37722171352](https://github.com/wanghao9103/farsail/actions/runs/37722171352), [Windows client 37722169148](https://github.com/wanghao9103/farsail/actions/runs/37722169148), [transport 37722169167](https://github.com/wanghao9103/farsail/actions/runs/37722169167), [backend 37722172138](https://github.com/wanghao9103/farsail/actions/runs/37722172138) succeeded at this exact source. The installed production artifact passed preferences/settings restart, actual desktop preflight, reinstall/uninstall retention and debug-probe absence.
- [Release 0.1.11](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.11-70d9393) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.11-70d9393/FarSail_0.1.11_x64-setup.exe). Tag matches exact source. Four public files downloaded anonymously with ordinary verified HTTPS and match tested CI originals. Installer's first download hit an intermittent TLS handshake failure; only that file was retried, without changing trust/proxy/firewall settings.

| Attachment                     |     Bytes | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.11_x64-setup.exe   | 7,916,157 | `f3afc4894f351b079390bfd97cde2ed27061274a1c71085ae7d441e3584b7ef4` |
| SHA256SUMS.txt                 |        96 | `1b9d11736d7e4dc1cf4e7e03872eafebb98794ddc279b1bd8bf40225043a6805` |
| release-metadata.json          |       653 | `b98c0e2c57c4276b30a328b5cd1878b0234f1eae0d2451d14db69127ab32fc09` |
| installation-verification.json |       508 | `bbee5750d3a53c5c5080180e9ef16db8df603b7448bcb4e9ff77d95c866330dd` |

Actual native sharing/watch six-cycle restart fixture independently passed again on0.1.11. Local own test/Vite processes ended; primary WIP, production profile and server unchanged. Both peers should upgrade for the native release fix and viewer capture behavior. The user's uninstrumented original incident needs physical confirmation; higher-integrity/UAC/secure desktop restrictions remain. Knowledge capture updated with the verified mechanism and evidence. Tag-triggered extra CI is checked before final task closure.

Tag-triggered Windows client `37723088627`, transport `37723088702` and backend `37723088610` also succeeded at exact source. All seven relevant CI runs pass. INPUT-021 delivery is complete; root source writer is closed after final documentation push. Interrupt/resume successors should read this record and avoid reimplementing the completed result.
