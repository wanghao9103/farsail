**English** | [简体中文](WI-TASK-MANAGER-048.zh-CN.md)

# WI-TASK-MANAGER-048 — Remote Task Manager launch

The user clarified that clicking Open Task Manager produces no window. Version 0.1.45 sent six Ctrl+Shift+Esc events; its synthetic UI test proved ordering only, not a system process or window. This fix replaces that shortcut with a fixed host action.

The installed interactive desktop process resolves Taskmgr.exe using GetSystemDirectoryW and launches it without a shell, arguments or caller-supplied path. It runs in the logged-in caller's session, never in the SYSTEM broker. It confirms a visible top-level window whose process image matches that system executable and whose session matches the caller. An existing window is reused, restored if minimized, and offered foreground activation. Windows may refuse foreground activation; the acknowledgment confirms a visible window, not guaranteed focus.

The action uses the existing authorized ordered control channel. Only control sessions accept it. The host releases held inputs first. Capability negotiation rejects older hosts explicitly; correlated replies prevent a previous action's response from completing a newer request. One pending request per viewer/host and bounded eight-second window verification / twelve-second reply timeout prevent repeated clicks from creating unbounded work. The button shows pending state and exposes failures. No generic process execution, policy change or SYSTEM process launch is added.

## Evidence and limits

On source `cbc03d4`, dedicated Windows workflow 42781552945 passed both real Task Manager window creation and reuse, followed by the existing system-service capture probe. The test uses a disposable interactive Windows runner and cleans up Taskmgr only after checking that no instance existed beforehand. Local GNU Windows cross-target compilation and Clippy pass; frontend type checking and eleven portable Windows-crate tests pass. Final release-source verification will be recorded after packaging.

This proves the launch helper on hosted Windows. Actual user-machine Task Manager interaction, Windows policy restrictions, UAC consent/refusal, locked/RDP desktops and other system tools remain separate verification scopes.
