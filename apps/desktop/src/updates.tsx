import { useCallback, useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Dismiss16Regular } from "@fluentui/react-icons";

type Preferences = { autoCheck: boolean; autoInstall: boolean };
type Status = {
  currentVersion: string;
  availableVersion: string | null;
  phase: string;
  downloaded: number;
  total: number | null;
  message: string;
  preferences: Preferences;
  canPrompt?: boolean;
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

export function useUpdatePrompt(updates: Updates, suspended: boolean) {
  const [open, setOpen] = useState(false);
  const prompted = useRef(new Set<string>());
  const version = updates.status?.availableVersion;
  const show = useCallback(() => {
    if (version) prompted.current.add(version);
    setOpen(true);
  }, [version]);
  const dismiss = useCallback(() => {
    if (version) prompted.current.add(version);
    setOpen(false);
  }, [version]);
  useEffect(() => {
    const status = updates.status;
    if (
      suspended ||
      status?.canPrompt !== true ||
      !status.availableVersion ||
      !["available", "downloading", "ready", "waiting", "error"].includes(
        status.phase,
      ) ||
      prompted.current.has(status.availableVersion) ||
      document.querySelector("dialog[open]")
    )
      return;
    prompted.current.add(status.availableVersion);
    setOpen(true);
  }, [updates.status, suspended]);
  return { open, show, dismiss };
}

function UpdateProgress({ status }: { status: Status }) {
  if (status.phase !== "downloading") return null;
  const knownTotal = status.total !== null && status.total > 0;
  const percent = knownTotal
    ? Math.min(100, Math.round((status.downloaded / status.total!) * 100))
    : null;
  return (
    <div className="update-progress">
      <progress
        aria-label="更新下载进度"
        value={knownTotal ? status.downloaded : undefined}
        max={knownTotal ? status.total! : undefined}
      />
      <span>
        {percent === null
          ? `${Math.round(status.downloaded / 1024)} KB`
          : `${percent}%`}
      </span>
    </div>
  );
}

export function UpdateDialog({
  updates,
  open,
  onDismiss,
}: {
  updates: Updates;
  open: boolean;
  onDismiss: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const description = useId();
  useEffect(() => {
    const node = dialog.current;
    if (!open || !node) return;
    const previous = document.activeElement;
    if (!node.open) node.showModal();
    return () => {
      if (node.open) node.close();
      if (previous instanceof HTMLElement && previous.isConnected)
        previous.focus({ preventScroll: true });
    };
  }, [open]);
  const { status, busy, error, run } = updates;
  if (!status) return null;
  const working =
    busy || ["checking", "downloading", "installing"].includes(status.phase);
  const ready = ["ready", "waiting"].includes(status.phase);
  return (
    <dialog
      ref={dialog}
      className="update-dialog"
      aria-labelledby={title}
      aria-describedby={description}
      onCancel={(event) => {
        event.preventDefault();
        onDismiss();
      }}
      onClose={onDismiss}
      onKeyDown={(event) => event.stopPropagation()}
      onKeyUp={(event) => event.stopPropagation()}
    >
      <button
        type="button"
        className="dialog-close"
        aria-label="关闭更新窗口"
        onClick={onDismiss}
      >
        <Dismiss16Regular aria-hidden="true" />
      </button>
      <div className="update-dialog-body">
        <h2 id={title}>软件更新</h2>
        <p id={description}>
          {status.availableVersion
            ? `FarSail ${status.availableVersion} 已可更新。`
            : "检查并安装 FarSail 更新。"}
        </p>
        <p className="muted">当前版本 {status.currentVersion}</p>
        {(error || !["idle", "available"].includes(status.phase)) && (
          <p role={error || status.phase === "error" ? "alert" : "status"}>
            {error || status.message}
          </p>
        )}
        <UpdateProgress status={status} />
        <p className="hint">
          远程连接结束后安装并重启；Ubuntu 安装时可能需要系统授权。
        </p>
        {status.preferences.autoInstall && (
          <p className="hint">关闭窗口后继续自动更新，可在设置中关闭。</p>
        )}
        <div className="actions update-dialog-actions">
          <button className="secondary" autoFocus onClick={onDismiss}>
            {status.phase === "installing"
              ? "关闭窗口"
              : status.preferences.autoInstall
                ? "后台更新"
                : "稍后"}
          </button>
          <button
            className="primary"
            disabled={working}
            onClick={() =>
              void run(
                ready
                  ? "update_install"
                  : status.availableVersion
                    ? "update_download"
                    : "update_check",
              )
            }
          >
            {status.phase === "downloading"
              ? "正在下载…"
              : status.phase === "installing"
                ? "正在安装…"
                : status.phase === "checking"
                  ? "正在检查…"
                  : ready
                    ? "安装并重启"
                    : status.phase === "error"
                      ? "重试更新"
                      : status.availableVersion
                        ? "立即更新"
                        : "检查更新"}
          </button>
        </div>
      </div>
    </dialog>
  );
}
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
    <div className="alert success update-notice" role="status">
      <span>
        FarSail {status.availableVersion} 可更新。
        {status.phase === "waiting" ? "将在远程连接结束后安装。" : ""}
      </span>
      <button className="text-button update-notice-action" onClick={onOpen}>
        查看更新
      </button>
    </div>
  );
}

export function UpdateSettings({
  updates,
  onOpen,
}: {
  updates: Updates;
  onOpen: () => void;
}) {
  const { status, busy, error, run } = updates;
  if (!status) return null;
  const working =
    busy || ["checking", "downloading", "installing"].includes(status.phase);
  return (
    <div className="card client-updates">
      <h2>软件更新</h2>
      <p className="muted">当前版本 {status.currentVersion}</p>
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
        <span>自动下载并安装</span>
      </label>
      <p className="hint">远程连接结束后安装并重启。</p>
      {(error || status.phase !== "idle") && (
        <p role={error || status.phase === "error" ? "alert" : "status"}>
          {error || status.message}
        </p>
      )}
      <UpdateProgress status={status} />
      <div className="actions">
        <button
          className="secondary"
          disabled={working}
          onClick={() => void run("update_check")}
        >
          检查更新
        </button>
        {status.availableVersion && (
          <button className="primary" onClick={onOpen}>
            查看更新
          </button>
        )}
      </div>
    </div>
  );
}
