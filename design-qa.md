**English** | [简体中文](design-qa.zh-CN.md)

# Remote controls dropdown design QA

final result: passed

Scope: the reference image supplied on 2026-11-10 defines the compact title bar, narrow dark vertical controls menu, icon/text rows, disclosure arrows and mouse-exit dismissal. Remote desktop/game pixels and competitor-only functions are outside scope. This is an existing production app change, not a new site.

## Evidence and fixes

The original horizontal toolbar was a P2 mismatch and is replaced by a 208 px vertical menu. The open-details state previously prevented dismissal, and mouse-focused buttons were confused with keyboard focus; both are P2 interaction issues fixed by closing disclosures on mouse exit while preserving genuine keyboard/form focus. Normal exit waits 650 ms so crossing into descendants does not flicker; opening allows 1600 ms to reach the menu.

The in-app browser rendered the actual component against synthetic IPC. It confirmed the menu, keyboard disclosure, and mouse exit returning to the reveal handle. Full capture: `.local/release-20261110/dropdown-cua.jpg`. Combined source/implementation comparison with focused menu crops: `.local/release-20261110/dropdown-comparison.png`. The reference is the supplied clipboard image ending in `90212815-3042-4ed9-b487-62ef4f326bae.png`. Remote content is deliberately excluded from the comparison.

## Required fidelity surfaces

- Typography: existing app system font and readable 13 px menu labels; intentional product-system choice rather than copying another application's font rendering.
- Layout rhythm: 208 px width, 38 px rows, icons aligned with labels, trailing disclosure arrows, compact 28 px title and 36 × 16 px reveal handle. Expanded sections stay within the menu; narrow screens can scroll it.
- Colors/tokens: dark neutral surface, subtle border, muted unavailable actions and existing danger color; no new palette.
- Image quality: no new raster assets; existing Fluent icons render cleanly. Synthetic remote pixels are a test fixture outside the requested control-surface redesign.
- Copy/content: only implemented FarSail actions appear. Competitor-only categories are not represented as nonfunctional controls. Switch screen remains a direct button as requested earlier.

No outstanding P0/P1/P2 findings within this scope. Remaining P3 differences are intentional product labels, system-font rendering and accordion expansion rather than competitor-specific submenus. Cloud full-suite evidence and native-device acceptance are recorded separately; this visual pass does not establish physical Windows UAC or OPPO behavior.

---

# Viewer workspace comparison — 2026-11-05

final result: passed

Scope: the remote viewer chrome requested in this turn, inspired by the supplied Sunlogin screenshot. This does not accept the older DESIGN-023 main-page work or the physical Windows capture fault; those limits and historical evidence remain below.

Source visual truth: `/tmp/codex-clipboard-7761b9b7-6d2c-4c97-94bf-deef62dfe7d9.png` (2995×1737 pixels). Implementation: `.local/viewer-compact-20261105/collapsed-final.png`, `expanded-final.png` (1280×720), and `chrome-final.png` (1280×110). The source and implementation were opened together in one comparison input, including the focused chrome capture. The source’s live desktop/game/account content is outside the viewer-chrome brief and is neither copied into the app nor committed. The implementation uses synthetic session media, not a screenshot wallpaper or a performance claim.

Normalization: compare the source’s top bar/reveal handle and content proportions at the 1280-wide implementation scale, not its inner FarSail application window. Browser screenshots use CSS pixels at density 1. This is reference-based improvement, not a pixel-identical clone: the native 36px titlebar, existing graphite/teal tokens and Fluent icons are intentional. The full normal-view canvas remains 684px of a 720px viewport (95%); expanding the toolbar overlays it rather than changing its bounds. Additional responsiveness checks used 440×900 and the natural 571×571 in-app viewport.

**Findings and comparison history**

- [P2, fixed] Early 440px capture `narrow.png` showed a horizontal scrollbar and a vertical scrollbar competing inside the 36px tab strip. The final strip hides scrollbars while preserving scroll/focus behavior; a named device list provides access to all tabs. `narrow-final.png` confirms the repaired titlebar.
- [P2, fixed] The toolbar used translated slide transitions while rapid reveal/click testing could target moving controls. It now fades at its final coordinates; the reveal action is explicit and outside the titlebar hover zone. Repeated native browser interaction verified reveal did not add remote pointer input; Switch window produced only the intended ordered Alt/Tab presses and releases for the active session.
- No actionable P0/P1/P2 findings remain within this chrome scope. Fonts use the product’s platform-aware UI stack (12px tabs, compact labels), with truncation and explicit accessible names. Spacing keeps a narrow floating toolbar, separate title controls and an overflow device list. Colors use graphite surfaces with teal selection and readable disabled states. Assets are existing Fluent icons; the remote frame remains actual session content in production. Copy describes available actions, measured network RTT and genuine connection state; no invented CPU/GPU telemetry is shown.

**Interactions and limits**

The in-app browser exercised tab clicks, ArrowRight switching, closing one tab, the overflow device list, new-connection IPC intent, toolbar collapse/reveal and fullscreen return. The synthetic IPC record confirmed release followed by Alt-down, Tab-down, Tab-up, Alt-up for only the selected session. Final fresh reload had zero new console errors; earlier hot-reload createRoot warnings are retained and not claimed as a production failure. The full older CLI browser suite was not run in this turn.

