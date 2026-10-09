import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Preferences = { autoCheck: boolean; autoInstall: boolean };
type Status = {
  currentVersion: string;
  availableVersion: string | null;
  phase: string;
  downloaded: number;
  total: number | null;
  message: string;
  preferences: Preferences;
};

export function useClientUpdates(enabled: boolean) {
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const active = useRef(false);
  const accept = useCallback((next: Status) => {
    if (next?.currentVersion && next.preferences) setStatus(next);
  }, []);
  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let reading = false;
    const read = async () => {
      if (reading) return;
      reading = true;
      try {
        const next = await invoke<Status>("update_status");
        if (!disposed) accept(next);
      } catch {
        // An update check must not interrupt login or an active remote session.
      } finally {
        reading = false;
      }
    };
    void read();
    const timer = window.setInterval(() => void read(), 1000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [enabled, accept]);
  const run = async (command: string, args?: Record<string, unknown>) => {
    if (active.current) return;
    active.current = true;
    setBusy(true);
    setError("");
    try {
      accept(await invoke<Status>(command, args));
    } catch (failure) {
      setError(String(failure));
    } finally {
      active.current = false;
      setBusy(false);
    }
  };
  return { status, busy, error, run };
}

type Updates = ReturnType<typeof useClientUpdates>;
export function UpdateNotice({
  updates,
  onOpen,
}: {
  updates: Updates;
  onOpen: () => void;
}) {
  const status = updates.status;
  if (
    !status?.availableVersion ||
    !["available", "ready", "waiting", "downloading"].includes(status.phase)
  )
    return null;
  return (
    <div className="alert success" role="status">
      <span>
        FarSail {status.availableVersion} 可更新。
        {status.phase === "waiting" ? "将在远程连接结束后安装。" : ""}
      </span>
      <button className="text-button" onClick={onOpen}>
        查看更新
      </button>
    </div>
  );
}

export function UpdateSettings({ updates }: { updates: Updates }) {
  const { status, busy, error, run } = updates;
  if (!status) return null;
  const working =
    busy || ["checking", "downloading", "installing"].includes(status.phase);
  const percent = status.total
    ? Math.min(100, Math.round((status.downloaded / status.total) * 100))
    : null;
  return (
    <div className="card client-updates">
      <h2>软件更新</h2>
      <p className="muted">
        当前版本 {status.currentVersion}
        {status.availableVersion ? ` · 新版本 ${status.availableVersion}` : ""}
      </p>
      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={status.preferences.autoCheck}
          disabled={busy}
          onChange={(event) =>
            void run("update_preferences", {
              preferences: {
                ...status.preferences,
                autoCheck: event.target.checked,
              },
            })
          }
        />
        <span>自动检查更新</span>
      </label>
      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={status.preferences.autoInstall}
          disabled={busy}
          onChange={(event) =>
            void run("update_preferences", {
              preferences: {
                autoCheck: event.target.checked || status.preferences.autoCheck,
                autoInstall: event.target.checked,
              },
            })
          }
        />
        <span>自动下载，并在远程连接结束后安装更新</span>
      </label>
      <p className="hint">安装后会重新启动。Ubuntu 可能弹出系统授权窗口。</p>
      <p role="status">{error || status.message}</p>
      {status.phase === "downloading" && (
        <div>
          <progress
            aria-label="更新下载进度"
            value={status.downloaded}
            max={status.total || undefined}
          />
          <span>
            {percent === null
              ? `${Math.round(status.downloaded / 1024)} KB`
              : `${percent}%`}
          </span>
        </div>
      )}
      <div className="actions">
        <button disabled={working} onClick={() => void run("update_check")}>
          检查更新
        </button>
        {status.availableVersion &&
          !["ready", "waiting", "installing"].includes(status.phase) && (
            <button
              className="primary"
              disabled={working}
              onClick={() => void run("update_download")}
            >
              下载更新
            </button>
          )}
        {["ready", "waiting"].includes(status.phase) && (
          <button
            className="primary"
            disabled={working}
            onClick={() => void run("update_install")}
          >
            安装并重启
          </button>
        )}
      </div>
    </div>
  );
}
