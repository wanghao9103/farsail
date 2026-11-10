**English** | [简体中文](WI-SYSTEM-HOST-047.zh-CN.md)

# WI-SYSTEM-HOST-047 — UAC desktop service and viewer recovery

## Request and prior limitation

The user confirmed that UAC from other programs must remain visible and controllable, and that Windows system tools such as Task Manager must open remotely. Version 0.1.44 only pauses ordinary-desktop capture during UAC; that behavior does not satisfy secure-desktop operation. The screenshot alone does not establish every first-frame failure's cause.

## Implementation

The Windows installer now includes `farsail-system-host.exe`, installs under protected Program Files with a per-machine NSIS installer, and registers the automatic LocalSystem service `FarSailDesktopHost`. Upgrade hooks stop the previous service before replacing files; uninstall removes it. An old per-user destination is redirected to Program Files, while account/profile storage remains unchanged. The old user-directory copy is not executed as SYSTEM or automatically removed.

The service has no network listener or account tokens. It starts a separate SYSTEM agent in each active interactive session. Each agent attaches worker threads to the input desktop, rebuilding DXGI capture after desktop changes. Session-specific local named pipes reject remote clients. Both ends pin the peer process, executable path and Windows session; clients additionally require a SYSTEM server. Only protected installed FarSail processes can request capture/input. The helper's diagnostic mode can capture into memory but cannot inject input. Remote authorization, sharing, view/control permission and cancellation remain in the desktop runtime. No command interpreter or arbitrary process-launch operation is exposed. UAC policy is unchanged.

An input request must carry the desktop witness from a fresh capture. A click from the ordinary desktop is discarded if UAC has become the active desktop, until its own frame has arrived. Capture/layout resets continue to invalidate coordinates. Connection loss and service stop release owned input. The main application retains its existing administrator manifest; installation of the service is not a claim that startup consent has been removed.

The viewer adds a Task Manager button that sends the existing ordered Ctrl+Shift+Esc input sequence, avoiding local browser/window shortcut interception. A display-selection timeout now continues receiving the requested target, while wrong targets and superseded selections remain rejected. Selecting the same monitor explicitly recreates capture. The empty state offers direct retry/reconnect actions.

More actions open in a separate 320 px side panel on desktop. The primary 208 px menu remains anchored when opening it. Smaller windows use a wider bounded scrolling layout. Mouse-exit dismissal and keyboard/form focus retention remain active.

## Verification

Local evidence: the Windows GNU cross-target check and Clippy cover all targets; ten platform-independent Windows-crate tests pass, including protocol bounds/desktop witness and rejection of unknown operations. Twelve display-policy tests include late target arrival after timeout and rejection after a newer choice. Frontend type checking and production build pass. Evidence is retained in `.local/system-host-20261110`.

Cloud native installation/service capture, untrusted pipe-peer refusal, upgrade/uninstall and complete Edge/WebKit checks are in progress. A normal-desktop service capture probe is not physical UAC acceptance. Actual UAC consent/refusal, Task Manager on the user's Windows machine, locked sessions, RDP, multi-user transitions, old installed-client migration and WAN performance remain unverified. This record is partial until evidence below supersedes these limits.
