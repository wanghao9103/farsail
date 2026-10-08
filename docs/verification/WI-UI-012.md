**English** | [简体中文](WI-UI-012.zh-CN.md)

# UI-012 — Desktop workspace and readable connections

## Baseline and scope

- Baseline: `024eb05` (2026-09-29). Existing file-transfer Rust/Cargo changes are unrelated WIP and must be preserved.
- Write set: `apps/desktop/src/main.tsx`, `apps/desktop/src/style.css`, `scripts/test-desktop-ui.cjs`, this document.
- Runtime: existing npm dependencies; loopback Vite on 1420; synthetic IPC/browser fixtures and screenshots in ignored `.local/ui-verification/`. No native account configuration or remote service changes.
- Acceptance: `npm run build`; browser regression covering fixed shell, local scrolling with many devices/requests, readable connection states, invitation permissions, account login records and compact viewports.

## Implementation and evidence

- The shell keeps navigation, title and notifications fixed. Device panes, remote-connection lists and overview activity own their scroll areas. The remote screen takes the remaining height. Settings use two columns; account login records sit beside account/password controls. Small windows retain an accessible scroll area rather than clipping their content.
- “Remote connections” separates current requests, incoming approval, past requests, connecting to others and invitations. Device names and Chinese permission/status labels replace raw IDs/enums; full identifiers remain expandable. Approval and transport connectivity are displayed separately. Account login records explain what signing out another client does.
- Invitation permission is explicit and independent of outgoing-request permission. Generated invitation metadata is snapshotted so changing a selector cannot relabel a previously generated code.
- `npm run build` passed (TypeScript + Vite).
- `scripts/test-desktop-ui.cjs` passed with bundled Playwright and Chrome, synthetic IPC only: existing onboarding/401/offline/cancel/approval flow; 43-device and 47-request overflow fixtures; incoming refusal; outgoing control versus independent view invitation; original invitation device retained after changing selection; sign out another login.
- Fixed document/main shell verified on six pages at 1120×760, 900×580 and 680×700. Overview/devices/connections additionally assert no workspace overflow. At 360×580, the remote-connections page has no document overflow and keeps at least 100px of usable list height. Short-window viewer retains at least 120px of screen area.
- Screenshots reviewed under `.local/ui-verification/`: local-device, remote-device, many-devices, connections, invitation, login-records, overview-fit, settings-fit, viewer-fit, narrow and compact-connections.
- First test pass exposed ambiguous form-control accessible names; explicit select labels fixed lookup and accessibility. Visual review then caught overview overflow and an unusably short compact list despite a stable outer shell; both now have targeted assertions.
- `git diff --check` passed. Existing Rust/Cargo/file-transfer WIP remains untouched. No installer produced, no release published and no real peer/native installer verification claimed. Local preview process stopped after verification.
