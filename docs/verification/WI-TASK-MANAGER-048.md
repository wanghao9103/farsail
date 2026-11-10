**English** | [简体中文](WI-TASK-MANAGER-048.zh-CN.md)

# WI-TASK-MANAGER-048 — Remote Task Manager launch

The user clarified that clicking Open Task Manager produces no window. Version 0.1.45 sent six Ctrl+Shift+Esc events; its synthetic UI test proved ordering only, not a system process or window. This fix replaces that shortcut with a fixed host action.

The installed interactive desktop process resolves Taskmgr.exe using GetSystemDirectoryW and launches it without a shell, arguments or caller-supplied path. It runs in the logged-in caller's session, never in the SYSTEM broker. It confirms a visible top-level window whose process image matches that system executable and whose session matches the caller. An existing window is reused, restored if minimized, and offered foreground activation. Windows may refuse foreground activation; the acknowledgment confirms a visible window, not guaranteed focus.

The action uses the existing authorized ordered control channel. Only control sessions accept it. The host releases held inputs first. Capability negotiation rejects older hosts explicitly; correlated replies prevent a previous action's response from completing a newer request. One pending request per viewer/host and bounded eight-second window verification / twelve-second reply timeout prevent repeated clicks from creating unbounded work. The button shows pending state and exposes failures. No generic process execution, policy change or SYSTEM process launch is added.

## Evidence and limits

On source `cbc03d4`, dedicated Windows workflow 42781552945 passed both real Task Manager window creation and reuse, followed by the existing system-service capture probe. The test uses a disposable interactive Windows runner and cleans up Taskmgr only after checking that no instance existed beforehand. Local GNU Windows cross-target compilation and Clippy pass; frontend type checking and eleven portable Windows-crate tests pass. Final release-source verification will be recorded after packaging.

This proves the launch helper on hosted Windows. Actual user-machine Task Manager interaction, Windows policy restrictions, UAC consent/refusal, locked/RDP desktops and other system tools remain separate verification scopes.

## Final 0.1.46 delivery and expanded tool acceptance

Immutable product source `a9ca39bff791e1ce7f8bb64aa6a1ea4080eee56e`, publishing run 42782466290: all three native package jobs and both exact-source Edge/WebKit gates passed. Windows installation, authenticated service capture, updater restart, reinstall and uninstall passed. The publish-only retry preserved original package jobs and immutable assets. Public verification checked three updater signatures, eleven checksums, twelve exact publication reconstructions and the ordinary canonical update feed.

Dedicated release-source native workflow 42782468982 also passed real Task Manager window creation/reuse and service capture. Expanded workflow 42786792226, harness source `f73c49921c1c9dd2794e5255e7ef4e323f250deb`, passed all 15 tools below. Compared with the product tag, this later source adds only the test module guarded by cfg(test), its installer-test script and workflow; runtime implementation is unchanged. The test harness runs from protected Program Files under the approved main-executable basename, and requires a nonempty authenticated capture desktop witness. It opens each tool through actual broker Win+R/text events, captures a fresh 960×720 frame, verifies cursor coordinates and an unobscured title-bar click, then closes the newly opened window through broker Alt+F4. No tool settings are changed.

| Tool | Open / capture / mouse / keyboard close |
|---|---|
| Task Manager | passed |
| Device Manager | passed |
| Services | passed |
| Computer Management | passed |
| Event Viewer | passed |
| Disk Management | passed |
| Registry Editor | passed |
| System Information | passed |
| Control Panel | passed |
| System Properties | passed |
| Task Scheduler | passed |
| System Configuration | passed |
| Command Prompt | passed |
| PowerShell | passed |
| Settings | passed |

Earlier failed harness runs remain under .local/task-manager-20261110: hosted images can already contain Performance property-sheet dialogs with different captions; only that exact image fixture is dismissed without applying settings before a fresh property-sheet test. Console HWND ownership may resolve to cmd/powershell rather than conhost. Run command text is read back before Enter, and all commands obey the real 64-character input limit. UWP frames are matched using window relationships and allowed to finish foreground activation before input checks. These are test-condition corrections, not additional product permission changes.

This acceptance covers the host service capture/input path on disposable Windows Server 2022. Physical Windows 10/11, two-machine WAN sessions with these tools, actual UAC consent/refusal, locked desktops, RDP, multi-user transitions, Ctrl+Alt+Del and changing system settings remain unverified. It does not establish that every Windows system function works remotely. Machine-readable evidence is .local/task-manager-20261110/system-tools-verification.json and .local/client-public-0.1.46/delivery-verification.json.