The native Linux Xvfb/Openbox smoke passed 23 unauthorized-command denials, fullscreen/window geometry preservation and the new action restoring the existing main window. Initial native attempts lacked the dev server and then used an outdated text-only fullscreen selector; both failures remain in the evidence directory. The final native report is `native-final/report.json`. Windows module compilation passed; actual Windows capture reset and overnight physical operation remain unaccepted.

**Implementation checklist**

- Completed: compact tabs, overflow list, add-connection action, floating tools, direct remote Alt+Tab and viewport checks.
- Completed: focused and full-view comparison after fixes; final browser capture and native window checks.
- Follow-up P3: evaluate native Windows font metrics and very long device names on the user’s display during installation acceptance.

## Historical QA evidence (earlier scope)

# DESIGN-023 visual comparison

final result: blocked

Selected source: `docs/design/desktop-dark-selected.png`, the second displayed generated-image result. Source board inspected; it contains nine related graphite/teal desktop views. Production implementation is the existing React/Tauri client, not a separate mock website. Branding is reused, icons come from Fluent System Icons, remote imagery remains live session content rather than a hardcoded reference wallpaper.

The user explicitly chose “Implement first, without browser automation” on 2026-10-08. No browser automation, native WebView interaction fixture, screenshot capture or screenshot comparison was run for DESIGN-023. Accordingly no pixel-fidelity, responsive-layout, modal-focus, keyboard-flow or end-to-end UI success is claimed. Build/type checks are not visual evidence.

Implementation intended for manual review: global graphite palette; compact navigation/controls; overview current device and activity; split device workspace; tabbed connection lists; settings with accessible sharing/watch switches and actual endpoint configuration; standalone authentication; security/admin surfaces; overlay viewer tools; operation error and removal confirmation dialogs. Technical errors are collapsed. Progress/state messages remain nonmodal. Generated mock features without an existing production API (such as direct admin user creation) are not added.

Checks available without browser automation: TypeScript/Vite production build; native default-relay/restoration and cancellation/DPAPI tests; native mouse/reconnect regressions; Clippy/fmt/diff checks. UI regression definitions are updated where controls changed but are not executed. Manual screenshot comparison at matching viewport/state is still required before this can become `passed`.

Independent static follow-up found and corrected repeated background-failure dialogs and key carry-over after keyboard dismissal. Four pure Node state regressions pass for deduplication and keyboard quarantine. No browser or DOM was involved, so this does not change the blocked visual/interaction QA result.

INPUT-024 follow-up: user screenshot exposed the primary remote-control action's stacked/clipped SVG and label. Static cascade inspection identified the inherited class-specific grid overriding the element-level flex rule plus a fixed height. The same-class theme override now uses horizontal inline-flex and intrinsic height with a 38px minimum; independent static review found no conflict. User screenshot is defect evidence only. No corrected rendering/screenshot comparison or UI automation was performed; visual QA remains blocked/deferred. Settings now explain the current Windows process elevation and the host-side administrator requirement, without adding an automatic elevation action.

ALIGNMENT-025: supplied device screenshot confirms mismatched sidebar footer text edges; metadata inspection found the installed executable is 0.1.13, so its captured primary-action stacking does not prove the uninstalled 0.1.14 correction failed. All application surfaces received source/structure checks and three bounded independent cascade reviews. Shared footer columns, count headers, checkbox row, notice close icon, normal input weights and consistent top viewer control heights are implemented. Superseded 850/1050px sidebar rules and overview block/start overrides are removed; compact navigation removes the gap to hidden anonymous text, long overview names may shrink/wrap, and <=580px device panels explicitly use one column. Existing dimensions, local scrolling, semantics, actions and fonts remain. No new rendered evidence, DPI/font-metric comparison, interaction fixture or screenshot was obtained. Visual/interaction QA therefore remains blocked/deferred, not passed; frontend compilation and installer packaging are separate checks.

RECONNECT-026: source confirms a routine reconnect strip can be covered by the absolute toolbar. Progress now has an independent bottom status layer and centered empty-state text/cancel action, with native attempts/stages. A short success notice survives Viewer session-ID remount and starts after the retained target frame is accepted. Original screen missing/ambiguous and target timeout remain explicit choose-screen states, without default-screen control. Pure model/native progress and deferred-input regressions pass; scoped browser fixture definitions are updated/syntax-checked only. No actual rendering, ARIA announcement, visible overlay, screen-switch focus or physical multi-display reconnect was exercised. Overall visual QA remains blocked/deferred under the existing user instruction.

NOTICE-027: supplied screenshot shows the main viewer-open success notice persisting after reported closure. Main banner and device-detail message now share a typed window-instance owner, verified before publication and observed at500ms only while needed. Native instance survives session reconnect and is invalid when pending/cancelled/missing; other operation messages are unscoped. Pure deferred-query and native identity tests pass; main UI fixture contract is updated/syntax checked only. No actual window close, main-page layout reflow, status announcement or installation was exercised, so visual/interaction QA remains blocked/deferred. The snapshot's personal data is not persisted in the repo.
