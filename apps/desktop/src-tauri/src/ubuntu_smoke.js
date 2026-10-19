// Debug-only probe of the actual Ubuntu WebKitGTK UI and native command bridge.
(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const waitFor = async (check) => {
    for (let n = 0; n < 100; n++) {
      const value = check();
      if (value) return value;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw Error("Ubuntu UI did not become ready");
  };
  try {
    const state = await invoke("state");
    if (
      state.platform !== "linux" ||
      state.canShareLocalScreen !== false ||
      !state.computerName
    )
      throw Error("incorrect native platform metadata");
    const files = await invoke("files_status");
    if (
      state.remoteAccessEnabled !== false ||
      state.sharing ||
      state.filesEnabled ||
      files.enabled ||
      files.sessions.length
    )
      throw Error("remote access must start disabled");
    let pathBlocked = false;
    try {
      await invoke("plugin:fs|read_file", { path: "/etc/hostname" });
    } catch (error) {
      pathBlocked = String(error).includes("not allowed");
    }
    if (!pathBlocked)
      throw Error("filesystem plugin must not grant webview path access");
    const settings = await waitFor(() =>
      Array.from(document.querySelectorAll("nav button")).find((b) =>
        b.textContent.includes("共享与设置"),
      ),
    );
    settings.click();
    await waitFor(() =>
      document.body.textContent.includes(
        "Ubuntu 暂不支持共享本机屏幕或远程值守。",
      ),
    );
    const remoteAccess = document.querySelector(
      '[aria-label="允许远程连接"][role="switch"]',
    );
    if (!remoteAccess || remoteAccess.checked || !remoteAccess.disabled)
      throw Error(
        "signed-out remote-access switch must be off and unavailable",
      );
    if (
      document.querySelector('[aria-label="允许接收文件"][role="switch"]') ||
      document.querySelector('[aria-label="同账号远程值守"][role="switch"]') ||
      document.querySelector('[aria-label="本机屏幕共享"][role="switch"]') ||
      document.body.textContent.includes("Windows 运行权限")
    )
      throw Error("unsupported host controls are visible");
    const root = document.documentElement;
    if (
      root.scrollWidth > window.innerWidth ||
      root.scrollHeight > window.innerHeight
    )
      throw Error("Ubuntu workspace overflows");
    const update = await invoke("update_status");
    if (
      !update.currentVersion ||
      !update.preferences.autoCheck ||
      update.preferences.autoInstall ||
      update.canPrompt !== true
    )
      throw Error("incorrect native update defaults");
    await invoke("update_preferences", {
      preferences: { autoCheck: false, autoInstall: false },
    });
    if ((await invoke("update_status")).preferences.autoCheck)
      throw Error("native update preferences were not applied");
    await invoke("set_server", { server: "http://127.0.0.1:8787" });
    const media = new Uint8Array(await invoke("ipc_media_smoke"));
    if (String.fromCharCode(...media.slice(0, 4)) !== "FSM1")
      throw Error("binary media IPC failed");
    await invoke("ipc_smoke_report", {
      result: JSON.stringify({
        ok: true,
        platform: state.platform,
        filesDefaultOff: true,
        remoteAccessDefaultOff: true,
        unifiedRemoteAccessControl: true,
        webviewFilesystemDenied: true,
        hostControlsHidden: true,
        updatePromptAvailable: true,
        workspaceFits: true,
        mediaBytes: media.length,
      }),
    });
  } catch (e) {
    await invoke("ipc_smoke_report", {
      result: JSON.stringify({ ok: false, error: String(e) }),
    });
  }
})();
