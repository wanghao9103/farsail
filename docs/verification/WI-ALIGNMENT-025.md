**English** | [简体中文](WI-ALIGNMENT-025.zh-CN.md)

# ALIGNMENT-025 — desktop text and control alignment

Baseline `f1c3ac1`, managed mouse-control-022 worktree. User requests all-page alignment checks from an attached device-page screenshot. Installed executable metadata reports 0.1.13; remote-primary stacking was already corrected in the uninstalled 0.1.14 candidate. Do not infer that the running window necessarily uses the inspected executable. The new screenshot confirms footer text uses inconsistent left edges and the primary action remains stacked in that captured version.

Read-only knowledge bootstrap: the existing desktop workspace note applies to preserving finite heights, local scrolling, narrow usable panes and distinguishing build checks from rendering evidence. Its prior browser results describe older versions, not current verification. Current user instruction continues to prohibit browser automation. Product Design audit guidance is used for the supplied screenshot; other surfaces receive source/cascade checks, not a claimed full visual audit. Saved Product Design context is absent. Do not copy screenshot account/device data into tracked files or knowledge notes.

Work item: align common text/icon/status controls without redesigning or changing behavior. Write set: `apps/desktop/src/main.tsx`, `style.css`, `desktop-theme.css`, candidate Tauri version, this verification record and existing design QA record. No native runtime/auth/input, installed application, user profile, server or dependency changes. Primary checkout WIP remains outside the write set. Cache: existing Node dependencies and shared Rust target; deliver new local candidate under primary `.local/alignment-025`.

Findings from screenshot/source and three independent read-only reviews:

1. Sidebar status dot plus bare text, br and small use different left edges; account row also has a different icon/text column. Use shared fixed icon/text columns and center the status marker on the first line.
2. Success notification lacks vertical centering, and the larger text close glyph has a different line box. Use centered layout and a fixed Fluent close icon target.
3. Force-relay checkbox inherits grid form-label layout and control content inherits bold labels. Use a dedicated horizontal checkbox row and explicit normal control font weight.
4. Header counts remain inline spans with vertical padding outside the line box. Use an aligned heading row and self-contained centered badge.
5. Legacy 1050px sidebar collapse conflicts with the 900px theme breakpoint; higher-specificity old selectors hide text inside a wide sidebar. Remove superseded sidebar rules while retaining content-pane responsive rules.
6. Legacy overview block/start rules override the theme's device/action row and right alignment. Remove those overrides, retaining narrow reflow.
7. Viewer top select/buttons/summary use different control heights. Align top controls without affecting popover forms.
8. Theme's global device columns override the legacy narrow single-column layout. Restore a single column explicitly at <=580px, retaining bounded panel rows. Align narrow title size with its actual selector.

Final static review additionally caught icon-only navigation retaining a 12px gap to its zero-width anonymous text item, offsetting the icon from center. The compact breakpoint now removes that gap and centers the brand icon. Restoring the overview flex row also requires a shrinkable text column and wrapping for long device names; add min-width:0, explicit gap and a shrinkable action column without removing the local-scroll boundary.

Coverage: overview, device list/detail/local sharing/approval, connection forms/lists/invitation, sharing/transport settings, account/security and admin tables, authentication, feedback dialogs and viewer chrome. No additional reachable button grid/text stacking, summary multi-element layout or device-row/window-control centering defect was found. Screenshot/accessibility evidence remains limited to the supplied state; no new browser, native UI interaction or OS input is run.

Acceptance: independent static review of final cascade/structure, TypeScript/Vite production build, scoped Prettier/diff checks, release/NSIS build and installer copy SHA256/version checks. No implementation-mirroring CSS tests. Visual appearance, DPI/font metrics, modal focus and installation lifecycle remain manual verification. Candidate only, no public release/install.

Final source checks: three independent reviewers inspected all page structures/cascades. Follow-up issues (compact anonymous-text gap and long overview names) were corrected with breakpoint gap reset, wrapping/shrinkable columns and narrow action-column reset. TypeScript/Vite build, scoped Prettier and diff check passed; four existing pure Node feedback-policy regressions passed. Native implementation is unchanged, so prior Rust checks are not rerun or reported as new results. Final-source Tauri packaging runs its own production frontend build. These checks do not establish rendered alignment; supplied screenshot remains the only visual evidence in this work item.

Final delivery: embedded source `4b68d7f6701bc2c2a8d9c274d7fc23d6b62ee958`; final-source frontend and Tauri release/NSIS build passed, executable ProductVersion 0.1.15. Primary `.local/alignment-025/FarSail_0.1.15_x64-setup.exe` is 9,016,807 bytes, SHA256 `0267ed218ae0ba7f00ff5694b3be90975f7ee201bda5027872a79c9b39ee81f7`; source/copy hashes match. Metadata and SHA256SUMS.txt are beside the installer. No installation, browser, native UI or production change was performed. Reusable cascade/alignment lesson is saved as an Obsidian inbox note with partial verification.
