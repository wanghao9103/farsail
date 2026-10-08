**English** | [简体中文](WI-UI-014.zh-CN.md)

# UI-014 — Use the system computer name

- Baseline: `024eb05` plus existing uncommitted UI-012/013. Preserve all unrelated file-transfer WIP.
- Write set: desktop `src/main.tsx`, `src-tauri/src/lib.rs`, new `src-tauri/src/computer.rs`, `scripts/test-desktop-ui.cjs`, this document. Existing dependencies and ignored `.local/ui-verification/` runtime outputs only.
- Native state exposes optional `computerName` using Windows `GetComputerNameExW(ComputerNamePhysicalDnsHostname)`. Native failure returns null; form allows manual input without inventing a system name.
- New binding defaults to the system name. On refresh, only this client's exact legacy default name is synchronized through the existing rename API; custom aliases and other devices remain unchanged. A failed migration retains the existing name and reports how to retry. Each device must run the updated client to report its own name.
- API contract references: [GetComputerNameExW](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getcomputernameexw), [COMPUTER_NAME_FORMAT](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ne-sysinfoapi-computer_name_format).

## Verification

- `rustc --edition=2024 --test apps/desktop/src-tauri/src/computer.rs -o .local/ui-verification/computer-name-tests.exe` and the resulting executable passed on Windows. Actual native API returned a nonempty, NUL-free, stable name; the machine identity was not logged.
- `npm run build` passed (TypeScript and Vite). `scripts/test-desktop-ui.cjs` passed existing regression coverage plus: default computer name submitted at binding; only local legacy name migrated once; other legacy-named device unchanged; existing custom alias unchanged; rename failure retains original and allows manual recovery; null hostname leaves manual-name entry available. Screenshot `computer-name.png` reviewed.
- `git diff --check` passed. `rustfmt` scoped to the two touched native files, skipping child modules.
- `cargo check --locked -p farsail-desktop --lib` could not proceed because the existing workspace manifests/lockfile require a lock update. This change adds no dependency and does not alter the existing Cargo/file-transfer WIP. Full native integration/installer build is unverified; no installer or release produced.
- Legacy migration recognizes only the historical exact string `这台 Windows 电脑`; there is no stored name-origin field. It does not track future OS renames or overwrite already-saved names. Each client's startup is responsible only for its own bound device. One attempt per account/server/device per app run avoids repeated failed writes.
