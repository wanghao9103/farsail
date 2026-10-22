// Debug-only lifecycle probe. The harness owns an isolated profile, D-Bus and tray host.
(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const pause = () => new Promise((resolve) => setTimeout(resolve, 200));
  const waitFor = async (check) => {
    for (let n = 0; n < 150; n++) {
      const state = await invoke("ipc_desktop_state");
      if (check(state)) return state;
      await pause();
    }
    throw Error("desktop lifecycle did not reach the expected state");
  };
  const report = (value) =>
    invoke("ipc_smoke_report", { result: JSON.stringify(value) });
  const close = () => invoke("plugin:window|close", { label: "main" });
  try {
    await waitFor((state) => state.visible && !state.minimized);
    const before = await invoke("state");
    await close();
    const closed = await waitFor((state) =>
      state.trayCreated && state.trayHost
        ? !state.visible
        : state.visible && state.minimized,
    );
    if (closed.quitting)
      throw Error("closing the window started application shutdown");
    const after = await invoke("state");
    if (
      before.remoteAccessEnabled !== after.remoteAccessEnabled ||
      before.filesEnabled !== after.filesEnabled ||
      before.sharing !== after.sharing ||
      before.transportRunning !== after.transportRunning
    ) {
      throw Error("closing the window changed remote access state");
    }
    await report({ phase: "closed", ...closed });
    await waitFor((state) => state.visible && !state.minimized);
    if (closed.trayCreated && closed.trayHost) {
      await close();
      await waitFor((state) => !state.visible);
      await report({ phase: "closed-again", singleInstanceRestored: true });
      await waitFor((state) => state.visible && !state.minimized);
    }
    await report({
      ok: true,
      closeToTray: closed.trayCreated && closed.trayHost,
      noTrayHostMinimized: !(closed.trayCreated && closed.trayHost),
      singleInstanceRestored: true,
      trayMenuRestored: closed.trayCreated && closed.trayHost,
      remoteStatePreserved: true,
    });
    if (!closed.trayCreated || !closed.trayHost) await invoke("app_exit");
  } catch (error) {
    await report({ ok: false, error: String(error) });
  }
})();
