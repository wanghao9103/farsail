# UI-013 — Clear actions and complete user flows

## Baseline and scope
- Baseline commit `024eb05`, including the uncommitted UI-012 changes. Preserve unrelated Rust/Cargo/file-transfer WIP.
- Write set: `apps/desktop/src/main.tsx`, `apps/desktop/src/style.css`, `scripts/test-desktop-ui.cjs`, this file. Runtime is loopback Vite 1420 and synthetic browser IPC fixtures under `.local/ui-verification/`.
- Make sharing and approved connections prepare transport automatically; clarify ending connections, invitations, account recovery, device management and admin operations. Preserve server permissions and explicit host consent.
- Acceptance: production frontend build, existing layout/onboarding regression plus action/failure/recovery tests, screenshot inspection. No native installer or public release in this scope.

## Evidence
- Sharing from settings is one action that prepares transport as needed. Relay/bind/force-relay configuration is under advanced settings; applying it explicitly explains that existing local connections/sharing stop.
- Approved requests prepare transport instead of presenting a disabled connection button. View/control connection buttons name their actual permission. New requests advance to current connections; pending requests refresh while waiting. Approval still requires the host action and native/server authorization.
- Request-list and viewer end actions both use `revoke_remote`. They leave the current step on failure for retry and close the view only after success. This differs deliberately from merely closing local transport, which retained the grant. Failure refreshes local state because native operations may already have stopped sharing/transport.
- Invitation metadata lives in App memory, survives page changes, and clears on signed-out state. Copy, collapse/expand and cancel are distinct. Generated metadata remains bound to its original device/permission/email. Collapse does not claim the server-side invite remains usable indefinitely. No clipboard plugin, durable code storage or server change was added.
- Password recovery now advances from email request to reset form to login only after successful responses. Navigation/actions have explicit labels. Raw errors have readable summaries and expandable details. Rename/remove have cancel paths; password/logout explain effects. Admin user/session/audit fields render labels instead of raw JSON.
- `npm run build` passed (TypeScript + Vite). `git diff --check` passed.
- `scripts/test-desktop-ui.cjs` passed with Chrome and synthetic IPC, retaining onboarding/consent/layout coverage and adding: recovery request/reset failure and retry; ordered transport start before sharing; approved-request transport failure/retry; connection end failure/retry and grant revocation; invitation clipboard/collapse/cancel and page navigation persistence; clipboard failure; cancel rename/remove without mutation; readable admin records; admin invitation persistence; password sign-out and invitation memory clearing on next login.
- Existing six-page desktop size matrix and compact 360×580 list-height assertion passed. Reviewed screenshots include sharing-actions, invitation, connections, admin-actions, admin-records and compact-connections under `.local/ui-verification/`.
- Evidence is frontend browser/IPC-fixture verification. Native clipboard, real peer connectivity and new installer lifecycle were not tested. No release or installer was produced. Existing unrelated Rust/Cargo/file-transfer WIP remains outside this change.
