# DESIGN-023 visual comparison

final result: blocked

Selected source: `docs/design/desktop-dark-selected.png`, the second displayed generated-image result. Source board inspected; it contains nine related graphite/teal desktop views. Production implementation is the existing React/Tauri client, not a separate mock website. Branding is reused, icons come from Fluent System Icons, remote imagery remains live session content rather than a hardcoded reference wallpaper.

The user explicitly chose “先实现，不使用浏览器自动化” on 2026-10-08. No browser automation, native WebView interaction fixture, screenshot capture or screenshot comparison was run for DESIGN-023. Accordingly no pixel-fidelity, responsive-layout, modal-focus, keyboard-flow or end-to-end UI success is claimed. Build/type checks are not visual evidence.

Implementation intended for manual review: global graphite palette; compact navigation/controls; overview current device and activity; split device workspace; tabbed connection lists; settings with accessible sharing/watch switches and actual endpoint configuration; standalone authentication; security/admin surfaces; overlay viewer tools; operation error and removal confirmation dialogs. Technical errors are collapsed. Progress/state messages remain nonmodal. Generated mock features without an existing production API (such as direct admin user creation) are not added.

Checks available without browser automation: TypeScript/Vite production build; native default-relay/restoration and cancellation/DPAPI tests; native mouse/reconnect regressions; Clippy/fmt/diff checks. UI regression definitions are updated where controls changed but are not executed. Manual screenshot comparison at matching viewport/state is still required before this can become `passed`.

Independent static follow-up found and corrected repeated background-failure dialogs and key carry-over after keyboard dismissal. Four pure Node state regressions pass for deduplication and keyboard quarantine. No browser or DOM was involved, so this does not change the blocked visual/interaction QA result.

INPUT-024 follow-up: user screenshot exposed the primary remote-control action's stacked/clipped SVG and label. Static cascade inspection identified the inherited class-specific grid overriding the element-level flex rule plus a fixed height. The same-class theme override now uses horizontal inline-flex and intrinsic height with a 38px minimum; independent static review found no conflict. User screenshot is defect evidence only. No corrected rendering/screenshot comparison or UI automation was performed; visual QA remains blocked/deferred. Settings now explain the current Windows process elevation and the host-side administrator requirement, without adding an automatic elevation action.
