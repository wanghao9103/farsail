**English** | [简体中文](WI-FIRST-FRAME-046.zh-CN.md)

# WI-FIRST-FRAME-046: system-desktop pauses and Android static first frames

Date: 2026-11-10. Candidate desktop 0.1.43 and Android 0.1.16 (code 17). Publication evidence is recorded separately after the workflows complete.

## Changes

- Windows capture checks whether its thread desktop receives input. On a system desktop switch it clears cached media, capture resources and input geometry, tries to release owned input, and waits up to 120 seconds while checking cancellation and authorization. Returning to the original desktop requires successful input cleanup and new capture. Pauses preserve the previous graphics-reset budget; they do not replenish it. UAC approval remains local. The application manifest and system UAC settings remain unchanged.
- Android can bootstrap an authorized session from its active capture's most recent cached image, preserving the original capture timestamp and layout. Only the first image may use the previous viewport dimensions; subsequent images retain the negotiated-size and live-age checks. Profile negotiation keeps this bootstrap image until delivery. Stop and geometry changes invalidate it.
- MediaProjection defers acquisition of the latest ImageReader image instead of discarding a throttled callback. A reader schedules at most one acquisition; resize and stop remove old work.
- Viewers provide guidance and a manual reconnect action after five seconds without an initial image. Arrival on the same connection clears that guidance.
- A single Switch screen button cycles through available displays. One-display sessions disable it, except when explicitly recovering an unavailable selection. Selection still uses the existing display gate and blocks repeated requests until fresh pixels arrive. Current display details are available in its tooltip.

## Local evidence

Evidence directory: `.local/capture-first-frame-20261110/`.

- Android Rust: 28 passed, two explicit stress experiments ignored; all-target Clippy passed.
- Two real authenticated QUIC regressions cover a static pre-connection image and reconnecting from a smaller previous viewport, with unchanged capture timestamps and stop invalidation.
- Android Kotlin compilation, JVM unit tests and lint passed.
- Frontend production build and ten display-policy tests passed.
- Seven standalone Rust capture-recovery tests passed, including pause-budget preservation. The Windows capture module and its tests passed a GNU cross-target check.
- Chromium synthetic IPC checks passed for delayed initial images, metadata-only viewport startup, four independent viewports, and the existing full viewer suite before the button edit. A focused check after that edit confirmed cycling 1 → 2 → 1, absence of a display dropdown, and the single-display disabled state.

## Limits

Full local Linux desktop tests could not run without GTK/GIO development libraries. Full Windows desktop cross-checking could not finish without the MinGW C compiler. Cloud results must be checked separately before claiming those checks passed. Physical OPPO/ColorOS, Windows UAC approval/refusal, locking with held inputs, two-device WAN latency and installation on personal devices are not covered by local synthetic checks. A cached bootstrap image is the last capture, not a new measurement of current pixels or final presentation latency.

## Compact title bar follow-up

Before publishing, the user requested a closer match to the reference title bar. Desktop title height changes from 36 px to 28 px; the toolbar reveal handle changes from 50 × 22 px to 36 × 16 px and remains directly below the title bar. Tab and window controls shrink together. Fullscreen keeps its existing zero title-height rule. A Chromium synthetic check confirms dimensions and keyboard expansion; the complete suite is being re-run against frozen styles. The earlier desktop candidate was cancelled before publication; its source and cancellation evidence are retained. Android continues from its already tagged source because this follow-up changes desktop CSS only.

## Vertical controls and mouse-exit dismissal

The final requested interaction replaces the horizontal controls row with a narrow vertical dropdown. Desktop settings disclose the resolution selector; supported actions retain their existing authorization and transport paths. Mouse exit closes expanded sections after 650 ms, while keyboard navigation and focused form controls remain accessible. Opening permits 1600 ms to reach the menu. The previous title-bar-only candidate was cancelled before publication. In-app browser inspection and the focused comparison are recorded in the root design QA report; a standard Chromium/WebKit regression also exercises mouse dismissal and retained keyboard focus.
