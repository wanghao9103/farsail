import React, { useCallback, useEffect, useId, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Device, Me, Pending, Remote, User } from "@farsail/ui";
import {
  useClientUpdates,
  useUpdatePrompt,
  UpdateDialog,
  UpdateNotice,
  UpdateSettings,
} from "./updates";
import { FilesPanel } from "./files";
import { PageActions } from "./page-actions";
import "./style.css";
import "./desktop-theme.css";
import { RefreshFailureGate } from "./feedback-policy";
import {
  ViewerNoticeLease,
  inspectViewerNotice,
  publishViewerNotice,
} from "./viewer-notice-policy";
import {
  ViewerDisplayGate,
  recoveryStageText,
  dispatchViewerInput,
  type RemoteDisplay,
  type DisplaySnapshot,
  type DisplayTicket,
  type RecoveryStatus,
} from "./viewer-display-policy";
import { FailureDialog, failureMessage, type FailureAction } from "./feedback";
import {
  Home20Regular,
  Desktop20Regular,
  Eye20Regular,
  Folder20Regular,
  ArrowSwap20Regular,
  Shield20Regular,
  Settings20Regular,
  People20Regular,
  Dismiss16Regular,
  Subtract16Regular,
  Square16Regular,
  ChevronRight16Regular,
  ChevronLeft16Regular,
  ArrowSync20Regular,
} from "@fluentui/react-icons";

type Tab =
  | "overview"
  | "devices"
  | "requests"
  | "files"
  | "viewer"
  | "security"
  | "admin"
  | "settings";
type PublicState = {
  platform?: string;
  canShareLocalScreen?: boolean;
  computerName?: string | null;
  administratorMode?: boolean | null;
  server: string;
  signedIn: boolean;
  deviceId: string | null;
  deviceRestoreError?: string | null;
  filesEnabled?: boolean;
  remoteAccessEnabled?: boolean;
  sharing: boolean;
  remoteWatch?: boolean;
  transportRunning?: boolean;
  defaultRelay?: string | null;
  transportConfiguration?: { relay: string | null; forceRelay: boolean } | null;
  sharePreferences?: {
    sharing: boolean;
    watch: boolean;
    restore: "idle" | "pending" | "ready" | "failed";
  };
};
const canShareLocalScreen = (state: PublicState) =>
  state.canShareLocalScreen ?? state.platform !== "linux";
const remoteAccessEnabled = (state: PublicState) =>
  state.remoteAccessEnabled ?? state.filesEnabled ?? state.sharing;
const remoteAccessReady = (state: PublicState) =>
  remoteAccessEnabled(state) && (!canShareLocalScreen(state) || state.sharing);
const sharingChoice = (state: PublicState) =>
  remoteAccessEnabled(state) || !!state.sharePreferences?.sharing;
const watchChoice = (state: PublicState) =>
  state.remoteWatch || !!state.sharePreferences?.watch;
function SharingRecovery({ state }: { state: PublicState }) {
  if (remoteAccessReady(state) || !state.sharePreferences?.sharing) return null;
  return (
    <p className="hint" role="status">
      {remoteAccessEnabled(state)
        ? "文件传输仍可用，屏幕共享已停止。请检查 Windows 桌面后重试。"
        : state.sharePreferences.restore === "pending"
          ? "正在恢复远程连接…"
          : `本次未恢复远程连接，请检查登录和网络${canShareLocalScreen(state) ? "、Windows 桌面" : ""}后重试。`}
    </p>
  );
}
function RemoteAccessControls({
  state,
  busy,
  act,
  prepareTransport,
}: {
  state: PublicState;
  busy: boolean;
  act: Action;
  prepareTransport: () => Promise<void>;
}) {
  const descriptionId = useId();
  return (
    <div className="preference-rows">
      <label className="preference-row">
        <span>
          <strong>允许远程连接</strong>
          <small id={`${descriptionId}-access`}>
            {canShareLocalScreen(state)
              ? "屏幕控制与文件传输，默认每次连接需批准。"
              : "文件传输，每次连接需批准。"}
          </small>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-label="允许远程连接"
          aria-describedby={`${descriptionId}-access`}
          checked={sharingChoice(state)}
          disabled={busy || !state.deviceId}
          onChange={() =>
            void act(
              async () => {
                if (!sharingChoice(state)) await prepareTransport();
                await invoke(
                  sharingChoice(state) ? "share_disable" : "share_enable",
                );
              },
              sharingChoice(state) ? "远程连接已关闭" : "远程连接已允许",
            )
          }
        />
      </label>
      {canShareLocalScreen(state) ? (
        <label className="preference-row">
          <span>
            <strong>同账号远程值守</strong>
            <small id={`${descriptionId}-watch`}>
              同账号设备可直接控制屏幕并传输文件。
            </small>
          </span>
          <input
            type="checkbox"
            role="switch"
            aria-label="同账号远程值守"
            aria-describedby={`${descriptionId}-watch`}
            checked={watchChoice(state)}
            disabled={busy || (!state.sharing && !watchChoice(state))}
            onChange={() =>
              void act(
                () => invoke("remote_watch", { enabled: !watchChoice(state) }),
                watchChoice(state) ? "已关闭远程值守" : "已开启远程值守",
              )
            }
          />
        </label>
      ) : (
        <p className="hint" role="status">
          Ubuntu 暂不支持共享本机屏幕或远程值守。
        </p>
      )}
      <details className="sharing-guidance">
        <summary>运行与恢复说明</summary>
        <p className="hint">
          重启后尝试恢复已开启的远程连接。
          {canShareLocalScreen(state) && "屏幕共享需 Windows 已登录。"}
        </p>
      </details>
      <SharingRecovery state={state} />
      {!remoteAccessReady(state) &&
        state.sharePreferences?.sharing &&
        state.sharePreferences.restore !== "pending" && (
          <button
            disabled={busy || !state.deviceId}
            className="secondary"
            onClick={() =>
              void act(async () => {
                await prepareTransport();
                await invoke("share_enable");
              }, "远程连接已恢复")
            }
          >
            重试远程连接
          </button>
        )}
    </div>
  );
}
type ConnectionInvitation = {
  id: string;
  code: string;
  target: string;
  permission: "view" | "control" | "files";
  visible: boolean;
};
type SignupInvitation = {
  id: string;
  code: string;
  email: string;
  visible: boolean;
};
const api = <T,>(op: string, args: Record<string, unknown> = {}): Promise<T> =>
  invoke("call", { op, args });
const native = "__TAURI_INTERNALS__" in window;
const HANDLED_FAILURE = Symbol("handled failure");
const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);
const readableError = (raw: string) => {
  if (/local sign-out complete/.test(raw))
    return "本机已退出登录，但服务器尚未确认。请在网络恢复后检查其他登录记录。";
  if (/HTTP 400\b/.test(raw))
    return "提交的信息无效或已过期，请检查填写内容后重试。";
  if (/HTTP 401\b|signed out/i.test(raw))
    return "登录信息无效或已过期，请重新登录；新账号请先验证邮箱。";
  if (/HTTP 403\b/.test(raw))
    return "目前不允许执行此操作，请检查账号权限、设备共享和对方批准状态。";
  if (/HTTP 404\b/.test(raw)) return "未找到该设备或记录，请刷新后重试。";
  if (/HTTP 409\b/.test(raw)) return "状态已发生变化，请刷新后重试。";
  if (/HTTP 429\b/.test(raw)) return "操作过于频繁，请稍后再试。";
  if (/HTTP 422\b/.test(raw) && /unknown variant [`"']linux/.test(raw))
    return "服务器版本尚不支持 Ubuntu。请先升级协调服务，再添加这台电脑。";
  if (/HTTP 422\b/.test(raw))
    return "服务器无法识别提交的信息，请检查客户端与服务器版本是否兼容。";
  if (/no interactive display/i.test(raw))
    return "无法获取本机屏幕，请先登录 Windows 桌面再开启共享。";
  if (/local screen sharing is only supported on Windows/i.test(raw))
    return "Ubuntu 暂不支持共享本机屏幕，可连接已开启共享的 Windows 电脑。";
  if (/Ubuntu Secret Service unavailable/i.test(raw))
    return "无法访问 Ubuntu 密钥环，请在桌面会话中解锁登录密钥环后重试。";
  if (/relay URL|relay did not become reachable/i.test(raw))
    return "无法连接中继服务器，请在高级连接设置中检查 HTTPS 地址和网络。";
  if (/invalid UDP bind/i.test(raw))
    return "本机监听地址格式不正确，可恢复为 0.0.0.0:0 后重试。";
  if (/unbound|device not bound/i.test(raw))
    return "请先将这台电脑添加到账号，再尝试连接。";
  if (/[\u3400-\u9fff]/.test(raw)) return raw;
  return failureMessage(raw);
};
const copyText = async (value: string) => {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    throw new Error("无法自动复制，请选中显示的内容后手动复制。");
  }
};
const short = (id: string) => id.slice(0, 8);
const permissionLabel = (value: string) =>
  ({ view: "仅查看屏幕", control: "控制与文件传输", files: "文件传输" })[
    value
  ] ?? "未知权限";
const requestLabel = (value: string) =>
  ({
    pending: "等待批准",
    approved: "已批准",
    denied: "已拒绝",
    revoked: "已取消或结束",
    expired: "已过期",
  })[value] ?? "状态待确认";
const pathLabel = (value?: string, discovery?: string) => {
  if (value === "relay") {
    if (discovery === "forced_relay") return "已按设置强制中继";
    if (discovery === "trying_direct") return "中继传输中 · 正在尝试直连";
    if (discovery === "refresh_failed")
      return "中继传输中 · 地址刷新失败，将重试";
    if (discovery === "relay_fallback") return "中继传输中 · 定时重试直连";
  }
  return (
    {
      direct: "已直连",
      relay: "已通过中继连接",
      connected: "已连接",
      connecting: "正在连接",
      closed: "已断开",
    }[value ?? ""] ?? "尚未连接"
  );
};
const deviceLabel = (id: string, devices: Device[], localId: string | null) =>
  `${devices.find((d) => d.id === id)?.name ?? `设备 ${short(id)}`}${id === localId ? "（本机）" : ""}`;
const dateLabel = (value: string) => new Date(value).toLocaleString();
type OperationNotice = { message: string; viewer?: ViewerNoticeLease };

const readViewerWindow = (instance: string) =>
  invoke<boolean>("viewer_window_active", { instance });
function useViewerNoticePresent(lease: ViewerNoticeLease | null) {
  const [closed, setClosed] = useState<ViewerNoticeLease | null>(null);
  useEffect(() => {
    if (!lease) return;
    let current = true;
    let inFlight = false;
    const inspect = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        await inspectViewerNotice(
          lease,
          readViewerWindow,
          () => current,
          setClosed,
        );
      } finally {
        inFlight = false;
      }
    };
    void inspect();
    const timer = window.setInterval(() => void inspect(), 500);
    return () => {
      current = false;
      window.clearInterval(timer);
    };
  }, [lease]);
  return !lease || (closed !== lease && lease.isPresent());
}

function App() {
  const updates = useClientUpdates(native);
  const [publicState, setPublicState] = useState<PublicState>({
    server: "http://127.0.0.1:8787",
    signedIn: false,
    deviceId: null,
    sharing: false,
  });
  const [me, setMe] = useState<Me | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [fileSession, setFileSession] = useState<string | null>(null);
  const openFiles = (id: string) => {
    setFileSession(id);
    setTab("files");
  };
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<OperationNotice | null>(null);
  const actionVersion = useRef(0);
  const noticePresent = useViewerNoticePresent(notice?.viewer ?? null);
  useEffect(() => {
    if (!noticePresent)
      setNotice((current) =>
        current?.viewer === notice?.viewer ? null : current,
      );
  }, [noticePresent, notice]);
  const openViewer = useCallback(async (id: string) => {
    const lease = new ViewerNoticeLease(
      await invoke<string>("viewer_open", { id }),
    );
    await lease.check(readViewerWindow);
    return lease;
  }, []);
  const [problem, setProblem] = useState("");
  const [failureAction, setFailureAction] = useState<FailureAction>();
  const [devices, setDevices] = useState<Device[]>([]);
  const [requests, setRequests] = useState<Remote[]>([]);
  const [pending, setPending] = useState<Pending[]>([]);
  const [server, setServer] = useState("http://127.0.0.1:8787");
  const [relayUrl, setRelayUrl] = useState("");
  const [bindAddr, setBindAddr] = useState("0.0.0.0:0");
  const [forceRelay, setForceRelay] = useState(false);
  const [transportReady, setTransportReady] = useState(false);
  const [connectionInvite, setConnectionInvite] =
    useState<ConnectionInvitation | null>(null);
  const [signupInvite, setSignupInvite] = useState<SignupInvitation | null>(
    null,
  );
  const [authMode, setAuthMode] = useState<
    "login" | "register" | "verify" | "recover" | "reset"
  >("login");
  const updatePrompt = useUpdatePrompt(
    updates,
    busy ||
      !!problem ||
      !!connectionInvite ||
      !!signupInvite ||
      (!me && authMode !== "login"),
  );
  const refreshVersion = useRef(0);
  const refreshFailures = useRef(new RefreshFailureGate());
  const nameMigrationAttempts = useRef(new Set<string>());

  const refresh = useCallback(async (foreground = true) => {
    if (!native) {
      setProblem(
        "浏览器预览仅用于检查布局；请运行 Tauri 客户端使用账号与设备功能。",
      );
      return;
    }
    const version = ++refreshVersion.current;
    const p = await invoke<PublicState>("state");
    if (version !== refreshVersion.current) return;
    setPublicState(p);
    setServer(p.server);
    setRelayUrl((previous) => {
      if (previous) return previous;
      if (p.transportConfiguration) return p.transportConfiguration.relay ?? "";
      if (p.defaultRelay !== undefined) return p.defaultRelay ?? "";
      try {
        const url = new URL(p.server);
        if (url.protocol !== "https:") return "";
        url.port = "8443";
        url.pathname = "/";
        url.search = "";
        url.hash = "";
        return url.toString();
      } catch {
        return "";
      }
    });
    if (!p.signedIn) {
      refreshFailures.current.recovered();
      setConnectionInvite(null);
      setFileSession(null);
      setSignupInvite(null);
      setTransportReady(false);

      setMe(null);
      setDevices([]);
      setRequests([]);
      setPending([]);
      return;
    }
    try {
      const current = await api<Me>("resume");
      if (version !== refreshVersion.current) return;
      setMe(current);
      const [all, sessions] = await Promise.all([
        api<Device[]>("devices"),
        api<Remote[]>("remote_sessions"),
      ]);
      if (version !== refreshVersion.current) return;
      setRequests(sessions);
      const q = await invoke<PublicState>("state");
      if (version !== refreshVersion.current) return;
      setPublicState(q);
      if (
        q.deviceRestoreError &&
        refreshFailures.current.shouldReport(q.deviceRestoreError, foreground)
      ) {
        setFailureAction(undefined);
        setProblem(
          `已登录，但本机连接尚未恢复，请检查网络后刷新。技术详情：${q.deviceRestoreError}`,
        );
      }
      if (typeof q.transportRunning === "boolean")
        setTransportReady(q.transportRunning);
      const local = all.find((d) => d.id === q.deviceId);
      const computerName = q.computerName?.trim();
      const migrationKey = `${q.server}/${current.id}/${q.deviceId}`;
      // Upgrade only the legacy default of this client, never another device or a custom alias.
      if (
        local?.name === "这台 Windows 电脑" &&
        computerName &&
        computerName !== local.name &&
        !nameMigrationAttempts.current.has(migrationKey)
      ) {
        nameMigrationAttempts.current.add(migrationKey);
        try {
          await api("rename_device", { id: local.id, name: computerName });
          if (version !== refreshVersion.current) return;
          local.name = computerName;
        } catch {
          if (version !== refreshVersion.current) return;
          setProblem(
            "未能同步本机的计算机名称，暂时保留原名称。你可以在设备信息与管理中修改名称，或重新打开应用后重试。",
          );
        }
      }
      if (version !== refreshVersion.current) return;
      setDevices(all);
      if (q.deviceId) {
        const rows = await api<Pending[]>("pending");
        if (version === refreshVersion.current) setPending(rows);
      } else setPending([]);
      if (version === refreshVersion.current && !q.deviceRestoreError)
        refreshFailures.current.recovered();
    } catch (e) {
      const error = errorText(e);
      if (
        version === refreshVersion.current &&
        refreshFailures.current.shouldReport(error, foreground)
      ) {
        setFailureAction(undefined);
        setProblem(error);
      }
    }
  }, []);
  useEffect(() => {
    void refresh().catch((e) => setProblem(errorText(e)));
  }, [refresh]);
  useEffect(() => {
    if (!native || publicState.sharePreferences?.restore !== "pending") return;
    const timer = window.setInterval(() => {
      const version = refreshVersion.current;
      void invoke<PublicState>("state")
        .then((next) => {
          if (version !== refreshVersion.current) return;
          setPublicState(next);
          if (typeof next.transportRunning === "boolean")
            setTransportReady(next.transportRunning);
        })
        .catch(() => {});
    }, 2000);
    return () => window.clearInterval(timer);
  }, [publicState.sharePreferences?.restore]);
  useEffect(() => {
    if (!me) return;
    const timer = window.setInterval(() => {
      void refresh(false).catch(() => {});
    }, 15000);
    return () => window.clearInterval(timer);
  }, [me?.id, refresh]);
  const act = async (
    work: () => Promise<unknown>,
    message: string,
    reload = true,
    failureAction?: FailureAction,
  ) => {
    const version = ++actionVersion.current;
    const current = () => version === actionVersion.current;
    setBusy(true);
    setProblem("");
    setFailureAction(undefined);
    setNotice(null);
    try {
      const result = await work();
      if (result === HANDLED_FAILURE) return;
      if (result instanceof ViewerNoticeLease) {
        await publishViewerNotice(result, readViewerWindow, current, (viewer) =>
          setNotice(viewer ? { message, viewer } : null),
        );
      } else if (current()) setNotice(message ? { message } : null);
      if (reload && current()) await refresh();
    } catch (e) {
      // Failed mutations may still stop sharing or clear local credentials.
      if (reload && current()) await refresh().catch(() => {});
      if (current()) {
        setProblem(errorText(e));
        setFailureAction(failureAction);
      }
    } finally {
      if (current()) setBusy(false);
    }
  };
  const resetConnection = () => {
    setTransportReady(false);
  };
  const prepareTransport = async () => {
    const current = await invoke<PublicState>("state");
    if (current.transportRunning ?? transportReady) {
      setTransportReady(true);
      return;
    }
    await invoke("transport_start", {
      relayUrl: relayUrl || null,
      forceRelay,
      bindAddr,
    });
    setTransportReady(true);
  };
  const signOut = () =>
    act(async () => {
      refreshVersion.current++;
      try {
        await api("logout");
      } finally {
        resetConnection();
        setMe(null);
        setDevices([]);
        setRequests([]);
        setPending([]);
        setTab("devices");
        await refresh();
      }
    }, "已退出登录");

  return (
    <div className="app">
      {native && (
        <header className="window-titlebar">
          <button
            className="window-menu-trigger"
            aria-label="应用菜单"
            popoverTarget="window-menu"
          >
            FarSail <span className="window-subtitle">遥舟</span>
          </button>
          <div id="window-menu" className="window-menu" popover="auto">
            <button
              onClick={() => {
                document.getElementById("window-menu")?.hidePopover();
                void act(() => invoke("app_exit"), "", false);
              }}
            >
              退出
            </button>
          </div>
          <div className="window-drag" data-tauri-drag-region></div>
          <div className="window-controls">
            <button
              aria-label="最小化"
              title="最小化"
              onClick={() =>
                void getCurrentWindow()
                  .minimize()
                  .catch((e) => setProblem(errorText(e)))
              }
            >
              <Subtract16Regular />
            </button>
            <button
              aria-label="最大化或还原"
              title="最大化 / 还原"
              onClick={() =>
                void getCurrentWindow()
                  .toggleMaximize()
                  .catch((e) => setProblem(errorText(e)))
              }
            >
              <Square16Regular />
            </button>
            <button
              className="window-close"
              aria-label="关闭窗口"
              title="关闭窗口"
              onClick={() =>
                void getCurrentWindow()
                  .close()
                  .catch((e) => setProblem(errorText(e)))
              }
            >
              <Dismiss16Regular />
            </button>
          </div>
        </header>
      )}
      <aside className="sidebar">
        <div className="brand">
          <img src="/icon.png" alt="" />
          <div>
            <strong>FarSail</strong>
            <span>遥舟 · 远程连接</span>
          </div>
        </div>
        <div className="side-label">工作空间</div>
        <nav aria-label="主导航">
          {(
            [
              ["overview", "总览", Home20Regular],
              ["devices", "我的设备", Desktop20Regular],
              ["requests", "远程连接", ArrowSwap20Regular],
              ["files", "文件传输", Folder20Regular],
              ["settings", "共享与设置", Settings20Regular],
              ["security", "账号安全", Shield20Regular],
              ...(me?.role === "admin"
                ? [["admin", "管理控制台", People20Regular]]
                : []),
            ] as [Tab, string, React.ComponentType][]
          ).map(([id, label, Icon]) => (
            <button
              key={id}
              className={tab === id ? "active" : ""}
              aria-label={label}
              aria-current={tab === id ? "page" : undefined}
              title={label}
              onClick={() => setTab(id)}
            >
              <span aria-hidden="true">
                <Icon />
              </span>
              {label}
            </button>
          ))}
        </nav>
        <div
          className="sidebar-foot"
          role="status"
          aria-label={
            remoteAccessEnabled(publicState)
              ? "远程连接已允许"
              : "远程连接已关闭"
          }
          title={
            remoteAccessEnabled(publicState)
              ? "远程连接已允许"
              : "远程连接已关闭"
          }
        >
          {me && (
            <div className="sidebar-account">
              <People20Regular aria-hidden="true" />
              <span title={me.email}>
                {me.email}
                <small>{me.role === "admin" ? "管理员" : "个人账号"}</small>
              </span>
            </div>
          )}
          <div className="sidebar-status">
            <span
              className={`status-dot ${remoteAccessEnabled(publicState) ? "is-sharing" : ""}`}
              aria-hidden="true"
            />
            <span>
              {remoteAccessEnabled(publicState)
                ? "远程连接已允许"
                : "远程连接已关闭"}
            </span>
            {remoteAccessEnabled(publicState) && (
              <small>{publicState.sharing ? "屏幕与文件" : "文件传输"}</small>
            )}
          </div>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <div>
            <div className="eyebrow">FARSAIL / {tab.toUpperCase()}</div>
            <h1>
              {
                (
                  {
                    overview: "总览",
                    devices: "我的设备",
                    requests: "远程连接",
                    files: "文件传输",
                    viewer: "远程画面",
                    security: "账号安全",
                    admin: "管理控制台",
                    settings: "共享与设置",
                  } as Record<Tab, string>
                )[tab]
              }
            </h1>
          </div>
          <div className="account-pill">
            {me ? (
              <>
                <span className="avatar">{me.email[0].toUpperCase()}</span>
                <span>
                  {me.email}
                  <small>{me.role === "admin" ? "管理员" : "个人账号"}</small>
                </span>
              </>
            ) : (
              <span>未登录</span>
            )}
          </div>
          <div
            id="page-actions"
            className="page-actions"
            aria-label="页面操作"
          />
        </header>
        {tab !== "settings" && (
          <UpdateNotice updates={updates} onOpen={updatePrompt.show} />
        )}
        <UpdateDialog
          updates={updates}
          open={updatePrompt.open}
          onDismiss={updatePrompt.dismiss}
        />
        <FailureDialog
          error={problem}
          message={readableError(problem)}
          onDismiss={() => {
            setProblem("");
            setFailureAction(undefined);
          }}
          action={failureAction}
        />
        {notice && noticePresent && (
          <div className="alert success" role="status">
            <span>{notice.message}</span>
            <button onClick={() => setNotice(null)} aria-label="关闭提示">
              <Dismiss16Regular aria-hidden="true" />
            </button>
          </div>
        )}
        <div
          className={`workspace-content page-${!me && tab !== "settings" ? "auth" : tab}`}
        >
          {!me && tab !== "settings" ? (
            <Auth
              mode={authMode}
              setMode={setAuthMode}
              busy={busy}
              act={act}
              onLogin={async (args) => {
                resetConnection();
                const current = await api<Me>("login", args);
                setMe(current);
                setTab("devices");
              }}
              onSettings={() => setTab("settings")}
            />
          ) : tab === "settings" ? (
            <section className="settings-grid">
              <div className="settings-column">
                <UpdateSettings updates={updates} onOpen={updatePrompt.show} />
                <div className="card">
                  <h2>服务器连接</h2>
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      void act(
                        () => invoke("set_server", { server }),
                        "服务地址已保存",
                      );
                    }}
                    className="inline-form"
                  >
                    <input
                      aria-label="服务地址"
                      placeholder="https://服务器地址"
                      value={server}
                      disabled={busy || !!me}
                      onChange={(e) => setServer(e.target.value)}
                    />
                    <button className="primary" disabled={busy || !!me}>
                      保存地址
                    </button>
                  </form>
                  <p className="hint">
                    {me
                      ? "退出登录后可修改地址。"
                      : "使用管理员提供的地址，公网连接需使用 HTTPS。"}
                  </p>
                  <details className="advanced-settings connection-settings">
                    <summary>高级连接设置</summary>
                    <p className="hint" role="status">
                      {!publicState.transportRunning
                        ? "连接尚未启动"
                        : publicState.transportConfiguration?.relay
                          ? publicState.transportConfiguration.forceRelay
                            ? "始终使用中继"
                            : "直连优先，中继备用"
                          : "仅直连 · 未配置中继"}
                    </p>
                    {publicState.transportConfiguration?.relay && (
                      <p className="setting-value">
                        {publicState.transportConfiguration.relay}
                      </p>
                    )}
                    <p className="muted">
                      默认使用服务器的 8443 端口，按管理员要求修改。
                    </p>
                    <form
                      className="form"
                      onSubmit={(e) => {
                        e.preventDefault();
                        void act(async () => {
                          resetConnection();
                          await invoke("transport_start", {
                            relayUrl: relayUrl || null,
                            forceRelay,
                            bindAddr,
                          });
                          setTransportReady(true);
                        }, "连接设置已应用");
                      }}
                    >
                      <label>
                        中继服务器地址
                        <input
                          placeholder="https://服务器IP:8443/"
                          value={relayUrl}
                          onChange={(e) => setRelayUrl(e.target.value)}
                        />
                      </label>
                      <label>
                        本机 UDP 监听地址
                        <input
                          value={bindAddr}
                          onChange={(e) => setBindAddr(e.target.value)}
                        />
                      </label>
                      <label className="checkbox-row">
                        <input
                          type="checkbox"
                          checked={forceRelay}
                          onChange={(e) => setForceRelay(e.target.checked)}
                        />
                        <span>始终通过中继连接</span>
                      </label>
                      <button
                        className="primary"
                        disabled={busy || !publicState.deviceId}
                      >
                        应用连接设置
                      </button>
                    </form>
                  </details>
                </div>
              </div>
              <div className="settings-column">
                <div className="card">
                  <h2>远程连接</h2>
                  <RemoteAccessControls
                    state={publicState}
                    busy={busy}
                    act={act}
                    prepareTransport={prepareTransport}
                  />
                  {canShareLocalScreen(publicState) && (
                    <details className="advanced-settings sharing-permissions">
                      <summary>Windows 运行权限</summary>
                      <p className="muted">
                        {publicState.administratorMode === true
                          ? "当前以管理员权限运行。"
                          : publicState.administratorMode === false
                            ? "当前使用普通权限。"
                            : "暂时无法读取运行权限。"}
                      </p>
                      <p className="hint">
                        {publicState.administratorMode === false
                          ? "重新启动 FarSail 并同意系统授权后，可控制管理员窗口。"
                          : "启动时会申请管理员权限。"}
                        系统授权与登录画面需在被控电脑处理。
                      </p>
                    </details>
                  )}
                  {!publicState.deviceId && (
                    <p className="hint">
                      {me
                        ? "添加本机后可允许远程连接。"
                        : "登录并添加本机后可允许远程连接。"}
                      <button
                        className="text-button"
                        onClick={() => setTab("overview")}
                      >
                        {me ? "去添加本机" : "去登录"}
                      </button>
                    </p>
                  )}
                </div>
              </div>
            </section>
          ) : tab === "overview" ? (
            <Overview
              me={me!}
              state={publicState}
              devices={devices}
              requests={requests}
              pending={pending}
              busy={busy}
              act={act}
              onRefresh={refresh}
              onNavigate={setTab}
            />
          ) : tab === "devices" ? (
            <Devices
              pending={pending}
              onRefresh={refresh}
              requests={requests}
              prepareTransport={prepareTransport}
              onConnectionReset={resetConnection}
              onView={openViewer}
              onFiles={openFiles}
              onNavigate={setTab}
              devices={devices}
              state={publicState}
              busy={busy}
              act={act}
            />
          ) : tab === "requests" ? (
            <Requests
              devices={devices}
              requests={requests}
              pending={pending}
              state={publicState}
              busy={busy}
              act={act}
              transportReady={transportReady}
              invitation={connectionInvite}
              setInvitation={setConnectionInvite}
              prepareTransport={prepareTransport}
              onRefresh={refresh}
              onNavigate={setTab}
              onView={openViewer}
              onFiles={openFiles}
            />
          ) : tab === "files" ? (
            <FilesPanel
              requests={requests}
              devices={devices}
              state={publicState}
              selectedId={fileSession}
              onSelect={setFileSession}
              busy={busy}
              act={act}
              prepareTransport={prepareTransport}
              onRefresh={refresh}
              onChooseDevice={() => setTab("devices")}
              onReviewRequests={() => setTab("requests")}
            />
          ) : tab === "security" ? (
            <Security
              me={me!}
              busy={busy}
              act={act}
              signOut={signOut}
              onConnectionReset={resetConnection}
            />
          ) : me?.role === "admin" && tab === "admin" ? (
            <Admin
              busy={busy}
              act={act}
              invitation={signupInvite}
              setInvitation={setSignupInvite}
            />
          ) : null}
        </div>
      </main>
    </div>
  );
}

type Action = (
  work: () => Promise<unknown>,
  message: string,
  reload?: boolean,
  failureAction?: FailureAction,
) => Promise<void>;
function Auth({
  mode,
  setMode,
  busy,
  act,
  onLogin,
  onSettings,
}: {
  mode: "login" | "register" | "verify" | "recover" | "reset";
  setMode: (m: "login" | "register" | "verify" | "recover" | "reset") => void;
  busy: boolean;
  act: Action;
  onLogin: (x: Record<string, unknown>) => Promise<void>;
  onSettings: () => void;
}) {
  const loginErrorDialog = useRef<HTMLDialogElement>(null);
  const loginTab = useRef<HTMLButtonElement>(null);
  const registerTab = useRef<HTMLButtonElement>(null);
  const flowHeading = useRef<HTMLHeadingElement>(null);
  const previousMode = useRef(mode);
  const pendingFocus = useRef<"heading" | "login" | "register" | null>(null);
  const isEntry = mode === "login" || mode === "register";
  useEffect(() => {
    const previous = previousMode.current;
    previousMode.current = mode;
    if (previous !== mode) {
      pendingFocus.current = !isEntry
        ? "heading"
        : previous !== "login" && previous !== "register"
          ? (mode as "login" | "register")
          : null;
    }
    if (busy || !pendingFocus.current) return;
    const target =
      pendingFocus.current === "heading"
        ? flowHeading.current
        : (pendingFocus.current === "login" ? loginTab : registerTab).current;
    target?.focus();
    pendingFocus.current = null;
  }, [mode, isEntry, busy]);
  const moveAuthTab = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (busy) return;
    let next: "login" | "register";
    if (event.key === "Home") next = "login";
    else if (event.key === "End") next = "register";
    else if (event.key === "ArrowLeft" || event.key === "ArrowRight")
      next = mode === "login" ? "register" : "login";
    else return;
    event.preventDefault();
    setMode(next);
    (next === "login" ? loginTab : registerTab).current?.focus();
  };
  const [loginError, setLoginError] = useState("");
  const [loginErrorTitle, setLoginErrorTitle] = useState("登录未成功");
  const [loginTechnicalError, setLoginTechnicalError] = useState("");
  const [verificationRequestedEmail, setVerificationRequestedEmail] =
    useState("");
  const [email, setEmail] = useState(""),
    [password, setPassword] = useState(""),
    [newPassword, setNewPassword] = useState(""),
    [token, setToken] = useState(""),
    [invite, setInvite] = useState("");
  const titles = {
    login: "登录 FarSail",
    register: "创建账号",
    verify: "验证邮箱",
    recover: "找回密码",
    reset: "设置新密码",
  };
  const resendVerification = async () => {
    await act(async () => {
      if (!email || !password)
        throw new Error(
          "重发所需的账号信息已失效，请在登录页填写邮箱和密码，再进入验证邮箱页重试。",
        );
      await api("resend", { email, password });
      setVerificationRequestedEmail(email.trim());
    }, "验证码重发请求已提交");
  };
  const submit = async () => {
    if (mode === "login")
      return act(async () => {
        try {
          await onLogin({ email, password });
        } catch (e) {
          const raw = errorText(e);
          const reasons: Record<string, [string, string]> = {
            account_not_found: [
              "账号不存在",
              "这个邮箱尚未注册。请检查邮箱是否输入正确，或创建账号。",
            ],
            invalid_password: [
              "密码错误",
              "密码不正确，请重新输入。忘记密码时可返回登录页选择‘找回密码’。",
            ],
            email_not_verified: ["邮箱尚未验证", "请先完成邮箱验证，再登录。"],
            account_disabled: [
              "账号已停用",
              "这个账号已被停用，请联系管理员。",
            ],
          };
          const code = /^HTTP 401: (\w+)$/.exec(raw)?.[1];
          const reason = code ? reasons[code] : undefined;
          setLoginErrorTitle(reason?.[0] ?? "登录未成功");
          setLoginError(
            reason?.[1] ??
              (/HTTP 401\b/.test(raw)
                ? "邮箱或密码不正确。请核对后重新输入。旧版服务器未提供具体登录失败原因。"
                : /HTTP 429\b/.test(raw)
                  ? "登录尝试过于频繁，请稍后再试。"
                  : "暂时未能登录。请稍后再试，或查看技术详情。"),
          );
          setLoginTechnicalError(raw);
          loginErrorDialog.current?.showModal();
          return HANDLED_FAILURE;
        }
      }, "欢迎回来");
    if (mode === "register")
      return act(async () => {
        await api("register", {
          email,
          password,
          invite_code: invite || undefined,
        });
        setVerificationRequestedEmail(email.trim());
        setToken("");
        setMode("verify");
      }, "账号已创建");
    if (mode === "verify")
      return act(async () => {
        await api("verify", {
          ...(email.trim() ? { email } : {}),
          token: token.trim(),
        });
        setToken("");
        setPassword("");
        setVerificationRequestedEmail("");
        setMode("login");
      }, "邮箱验证成功");
    if (mode === "recover")
      return act(async () => {
        await api("recover_request", { email });
        setToken("");
        setMode("reset");
      }, "密码重置请求已提交");
    return act(async () => {
      await api("recover_complete", {
        token: token.trim(),
        password: newPassword,
      });
      setToken("");
      setNewPassword("");
      setPassword("");
      setMode("login");
    }, "密码已重置");
  };
  return (
    <div className="auth-wrap">
      <div className="auth-identity">
        <img src="/icon.png" alt="FarSail 图标" />
        <h1>FarSail 遥舟</h1>
        <p>连接你的电脑，继续你的工作。</p>
        <button className="text-button" onClick={onSettings}>
          <Settings20Regular />
          连接设置
        </button>
      </div>
      <dialog
        ref={loginErrorDialog}
        className="login-error-dialog"
        aria-labelledby="login-error-title"
        aria-describedby="login-error-description"
      >
        <h2 id="login-error-title">{loginErrorTitle}</h2>
        <p id="login-error-description">{loginError}</p>
        <div className="row-actions">
          <button
            className="secondary"
            onClick={() => {
              loginErrorDialog.current?.close();
              setMode("register");
            }}
          >
            创建账号
          </button>
          <button
            className="secondary"
            onClick={() => {
              loginErrorDialog.current?.close();
              setMode("verify");
            }}
          >
            验证邮箱
          </button>
          <button
            autoFocus
            className="primary"
            onClick={() => loginErrorDialog.current?.close()}
          >
            返回登录
          </button>
        </div>
        <details className="failure-details">
          <summary>技术详情</summary>
          <code>{loginTechnicalError}</code>
        </details>
      </dialog>
      <section className="card auth-card">
        {isEntry ? (
          <div
            className="auth-tabs"
            role="tablist"
            aria-label="账号入口"
            onKeyDown={moveAuthTab}
          >
            <button
              ref={loginTab}
              id="auth-login-tab"
              role="tab"
              aria-controls="auth-panel"
              tabIndex={mode === "login" ? 0 : -1}
              aria-selected={mode === "login"}
              className={mode === "login" ? "active" : ""}
              disabled={busy}
              onClick={() => setMode("login")}
            >
              登录
            </button>
            <button
              ref={registerTab}
              id="auth-register-tab"
              role="tab"
              aria-controls="auth-panel"
              tabIndex={mode === "register" ? 0 : -1}
              aria-selected={mode === "register"}
              className={mode === "register" ? "active" : ""}
              disabled={busy}
              onClick={() => setMode("register")}
            >
              注册
            </button>
          </div>
        ) : (
          <button
            className="text-button auth-back"
            disabled={busy}
            onClick={() => setMode("login")}
          >
            <ChevronLeft16Regular aria-hidden="true" />
            返回登录
          </button>
        )}
        <div
          id="auth-panel"
          role={isEntry ? "tabpanel" : undefined}
          aria-labelledby={isEntry ? `auth-${mode}-tab` : "auth-flow-title"}
        >
          <h2
            ref={flowHeading}
            id="auth-flow-title"
            tabIndex={isEntry ? undefined : -1}
            className={isEntry ? "sr-only" : "auth-flow-title"}
          >
            {titles[mode]}
          </h2>
          {mode === "verify" && (
            <p className="muted">
              {verificationRequestedEmail
                ? `验证码发送请求已提交，请查看 ${verificationRequestedEmail} 的收件箱。`
                : "填写注册邮箱和收到的验证码。"}
            </p>
          )}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submit();
            }}
            className="form"
          >
            {["login", "register", "recover", "verify"].includes(mode) && (
              <label>
                邮箱地址
                <input
                  type="email"
                  required={mode !== "verify" || /^\d{6}$/.test(token.trim())}
                  disabled={busy}
                  value={email}
                  onChange={(e) => {
                    const next = e.target.value;
                    if (
                      next.trim().toLowerCase() !== email.trim().toLowerCase()
                    ) {
                      setVerificationRequestedEmail("");
                      if (mode === "verify") setPassword("");
                    }
                    setEmail(next);
                  }}
                  placeholder="you@example.com"
                  autoComplete="email"
                />
              </label>
            )}
            {["login", "register"].includes(mode) && (
              <label>
                密码
                <input
                  type="password"
                  aria-label="密码"
                  aria-describedby={
                    mode === "register" ? "register-password-hint" : undefined
                  }
                  required
                  minLength={mode === "register" ? 12 : 1}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete={
                    mode === "register" ? "new-password" : "current-password"
                  }
                />
                {mode === "register" && (
                  <small id="register-password-hint" className="muted">
                    至少 12 个字符
                  </small>
                )}
              </label>
            )}
            {["verify", "reset"].includes(mode) && (
              <label>
                {mode === "verify" ? "邮箱验证码" : "邮件中的重置码"}
                <input
                  required
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  autoComplete={mode === "verify" ? "one-time-code" : "off"}
                  inputMode={mode === "verify" ? "numeric" : "text"}
                  placeholder={
                    mode === "verify" ? "请输入 6 位数字验证码" : undefined
                  }
                />
              </label>
            )}
            {mode === "reset" && (
              <label>
                新密码
                <input
                  type="password"
                  required
                  minLength={12}
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  autoComplete="new-password"
                  aria-label="新密码"
                  aria-describedby="reset-password-hint"
                />
                <small id="reset-password-hint" className="muted">
                  至少 12 个字符
                </small>
              </label>
            )}
            {mode === "register" && (
              <label>
                注册邀请码（若服务要求）
                <input
                  value={invite}
                  onChange={(e) => setInvite(e.target.value)}
                  autoComplete="off"
                />
              </label>
            )}
            <button className="primary wide" disabled={busy}>
              {busy
                ? "请稍候…"
                : mode === "register"
                  ? "注册"
                  : mode === "verify"
                    ? "完成验证"
                    : mode === "login"
                      ? "登录"
                      : mode === "recover"
                        ? "发送邮件"
                        : "重置密码"}
            </button>
          </form>
          <div className="auth-links">
            {mode === "verify" && (
              <button disabled={busy} onClick={() => void resendVerification()}>
                重新发送验证码
              </button>
            )}
            {(
              {
                login: ["recover", "verify"],
                register: [],
                verify: [],
                recover: ["reset"],
                reset: ["recover"],
              }[mode] as (typeof mode)[]
            ).map((x) => (
              <button key={x} disabled={busy} onClick={() => setMode(x)}>
                {x === "reset"
                  ? "已有重置码"
                  : x === "recover" && mode === "reset"
                    ? "重新发送邮件"
                    : titles[x]}
              </button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}

function Overview({
  state,
  devices,
  requests,
  pending,
  busy,
  act,
  onRefresh,
  onNavigate,
}: {
  me: Me;
  state: PublicState;
  devices: Device[];
  requests: Remote[];
  pending: Pending[];
  busy: boolean;
  act: Action;
  onRefresh: (foreground?: boolean) => Promise<void>;
  onNavigate: (t: Tab) => void;
}) {
  const local = devices.find((x) => x.id === state.deviceId);
  const [name, setName] = useState(state.computerName ?? "");
  const nameEdited = useRef(false);
  useEffect(() => {
    if (!nameEdited.current && state.computerName) setName(state.computerName);
  }, [state.computerName]);
  return (
    <div className="stack">
      <PageActions>
        <button className="text-button" onClick={() => void onRefresh()}>
          <ArrowSync20Regular />
          刷新状态
        </button>
      </PageActions>
      {!state.deviceId ? (
        <div className="card attention">
          <div>
            <h2>添加这台电脑</h2>
            {!canShareLocalScreen(state) && (
              <p>Ubuntu 暂不支持共享本机屏幕。</p>
            )}
          </div>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                () => api("bind", { name: name.trim() }),
                "这台电脑已添加",
              );
            }}
          >
            <input
              aria-label="本机设备名称"
              value={name}
              onChange={(e) => {
                nameEdited.current = true;
                setName(e.target.value);
              }}
              placeholder="输入这台电脑的名称"
              maxLength={80}
              required
            />
            <button className="primary" disabled={busy || !name.trim()}>
              添加
            </button>
          </form>
          {!state.computerName && (
            <p className="muted">无法读取计算机名称，请手动填写。</p>
          )}
        </div>
      ) : (
        <div className="card local-device">
          <div>
            <Desktop20Regular
              className="local-device-icon"
              aria-hidden="true"
            />
            <div className="eyebrow">THIS DEVICE</div>
            <h2>{local?.name ?? "本机设备"}</h2>
            <p className="muted">
              设备 ID {short(state.deviceId)} ·{" "}
              {local?.online ? "在线" : "正在更新在线状态"}
            </p>
          </div>
          <div className="stack">
            <span className={remoteAccessEnabled(state) ? "tag ready" : "tag"}>
              {remoteAccessEnabled(state) ? "远程连接已允许" : "远程连接已关闭"}
            </span>
            <button
              className="secondary"
              onClick={() => onNavigate("settings")}
            >
              管理远程连接
            </button>
            {!canShareLocalScreen(state) && (
              <small className="muted">暂不支持共享本机屏幕。</small>
            )}
          </div>
        </div>
      )}
      <div className="overview-shortcuts">
        <span className="muted">
          {devices.length} 台设备 · {devices.filter((d) => d.online).length}{" "}
          台在线{pending.length ? ` · ${pending.length} 条待批准` : ""}
        </span>
        <button className="primary" onClick={() => onNavigate("devices")}>
          查看设备
          <ChevronRight16Regular />
        </button>
      </div>
      <div className="card overview-history">
        <div className="section-heading">
          <div>
            <div className="eyebrow">ACTIVITY</div>
            <h2>最近连接</h2>
          </div>
          <button
            className="text-button"
            onClick={() => onNavigate("requests")}
          >
            全部连接 →
          </button>
        </div>
        <div
          className="overview-records"
          tabIndex={0}
          aria-label="最近远程连接列表"
        >
          {requests.length ? (
            requests.slice(0, 4).map((x) => (
              <div className="list-row" key={x.id}>
                <div>
                  <strong>{permissionLabel(x.permission)}</strong>
                  <small>
                    {deviceLabel(x.source_device_id, devices, state.deviceId)} →{" "}
                    {deviceLabel(x.target_device_id, devices, state.deviceId)}
                  </small>
                </div>
                <span
                  className={"tag " + (x.state === "pending" ? "warm" : "")}
                >
                  {requestLabel(x.state)}
                </span>
              </div>
            ))
          ) : (
            <Empty text="暂无连接记录" />
          )}
        </div>
      </div>
    </div>
  );
}
function Empty({
  text,
  children,
}: {
  text: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="empty">
      <Desktop20Regular aria-hidden="true" />
      <p>{text}</p>
      {children}
    </div>
  );
}
function IncomingApprovals({
  pending,
  state,
  busy,
  act,
  prepareTransport,
  onFiles,
}: {
  pending: Pending[];
  state: PublicState;
  busy: boolean;
  act: Action;
  prepareTransport: () => Promise<void>;
  onFiles: (id: string) => void;
}) {
  if (!pending.length) return null;
  return (
    <>
      <h3 className="heading-with-count">
        等待批准的连接 <span className="count">{pending.length}</span>
      </h3>
      {pending.map((p) => (
        <div className="approval-card" key={p.id}>
          <strong>
            {p.source_device_name} 请求
            {p.permission === "files"
              ? "传输文件"
              : p.permission === "control"
                ? "控制电脑"
                : "查看屏幕"}
          </strong>
          <p className="muted">{p.requester_email}</p>
          <p className="muted">
            {p.permission === "files"
              ? "仅传输文件，每个文件需选择保存位置。"
              : p.permission === "control"
                ? "查看屏幕、操作鼠标键盘和传输文件。"
                : "仅查看屏幕。"}
          </p>
          <div className="row-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() =>
                void act(
                  () => api("decide", { id: p.id, approve: false }),
                  "已拒绝连接",
                )
              }
            >
              拒绝
            </button>
            <button
              className="primary"
              disabled={
                busy ||
                !(p.permission === "files" ? state.filesEnabled : state.sharing)
              }
              onClick={() =>
                void act(async () => {
                  if (p.permission === "files") await prepareTransport();
                  await api("decide", { id: p.id, approve: true });
                  if (p.permission === "files") onFiles(p.id);
                }, "已允许连接")
              }
            >
              {p.permission === "files"
                ? "允许文件传输"
                : p.permission === "control"
                  ? "允许控制"
                  : "允许查看"}
            </button>
          </div>
          {p.permission === "files" && !state.filesEnabled && (
            <p className="hint">请先开启“允许远程连接”。</p>
          )}
        </div>
      ))}
    </>
  );
}
function Devices({
  requests,
  pending,
  prepareTransport,
  onConnectionReset,
  onView,
  onFiles,
  onRefresh,
  onNavigate,
  devices,
  state,
  busy,
  act,
}: {
  requests: Remote[];
  pending: Pending[];
  prepareTransport: () => Promise<void>;
  onConnectionReset: () => void;
  onView: (id: string) => Promise<ViewerNoticeLease>;
  onFiles: (id: string) => void;
  onRefresh: (foreground?: boolean) => Promise<void>;
  onNavigate: (tab: Tab) => void;
  devices: Device[];
  state: PublicState;
  busy: boolean;
  act: Action;
}) {
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("all");
  const searchInput = useRef<HTMLInputElement>(null);
  const query = search.trim().toLowerCase();
  const filtered = !!query || filter !== "all";
  const [selected, setSelected] = useState<string | null>(null);
  const [waiting, setWaiting] = useState<{ id: string; target: string } | null>(
    null,
  );
  const [connectionNote, setConnectionNotice] =
    useState<OperationNotice | null>(null);
  const setConnectionNote = (message: string, viewer?: ViewerNoticeLease) =>
    setConnectionNotice(message ? { message, viewer } : null);
  const openedWindow = connectionNote?.viewer ?? null;
  const openedWindowPresent = useViewerNoticePresent(openedWindow);
  useEffect(() => {
    if (!openedWindowPresent)
      setConnectionNotice((current) =>
        current?.viewer === openedWindow ? null : current,
      );
  }, [openedWindowPresent, openedWindow]);
  const [connecting, setConnecting] = useState(false);
  const inFlight = useRef(false);
  const [editing, setEditing] = useState(false);
  const [newName, setNewName] = useState("");
  const [confirmUnbind, setConfirmUnbind] = useState<{
    id: string;
    name: string;
  } | null>(null);
  const shown = devices
    .filter(
      (d) =>
        (d.name.toLowerCase().includes(query) ||
          d.id.toLowerCase().includes(query)) &&
        (filter !== "online" || d.online),
    )
    .sort(
      (a, b) =>
        Number(b.id === state.deviceId) - Number(a.id === state.deviceId) ||
        Number(b.online) - Number(a.online),
    );
  const device = shown.find((d) => d.id === selected) ?? shown[0];
  const isLocal = device?.id === state.deviceId;
  useEffect(() => {
    setEditing(false);
    setConfirmUnbind(null);
  }, [device?.id]);
  useEffect(() => {
    if (!waiting) return;
    const timer = window.setInterval(
      () => void onRefresh(false).catch(() => {}),
      2000,
    );
    return () => window.clearInterval(timer);
  }, [waiting, onRefresh]);
  useEffect(() => {
    const request = requests.find((r) => r.id === waiting?.id);
    if (!request || inFlight.current) return;
    if (request.state === "approved") {
      inFlight.current = true;
      setConnecting(true);
      setWaiting(null);
      setConnectionNote("正在连接远程电脑…");
      void act(
        async () => {
          try {
            await prepareTransport();
            await invoke("transport_connect", {
              id: request.id,
              permission: request.permission,
            });
            if (request.permission === "files") {
              onFiles(request.id);
              setConnectionNote("");
              return;
            }
            const opened = await onView(request.id);
            setConnectionNote(
              opened.isPresent() ? "已在独立窗口打开，可继续管理设备。" : "",
              opened,
            );
            return opened;
          } catch (e) {
            setConnectionNote("");
            void api("revoke_remote", { id: request.id }).catch(() => {});
            throw e;
          } finally {
            inFlight.current = false;
            setConnecting(false);
          }
        },
        request.permission === "files"
          ? "文件连接已建立"
          : "已打开独立远程窗口",
        false,
        {
          retryLabel: "重新连接",
          retry: () =>
            connect(request.permission, request.target_device_id, true),
        },
      );
    } else if (request.state !== "pending") {
      setWaiting(null);
      setConnectionNote(
        request.state === "denied"
          ? "对方已拒绝连接。"
          : "请求已结束或过期，请重新连接。",
      );
    }
  }, [requests, waiting, act, prepareTransport, onView]);
  const connect = (
    permission: "control" | "view" | "files",
    target = device?.id,
    fresh = false,
  ) => {
    if (!device || inFlight.current || (!fresh && (busy || waiting))) return;
    if (!target) return;
    inFlight.current = true;
    setConnecting(true);
    setConnectionNote("正在发送连接请求…");
    void act(
      async () => {
        try {
          await prepareTransport();
          const existing = fresh
            ? undefined
            : requests.find(
                (r) =>
                  r.source_device_id === state.deviceId &&
                  r.target_device_id === target &&
                  r.permission === permission &&
                  r.state === "pending",
              );
          const result =
            existing ??
            (await api<{ id: string }>("request", {
              source_device_id: state.deviceId,
              target_device_id: target,
              permission,
            }));
          setWaiting({ id: result.id, target });
          setConnectionNote(
            permission === "files"
              ? "等待对方批准文件连接。"
              : "等待对方批准连接。",
          );
        } catch (e) {
          setConnectionNote("");
          throw e;
        } finally {
          inFlight.current = false;
          setConnecting(false);
        }
      },
      "已发送连接请求",
      true,
      {
        retryLabel: "重新连接",
        retry: () => connect(permission, target, true),
      },
    );
  };
  const blocked = !state.deviceId
    ? "先在“总览”添加这台电脑，才能发起连接。"
    : !device?.enabled
      ? "这台设备已停用。"
      : !device?.online
        ? "设备离线，请在对方电脑上打开 FarSail 并登录。"
        : !device?.can_host
          ? "对方尚未允许屏幕连接，请检查对方的远程连接设置。"
          : "";
  return (
    <section
      className={`device-workspace${device ? "" : " device-workspace-empty"}`}
    >
      <div className="device-list-pane">
        <div className="section-heading">
          <h2 className="heading-with-count">
            设备列表{" "}
            <span
              className="count"
              aria-label={`显示 ${shown.length} 台，共 ${devices.length} 台`}
            >
              {filtered
                ? `${shown.length} / ${devices.length}`
                : devices.length}
            </span>
          </h2>
          <button
            className="text-button"
            disabled={busy}
            onClick={() => void act(onRefresh, "设备状态已刷新", false)}
          >
            刷新
          </button>
        </div>
        <div className="filters">
          <input
            ref={searchInput}
            aria-label="搜索设备"
            placeholder="搜索电脑名称或设备 ID"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <select
            aria-label="筛选设备"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          >
            <option value="all">全部</option>
            <option value="online">在线</option>
          </select>
        </div>
        <div className="device-list" aria-label="设备列表">
          {shown.map((d) => (
            <button
              key={d.id}
              className={`device-list-item ${device?.id === d.id ? "selected" : ""}`}
              aria-pressed={device?.id === d.id}
              onClick={() => setSelected(d.id)}
            >
              <span
                className={`device-indicator ${d.online && d.enabled ? "online" : ""}`}
              >
                <Desktop20Regular />
              </span>
              <span className="device-list-name">
                <strong>{d.name}</strong>
                <small>
                  {d.id === state.deviceId ? "本机 · " : ""}
                  {!d.enabled ? "已停用" : d.online ? "在线" : "离线"} ·{" "}
                  {short(d.id)}
                </small>
              </span>
              <ChevronRight16Regular aria-hidden="true" />
            </button>
          ))}
          {!shown.length && (
            <Empty text={devices.length ? "没有匹配的设备" : "暂无设备"}>
              {filtered && devices.length > 0 && (
                <button
                  className="secondary"
                  onClick={() => {
                    setSearch("");
                    setFilter("all");
                    searchInput.current?.focus();
                  }}
                >
                  清除筛选
                </button>
              )}
            </Empty>
          )}
        </div>
        {!state.deviceId && (
          <button
            className="primary wide"
            onClick={() => onNavigate("overview")}
          >
            添加这台电脑
          </button>
        )}
      </div>
      <div className="device-detail-pane" key={device?.id}>
        {device ? (
          <>
            <div className="section-heading">
              <div>
                <div className="tag-row">
                  <span
                    className={`tag ${device.online && device.enabled ? "ready" : ""}`}
                  >
                    {!device.enabled
                      ? "已停用"
                      : device.online
                        ? "在线"
                        : "离线"}
                  </span>
                  {isLocal && <span className="tag">本机</span>}
                </div>
                <h2 className="device-detail-title">{device.name}</h2>
                <p className="muted">
                  {device.platform} · {short(device.id)}
                </p>
              </div>
            </div>
            {isLocal ? (
              <>
                <div className="sharing-panel remote-access-panel">
                  <RemoteAccessControls
                    state={state}
                    busy={busy || !device.enabled}
                    act={act}
                    prepareTransport={prepareTransport}
                  />
                </div>
                <IncomingApprovals
                  pending={pending}
                  state={state}
                  busy={busy}
                  act={act}
                  prepareTransport={prepareTransport}
                  onFiles={onFiles}
                />
                <button
                  className="text-button"
                  onClick={() => onNavigate("settings")}
                >
                  连接设置 →
                </button>
              </>
            ) : (
              <>
                <div className="remote-action-grid">
                  <button
                    className="primary remote-primary"
                    aria-label="远程控制"
                    disabled={
                      busy ||
                      connecting ||
                      !!waiting ||
                      !state.deviceId ||
                      !device.enabled ||
                      !device.online ||
                      !device.can_host
                    }
                    onClick={() => connect("control")}
                  >
                    <Desktop20Regular aria-hidden="true" />
                    远程控制
                  </button>
                  <button
                    className="secondary"
                    disabled={
                      busy ||
                      connecting ||
                      !!waiting ||
                      !state.deviceId ||
                      !device.enabled ||
                      !device.online ||
                      !device.can_host
                    }
                    onClick={() => connect("view")}
                  >
                    <Eye20Regular aria-hidden="true" />
                    仅查看屏幕
                  </button>
                </div>
                {!device.can_host && (
                  <>
                    <button
                      className="secondary file-device-action"
                      disabled={
                        busy ||
                        connecting ||
                        !!waiting ||
                        !state.deviceId ||
                        !device.enabled ||
                        !device.online ||
                        !device.can_files
                      }
                      onClick={() => connect("files")}
                    >
                      <Folder20Regular aria-hidden="true" />
                      文件传输
                    </button>
                    {!device.can_files && (
                      <p className="muted">对方尚未开放文件传输。</p>
                    )}
                  </>
                )}
                {blocked && <p className="muted">{blocked}</p>}
                {(waiting ||
                  connecting ||
                  (connectionNote && openedWindowPresent)) && (
                  <div className="notice-strip" role="status">
                    {connectionNote?.message}
                    {waiting && (
                      <button
                        className="danger-text"
                        disabled={busy}
                        onClick={() =>
                          void act(async () => {
                            inFlight.current = true;
                            try {
                              await api("revoke_remote", { id: waiting.id });
                              setWaiting(null);
                              setConnectionNote("已取消连接。");
                            } finally {
                              inFlight.current = false;
                            }
                          }, "已取消连接")
                        }
                      >
                        取消连接
                      </button>
                    )}
                  </div>
                )}
                {waiting && waiting.target !== device.id && (
                  <p className="hint">正在等待另一台设备的批准。</p>
                )}
              </>
            )}
            <details className="advanced-settings device-management">
              <summary>设备信息与管理</summary>
              <div className="device-facts">
                <span>设备 ID</span>
                <code>{device.id}</code>
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() =>
                    void act(() => copyText(device.id), "设备 ID 已复制", false)
                  }
                >
                  复制设备 ID
                </button>
                <span>最后在线</span>
                <span>
                  {device.last_seen_at
                    ? new Date(device.last_seen_at).toLocaleString()
                    : "暂无记录"}
                </span>
              </div>
              {editing ? (
                <form
                  className="inline-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void act(async () => {
                      await api("rename_device", {
                        id: device.id,
                        name: newName.trim(),
                      });
                      setEditing(false);
                    }, "设备名称已更新");
                  }}
                >
                  <input
                    aria-label="新的设备名称"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    maxLength={80}
                    required
                  />
                  <button
                    className="primary"
                    disabled={busy || !newName.trim()}
                  >
                    保存
                  </button>
                  <button
                    className="secondary"
                    type="button"
                    disabled={busy}
                    onClick={() => setEditing(false)}
                  >
                    取消
                  </button>
                </form>
              ) : (
                <button
                  className="text-button"
                  onClick={() => {
                    setNewName(device.name);
                    setEditing(true);
                  }}
                >
                  重命名
                </button>
              )}
              {confirmUnbind ? (
                <FailureDialog
                  confirmation
                  error={`确定从账号移除“${confirmUnbind.name}”？`}
                  onDismiss={() => setConfirmUnbind(null)}
                  action={{
                    title: "从账号移除设备？",
                    retryLabel: "确认移除设备",
                    retry: () =>
                      void act(async () => {
                        onConnectionReset();
                        await api("unbind_device", { id: confirmUnbind.id });
                      }, "设备已从账号移除"),
                  }}
                />
              ) : (
                <button
                  className="danger-text"
                  onClick={() =>
                    setConfirmUnbind({ id: device.id, name: device.name })
                  }
                >
                  从账号移除设备
                </button>
              )}
            </details>
          </>
        ) : null}
      </div>
    </section>
  );
}
function Requests({
  devices,
  requests,
  pending,
  state,
  busy,
  act,
  transportReady,
  prepareTransport,
  onRefresh,
  invitation,
  setInvitation,
  onNavigate,
  onView,
  onFiles,
}: {
  devices: Device[];
  requests: Remote[];
  pending: Pending[];
  state: PublicState;
  busy: boolean;
  act: Action;
  transportReady: boolean;
  prepareTransport: () => Promise<void>;
  onRefresh: (foreground?: boolean) => Promise<void>;
  invitation: ConnectionInvitation | null;
  setInvitation: React.Dispatch<
    React.SetStateAction<ConnectionInvitation | null>
  >;
  onNavigate: (tab: Tab) => void;
  onView: (id: string) => Promise<ViewerNoticeLease>;
  onFiles: (id: string) => void;
}) {
  const [paths, setPaths] = useState<
    Record<
      string,
      {
        state: string;
        rtt_ms: number | null;
        discovery?: string;
        verification_code?: string | null;
        error?: string | null;
      }
    >
  >({});
  useEffect(() => {
    if (!transportReady || !native) return;
    const poll = () => {
      for (const r of requests.filter((x) => x.state === "approved")) {
        void invoke<{
          state: string;
          rtt_ms: number | null;
          discovery?: string;
          verification_code?: string | null;
          error?: string | null;
        }>("remote_status", { id: r.id })
          .catch(() =>
            invoke<{
              state: string;
              rtt_ms: number | null;
              discovery?: string;
            }>("transport_status", { id: r.id }),
          )
          .then((v) => setPaths((old) => ({ ...old, [r.id]: v })))
          .catch(() => {});
      }
    };
    poll();
    const timer = window.setInterval(poll, 5000);
    return () => window.clearInterval(timer);
  }, [requests, transportReady]);
  const [target, setTarget] = useState(""),
    [targetChoice, setTargetChoice] = useState("custom"),
    [inviteTarget, setInviteTarget] = useState(invitation?.target ?? ""),
    [permission, setPermission] = useState<"view" | "control" | "files">(
      "view",
    ),
    [code, setCode] = useState("");
  const [section, setSection] = useState(
    pending.length ? "incoming" : "active",
  );
  const [invitePermission, setInvitePermission] = useState<
    "view" | "control" | "files"
  >(invitation?.permission ?? "view");
  const invite = invitation?.code ?? "";
  const inviteId = invitation?.id ?? "";
  const generatedInvite = invitation;
  const inviteVisible = invitation?.visible ?? true;
  const pendingCount = requests.filter((r) => r.state === "pending").length;
  useEffect(() => {
    if (!pendingCount) return;
    const timer = window.setInterval(
      () => void onRefresh(false).catch(() => {}),
      2000,
    );
    return () => window.clearInterval(timer);
  }, [pendingCount, onRefresh]);
  const activeRequests = requests.filter((r) =>
    ["pending", "approved"].includes(r.state),
  );
  const shownRequests =
    section === "history"
      ? requests.filter((r) => !["pending", "approved"].includes(r.state))
      : activeRequests;
  const targets = devices.filter(
    (x) =>
      x.id !== state.deviceId &&
      x.enabled &&
      x.online &&
      (permission === "files" ? x.can_files : x.can_host),
  );
  const targetUnavailable =
    targetChoice !== "custom" && !targets.some((d) => d.id === targetChoice);
  const inviteTargets = devices.filter(
    (d) =>
      d.online &&
      d.enabled &&
      (invitePermission === "files" ? d.can_files : d.can_host),
  );
  const inviteTargetAvailable = inviteTargets.some(
    (d) => d.id === inviteTarget,
  );
  return (
    <div className="connections-workspace">
      <PageActions>
        <button
          className="text-button"
          disabled={busy}
          onClick={() => void act(onRefresh, "连接状态已刷新", false)}
        >
          刷新连接状态
        </button>
        <button className="secondary" onClick={() => onNavigate("devices")}>
          从我的设备连接
        </button>
      </PageActions>
      <div className="section-switcher" role="group" aria-label="连接分类">
        {[
          ["active", `当前连接 (${activeRequests.length})`],
          ["incoming", `待我批准 (${pending.length})`],
          ["history", "连接记录"],
          ["new", "连接他人电脑"],
          ["invite", "邀请他人连接"],
        ].map(([id, label]) => (
          <button
            key={id}
            className={section === id ? "active" : ""}
            aria-pressed={section === id}
            onClick={() => setSection(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="connection-content">
        <div className="card connection-form-pane" hidden={section !== "new"}>
          <div className="eyebrow">REQUEST ACCESS</div>
          <h2 className="sr-only">连接他人电脑</h2>
          <p className="muted">
            请对方允许远程连接，再提供设备 ID 和一次性邀请码。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              if (targetUnavailable) return;
              void act(async () => {
                await prepareTransport();
                await api("request", {
                  source_device_id: state.deviceId,
                  target_device_id: target.trim(),
                  permission,
                  invitation_code: code.trim() || undefined,
                });
                setSection("active");
              }, "连接请求已发送");
            }}
          >
            <label>
              目标设备
              <select
                aria-label="目标设备"
                value={targetChoice}
                aria-invalid={targetUnavailable || undefined}
                aria-describedby={
                  targetUnavailable ? "request-target-unavailable" : undefined
                }
                onChange={(e) => {
                  setTargetChoice(e.target.value);
                  setTarget(e.target.value === "custom" ? "" : e.target.value);
                }}
              >
                <option value="custom">输入对方提供的设备 ID</option>
                {targetUnavailable && (
                  <option value={targetChoice} disabled>
                    {devices.find((d) => d.id === targetChoice)?.name ??
                      "已选设备"}
                    （当前不可用）
                  </option>
                )}
                {targets.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </label>
            <input
              aria-label="目标设备 ID"
              placeholder="粘贴对方的完整设备 ID"
              value={target}
              onChange={(e) => {
                setTargetChoice("custom");
                setTarget(e.target.value);
              }}
              required
            />
            {targetUnavailable && (
              <p
                id="request-target-unavailable"
                className="hint field-notice"
                role="status"
              >
                所选设备暂不支持此连接。请检查设备状态或重新选择。
              </p>
            )}
            <label>
              连接后可以做什么
              <select
                aria-label="连接后可以做什么"
                value={permission}
                onChange={(e) =>
                  setPermission(e.target.value as typeof permission)
                }
              >
                <option value="view">仅查看屏幕</option>
                <option value="control">远程控制与文件传输</option>
                <option value="files">仅传输文件</option>
              </select>
            </label>
            <label>
              邀请码（跨账号时填写）
              <input
                value={code}
                onChange={(e) => setCode(e.target.value)}
                autoComplete="off"
              />
            </label>
            <button
              className="primary"
              disabled={busy || !state.deviceId || targetUnavailable}
            >
              发送连接请求
            </button>
          </form>
          {!state.deviceId && (
            <p className="hint">
              请先添加这台电脑。
              <button
                className="text-button"
                onClick={() => onNavigate("overview")}
              >
                去添加本机
              </button>
            </p>
          )}
        </div>
        <div
          className="card connection-form-pane"
          hidden={section !== "invite"}
        >
          <div className="eyebrow">SHARE ACCESS</div>
          <h2 className="sr-only">邀请他人连接我的电脑</h2>
          <p className="muted">连接需在目标电脑批准。</p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              if (!inviteTargetAvailable) return;
              void act(
                async () => {
                  const v = await api<{ code: string; id: string }>("invite", {
                    target_device_id: inviteTarget,
                    permission: invitePermission,
                  });
                  setInvitation({
                    id: v.id,
                    code: v.code,
                    visible: true,
                    target: inviteTarget,
                    permission: invitePermission,
                  });
                },
                "邀请码已生成",
                false,
              );
            }}
          >
            <label>
              允许对方连接哪台电脑
              <select
                aria-label="允许对方连接哪台电脑"
                value={inviteTarget}
                aria-invalid={
                  (!!inviteTarget && !inviteTargetAvailable) || undefined
                }
                aria-describedby={
                  inviteTarget && !inviteTargetAvailable
                    ? "invite-target-unavailable"
                    : undefined
                }
                onChange={(e) => setInviteTarget(e.target.value)}
                required
              >
                <option value="">选择设备</option>
                {inviteTarget && !inviteTargetAvailable && (
                  <option value={inviteTarget} disabled>
                    {devices.find((d) => d.id === inviteTarget)?.name ??
                      "已选设备"}
                    （当前不可用）
                  </option>
                )}
                {inviteTargets.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              允许对方做什么
              <select
                aria-label="允许对方做什么"
                value={invitePermission}
                onChange={(e) =>
                  setInvitePermission(
                    e.target.value as "view" | "control" | "files",
                  )
                }
              >
                <option value="view">仅查看屏幕</option>
                <option value="control">远程控制与文件传输</option>
                <option value="files">仅传输文件</option>
              </select>
            </label>
            {inviteTarget && !inviteTargetAvailable && (
              <p
                id="invite-target-unavailable"
                className="hint field-notice"
                role="status"
              >
                所选设备暂不支持此连接。请检查设备状态或重新选择。
              </p>
            )}
            <button
              disabled={busy || !!invite || !inviteTargetAvailable}
              className="secondary"
            >
              生成邀请码
            </button>
          </form>
          {!inviteTargets.length && !inviteTarget && (
            <p className="hint">
              {invitePermission === "files"
                ? "暂无可传输文件的在线电脑。请在对方电脑允许远程连接。"
                : "还没有开启共享的在线电脑。请先在那台电脑开启本机共享。"}
            </p>
          )}
          {invite && <p className="muted">生成新邀请前，请先取消当前邀请。</p>}
          {invite && generatedInvite && (
            <div className="invite-code">
              <span>
                {deviceLabel(generatedInvite.target, devices, state.deviceId)} ·
                将设备 ID 和邀请码一起发给对方
              </span>
              <code>{generatedInvite.target}</code>
              <span>允许：{permissionLabel(generatedInvite.permission)}</span>
              <span>一次性邀请码</span>
              <code>{inviteVisible ? invite : "邀请码已收起"}</code>
              <div className="row-actions">
                <button
                  disabled={busy}
                  onClick={() =>
                    void act(
                      () =>
                        copyText(
                          `设备 ID：${generatedInvite.target}\n允许：${permissionLabel(generatedInvite.permission)}\n一次性邀请码：${invite}`,
                        ),
                      "连接信息已复制",
                      false,
                    )
                  }
                >
                  复制连接信息
                </button>
                <button
                  onClick={() =>
                    setInvitation((previous) =>
                      previous
                        ? { ...previous, visible: !previous.visible }
                        : null,
                    )
                  }
                >
                  {inviteVisible ? "收起邀请码" : "显示邀请码"}
                </button>
                <button
                  className="danger-text"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      await api("revoke_invite", { id: inviteId });
                      setInvitation(null);
                    }, "邀请已取消")
                  }
                >
                  取消邀请
                </button>
              </div>
            </div>
          )}
        </div>
        <div
          className="card connection-list-pane"
          hidden={section !== "incoming"}
        >
          <div className="section-heading">
            <div>
              <div className="eyebrow">INBOX</div>
              <h2>连接请求</h2>
            </div>
            <span className="count">{pending.length}</span>
          </div>
          <p className="muted">只批准你认识的连接。</p>
          {pending.some((p) =>
            p.permission === "files" ? !state.filesEnabled : !state.sharing,
          ) && (
            <p className="hint">
              请先开启“允许远程连接”。
              <button
                className="text-button"
                onClick={() => onNavigate("settings")}
              >
                连接设置
              </button>
            </p>
          )}
          <div className="connection-list" tabIndex={0} aria-label="待批准列表">
            {pending.length ? (
              pending.map((p) => (
                <div className="list-row" key={p.id}>
                  <div>
                    <strong>
                      {p.source_device_name} · {permissionLabel(p.permission)}
                    </strong>
                    <small>申请人 {p.requester_email}</small>
                  </div>
                  <div className="row-actions">
                    <button
                      disabled={busy}
                      className="secondary"
                      onClick={() =>
                        void act(
                          () => api("decide", { id: p.id, approve: false }),
                          "已拒绝请求",
                        )
                      }
                    >
                      拒绝
                    </button>
                    <button
                      disabled={
                        busy ||
                        !(p.permission === "files"
                          ? state.filesEnabled
                          : state.sharing)
                      }
                      className="primary"
                      onClick={() =>
                        void act(async () => {
                          if (p.permission === "files")
                            await prepareTransport();
                          await api("decide", { id: p.id, approve: true });
                          if (p.permission === "files") onFiles(p.id);
                        }, "已允许连接")
                      }
                    >
                      {p.permission === "files"
                        ? "允许文件传输"
                        : p.permission === "control"
                          ? "允许控制"
                          : "允许查看"}
                    </button>
                  </div>
                </div>
              ))
            ) : (
              <Empty text="暂无待批准请求" />
            )}
          </div>
        </div>
        <div
          className="card connection-list-pane"
          hidden={!["active", "history"].includes(section)}
        >
          <div className="eyebrow">HISTORY</div>
          <h2 className="sr-only">
            {section === "history" ? "连接记录" : "当前连接"}
          </h2>
          <div
            className="connection-list"
            tabIndex={0}
            aria-label="远程连接列表"
          >
            {shownRequests.length ? (
              shownRequests.map((r) => (
                <div className="list-row" key={r.id}>
                  <div className="connection-description">
                    <strong>
                      {deviceLabel(r.source_device_id, devices, state.deviceId)}{" "}
                      →{" "}
                      {deviceLabel(r.target_device_id, devices, state.deviceId)}
                    </strong>
                    <small>
                      {permissionLabel(r.permission)} · {requestLabel(r.state)}
                    </small>
                    {r.state === "approved" && (
                      <small>
                        连接状态：
                        {pathLabel(paths[r.id]?.state, paths[r.id]?.discovery)}
                        {paths[r.id]?.rtt_ms != null
                          ? ` · 网络往返 ${paths[r.id].rtt_ms} 毫秒`
                          : ""}
                      </small>
                    )}
                    {paths[r.id]?.verification_code && (
                      <small>双方校验码：{paths[r.id].verification_code}</small>
                    )}
                    {paths[r.id]?.error && paths[r.id]?.state === "closed" && (
                      <small>结束原因：{paths[r.id].error}</small>
                    )}
                    <details className="connection-details">
                      <summary>连接详情</summary>
                      <small>连接编号：{r.id}</small>
                      <small>发起设备 ID：{r.source_device_id}</small>
                      <small>接收设备 ID：{r.target_device_id}</small>
                    </details>
                  </div>
                  <div className="row-actions">
                    {r.state === "approved" && r.permission === "files" && (
                      <button
                        className="secondary"
                        disabled={busy}
                        onClick={() =>
                          void act(
                            async () => {
                              if (
                                r.source_device_id === state.deviceId &&
                                !["connected", "direct", "relay"].includes(
                                  paths[r.id]?.state,
                                )
                              ) {
                                await prepareTransport();
                                await invoke("transport_connect", {
                                  id: r.id,
                                  permission: "files",
                                });
                              }
                              onFiles(r.id);
                            },
                            "",
                            false,
                          )
                        }
                      >
                        打开文件传输
                      </button>
                    )}
                    {r.state === "approved" &&
                      r.source_device_id === state.deviceId &&
                      r.permission !== "files" &&
                      !["connected", "direct", "relay"].includes(
                        paths[r.id]?.state,
                      ) && (
                        <button
                          className="secondary"
                          disabled={busy}
                          onClick={() =>
                            void act(
                              async () => {
                                await prepareTransport();
                                try {
                                  await invoke("transport_connect", {
                                    id: r.id,
                                    permission: r.permission,
                                  });
                                  return await onView(r.id);
                                } catch (e) {
                                  void api("revoke_remote", { id: r.id }).catch(
                                    () => {},
                                  );
                                  throw e;
                                }
                              },
                              "已打开独立远程窗口",
                              false,
                              {
                                retryLabel: "重新申请连接",
                                retry: () =>
                                  void act(async () => {
                                    await api("request", {
                                      source_device_id: r.source_device_id,
                                      target_device_id: r.target_device_id,
                                      permission: r.permission,
                                    });
                                    await onRefresh();
                                  }, "已重新申请连接，请等待对方批准"),
                              },
                            )
                          }
                        >
                          {r.permission === "control"
                            ? "远程控制"
                            : "仅查看屏幕"}
                        </button>
                      )}
                    {r.source_device_id === state.deviceId &&
                      r.state === "approved" &&
                      r.permission !== "files" &&
                      ["connected", "direct", "relay"].includes(
                        paths[r.id]?.state,
                      ) && (
                        <button
                          className="secondary"
                          onClick={() =>
                            void act(
                              () => onView(r.id),
                              "已打开独立远程窗口",
                              false,
                            )
                          }
                        >
                          返回画面
                        </button>
                      )}
                    {["pending", "approved"].includes(r.state) && (
                      <button
                        className="danger-text"
                        disabled={busy}
                        onClick={() =>
                          void act(
                            async () => {
                              await api("revoke_remote", { id: r.id });
                            },
                            r.state === "pending"
                              ? "连接请求已取消"
                              : "本次连接已结束",
                          )
                        }
                      >
                        {r.state === "pending" ? "取消请求" : "结束本次连接"}
                      </button>
                    )}
                  </div>
                </div>
              ))
            ) : (
              <Empty
                text={section === "history" ? "暂无连接记录" : "暂无连接"}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
type RemoteStatus = {
  files?: {
    supported: boolean;
    status?: {
      transfers: {
        id: string;
        name: string;
        size: number;
        transferred: number;
        direction: string;
        state: string;
      }[];
    } | null;
  };

  video?: {
    profile: number;
    generation: number;
    supported: boolean;
    max_profile?: number;
  };
  input?: { generation: number; blocked: boolean; message: string | null };
  mouse?: { generation: number; rejected: boolean };
  retryable?: boolean;
  state: string;
  rtt_ms: number | null;
  discovery?: string;
  permission: "view" | "control";
  verification_code: string | null;
  displays: RemoteDisplay[];
  error: string | null;
};
type Picture = {
  url: string;
  display: number;
  layout: number;
  sequence: number;
  width: number;
  height: number;
};
function ViewerWindow({ initialId }: { initialId: string }) {
  const [id, setId] = useState(initialId);
  const currentId = useRef(id);
  currentId.current = id;
  const displayChoice = useRef<RemoteDisplay | null>(null);
  const [restoring, setRestoring] = useState(false);
  const [recoveryNotice, setRecoveryNotice] = useState("");
  const noticeTimer = useRef<number | undefined>(undefined);
  const clearRecoveryNotice = useCallback(() => {
    window.clearTimeout(noticeTimer.current);
    setRecoveryNotice("");
  }, []);
  const reconnected = useCallback(
    (nextId: string) => {
      clearRecoveryNotice();
      setRestoring(true);
      setId(nextId);
    },
    [clearRecoveryNotice],
  );
  const restored = useCallback(
    (sessionId: string, display: RemoteDisplay | null) => {
      if (currentId.current !== sessionId) return;
      setRestoring(false);
      window.clearTimeout(noticeTimer.current);
      setRecoveryNotice(
        display ? `连接已恢复，继续显示 ${display.name}` : "连接与画面已恢复",
      );
      noticeTimer.current = window.setTimeout(
        () => setRecoveryNotice(""),
        4000,
      );
    },
    [],
  );
  useEffect(() => () => window.clearTimeout(noticeTimer.current), []);
  return (
    <Viewer
      key={id}
      id={id}
      onReconnect={reconnected}
      rememberedDisplay={displayChoice.current}
      resuming={restoring}
      onDisplayIntent={(display) => {
        displayChoice.current = { ...display };
      }}
      onRestored={restored}
      onRecoveryStart={clearRecoveryNotice}
      recoveryNotice={recoveryNotice}
      onStop={() => void invoke("viewer_window_action", { action: "close" })}
    />
  );
}
function Viewer({
  id,
  onStop,
  onReconnect,
  rememberedDisplay,
  resuming,
  onDisplayIntent,
  onRestored,
  onRecoveryStart,
  recoveryNotice,
}: {
  id: string;
  onStop: () => void;
  onReconnect: (id: string) => void;
  rememberedDisplay: RemoteDisplay | null;
  resuming: boolean;
  onDisplayIntent: (display: RemoteDisplay) => void;
  onRestored: (id: string, display: RemoteDisplay | null) => void;
  onRecoveryStart: () => void;
  recoveryNotice: string;
}) {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [picture, setPicture] = useState<Picture | null>(null);
  const [fps, setFps] = useState(0);
  const [text, setText] = useState("");
  const [problem, setProblem] = useState("");
  const image = useRef<HTMLImageElement>(null);
  const pointer = useRef<{
    id: number | null;
    buttons: number;
    point: { display: number; layout: number; x: number; y: number } | null;
  }>({ id: null, buttons: 0, point: null });
  const releasedPointerLeave = useRef<number | null>(null);
  const clearPointer = (releasedOutside = false) => {
    const id = pointer.current.id;
    releasedPointerLeave.current = releasedOutside ? id : null;
    pointer.current = { id: null, buttons: 0, point: null };
    if (id != null && image.current?.hasPointerCapture(id))
      image.current.releasePointerCapture(id);
  };
  const last = useRef(0);
  const frameTimes = useRef<number[]>([]);
  const url = useRef<string | null>(null);
  const inputQueue = useRef<Promise<unknown>>(Promise.resolve());
  const inputPending = useRef(0);
  const lastMove = useRef(0);
  const [ended, setEnded] = useState(false);
  const [fileBusy, setFileBusy] = useState(false);
  const fileInFlight = useRef(false);
  const swallowPasteUp = useRef(false);
  const [fileNote, setFileNote] = useState("");
  const [reconnecting, setReconnecting] = useState(false);
  const [reconnectFailed, setReconnectFailed] = useState(false);
  const [recoveryStatus, setRecoveryStatus] = useState<RecoveryStatus | null>(
    null,
  );
  const recoveryCycle = useRef(0);
  const minimumRecoveryCycle = useRef(0);
  const viewerLease = useRef({ active: true });
  const displayGate = useRef(
    new ViewerDisplayGate(rememberedDisplay, resuming),
  );
  const [displaySnapshot, setDisplaySnapshot] = useState<DisplaySnapshot>(() =>
    displayGate.current.snapshot(),
  );
  const [restoringConnection, setRestoringConnection] = useState(resuming);
  const resumePending = useRef(resuming);
  const receivedDisplay = useRef<number | null>(null);
  const currentDisplays = useRef<RemoteDisplay[]>([]);
  const displayBusy = displaySnapshot.stage !== "ready";
  const [retryingInput, setRetryingInput] = useState(false);
  const [dismissedInput, setDismissedInput] = useState<number | null>(null);
  const [dismissedMouse, setDismissedMouse] = useState<number | null>(null);
  const [mouseFailure, setMouseFailure] = useState<number | null>(null);
  useEffect(() => {
    if (!status?.mouse?.rejected || ended || status.input?.blocked) {
      setMouseFailure(null);
      return;
    }
    const generation = status.mouse.generation;
    const timer = window.setTimeout(() => setMouseFailure(generation), 1500);
    return () => window.clearTimeout(timer);
  }, [
    status?.mouse?.rejected,
    status?.mouse?.generation,
    status?.input?.blocked,
    ended,
  ]);
  const [toolbarVisible, setToolbarVisible] = useState(true);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [toolbarPinned, setToolbarPinned] = useState(false);
  const toolbarHovered = useRef(false);
  const toolbarFocused = useRef(false);
  const toolbar = useRef<HTMLElement>(null);
  const focusToolbarOnReveal = useRef(false);
  const toolbarTimer = useRef<number | undefined>(undefined);
  const [profileWanted, setProfileWanted] = useState<number | null>(null);
  const [profilePending, setProfilePending] = useState<number | null>(null);
  const [autoQuality, setAutoQuality] = useState(true);
  const [viewportEdge, setViewportEdge] = useState(0);
  const [fullscreen, setFullscreen] = useState(false);
  const [fitMode, setFitMode] = useState<"fill" | "contain">(() => {
    try {
      return localStorage.getItem("farsail.viewer.fit") === "contain"
        ? "contain"
        : "fill";
    } catch {
      return "fill";
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem("farsail.viewer.fit", fitMode);
    } catch {
      /* Session choice remains usable if storage is unavailable. */
    }
  }, [fitMode]);
  const windowAction = (action: string) =>
    void invoke<{ fullscreen?: boolean }>("viewer_window_action", { action })
      .then((s) => {
        if (typeof s.fullscreen === "boolean") setFullscreen(s.fullscreen);
      })
      .catch((e) => setProblem(errorText(e)));
  const requestProfile = (profile: number) => {
    setProfileWanted(profile);
    void invoke<number>("media_profile", { id, profile })
      .then(setProfilePending)
      .catch((err) => {
        setProfileWanted(null);
        setAutoQuality(false);
        setProblem(errorText(err));
      });
  };
  useEffect(() => {
    const refresh = () => windowAction("state");
    refresh();
    window.addEventListener("resize", refresh);
    return () => window.removeEventListener("resize", refresh);
  }, []);
  const showToolbar =
    toolbarVisible ||
    toolsOpen ||
    toolbarPinned ||
    reconnecting ||
    displayBusy ||
    fileBusy;
  const scheduleHide = () => {
    window.clearTimeout(toolbarTimer.current);
    if (
      !toolbarPinned &&
      !toolsOpen &&
      !toolbarHovered.current &&
      !toolbarFocused.current
    )
      toolbarTimer.current = window.setTimeout(() => {
        if (!toolbarFocused.current) setToolbarVisible(false);
      }, 2200);
  };
  useEffect(() => {
    if (toolbarVisible) {
      if (focusToolbarOnReveal.current) {
        focusToolbarOnReveal.current = false;
        toolbar.current
          ?.querySelector<HTMLElement>(
            "select:not(:disabled), button:not(:disabled), summary",
          )
          ?.focus({ preventScroll: true });
      }
      scheduleHide();
    }
    return () => window.clearTimeout(toolbarTimer.current);
  }, [toolbarVisible, toolbarPinned, toolsOpen]);
  useEffect(() => {
    if (
      profilePending != null &&
      status?.video &&
      status.video.generation >= profilePending
    ) {
      setProfilePending(null);
      setProfileWanted(null);
    }
  }, [profilePending, status?.video?.generation]);
  const inputBlocked = status?.input?.blocked ?? false;
  const inputBlockedRef = useRef(false);
  const inputGeneration = useRef(0);
  const screen = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = screen.current;
    if (!node) return;
    let timer: number;
    const measure = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const rect = node.getBoundingClientRect();
        setViewportEdge(
          Math.max(rect.width, rect.height) * window.devicePixelRatio,
        );
      }, 250);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    window.addEventListener("resize", measure);
    measure();
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
      window.clearTimeout(timer);
    };
  }, []);
  const selectedDisplay = status?.displays.find(
    (d) => d.id === (displaySnapshot.target ?? picture?.display ?? 1),
  );
  const sourceEdge = selectedDisplay
    ? Math.max(selectedDisplay.width, selectedDisplay.height)
    : 0;
  useEffect(() => {
    if (
      !autoQuality ||
      ended ||
      displayBusy ||
      !picture ||
      !status?.video?.supported ||
      !viewportEdge ||
      !sourceEdge ||
      profileWanted != null ||
      profilePending != null
    )
      return;
    const edge = Math.min(viewportEdge, sourceEdge);
    const target = Math.min(
      edge <= 1280 ? 0 : edge <= 1920 ? 1 : edge <= 2560 ? 2 : 3,
      status.video.max_profile ?? 1,
    );
    if (status.video.profile !== target) requestProfile(target);
  }, [
    autoQuality,
    ended,
    displayBusy,
    picture?.display,
    viewportEdge,
    sourceEdge,
    status?.video?.supported,
    status?.video?.profile,
    status?.video?.max_profile,
    profileWanted,
    profilePending,
  ]);
  const generation = useRef(0);
  const control =
    !ended &&
    !inputBlocked &&
    !fileBusy &&
    !displayBusy &&
    !!picture &&
    status?.state !== "closed" &&
    status?.permission === "control";
  const controlRef = useRef(control);
  controlRef.current = control;
  const send = (input: Record<string, unknown> | null) => {
    if (!controlRef.current || !displayGate.current.canControl()) return;
    if (input?.kind === "move") {
      if (performance.now() - lastMove.current < 30 || inputPending.current > 0)
        return;
      lastMove.current = performance.now();
    }
    if (inputPending.current >= 16) {
      setProblem("输入队列过长，请停止并重新连接");
      generation.current++;
      controlRef.current = false;
      setEnded(true);
      setPicture(null);
      void invoke("transport_close", { id }).catch(() => {});
      return;
    }
    const epoch = generation.current;
    const lease = viewerLease.current;
    const observedInputGeneration = inputGeneration.current;
    inputPending.current++;
    inputQueue.current = inputQueue.current
      .then(() =>
        dispatchViewerInput(
          () =>
            invoke("remote_input", {
              id,
              input,
              generation: observedInputGeneration,
            }),
          () =>
            lease.active &&
            epoch === generation.current &&
            controlRef.current &&
            displayGate.current.canControl(),
          (e) => {
            generation.current++;
            controlRef.current = false;
            setEnded(true);
            setPicture(null);
            setProblem(errorText(e));
            void invoke("transport_close", { id }).catch(() => {});
          },
        ),
      )
      .finally(() => {
        inputPending.current--;
      });
  };
  const pasteFiles = async (choose = false) => {
    if (
      !controlRef.current ||
      fileInFlight.current ||
      !status?.files?.supported
    )
      return;
    fileInFlight.current = true;
    const lease = viewerLease.current;
    generation.current++;
    clearPointer();
    setFileBusy(true);
    setFileNote(
      choose
        ? "请选择文件；文件将粘贴到远程当前文件夹。"
        : "正在准备剪贴板文件…",
    );
    try {
      await inputQueue.current;
      if (!lease.active) return;
      await invoke("remote_input", { id, input: null });
      if (!lease.active) return;
      const pasted = await invoke<boolean | undefined>(
        choose ? "remote_send_files" : "remote_paste_files",
        { id },
      );
      if (!lease.active) return;
      if (choose && !pasted) {
        setFileNote("");
        return;
      }
      if (!choose && !pasted) {
        // No local file list: preserve ordinary paste in the remote app.
        for (const [vk, down] of [
          [0xa2, true],
          [0x56, true],
          [0x56, false],
          [0xa2, false],
        ] as const) {
          if (!lease.active) return;
          await invoke("remote_input", {
            id,
            input: { kind: "key", vk, down },
            generation: inputGeneration.current,
          });
        }
        setFileNote("");
      } else setFileNote("已发送文件粘贴请求。");
    } catch (e) {
      if (lease.active) {
        setFileNote("");
        setProblem(errorText(e));
      }
    } finally {
      fileInFlight.current = false;
      if (lease.active) {
        setFileBusy(false);
        screen.current?.focus({ preventScroll: true });
      }
    }
  };
  const activeFile = status?.files?.status?.transfers.find(
    (t) =>
      t.direction === "send" && ["offered", "transferring"].includes(t.state),
  );
  useEffect(() => {
    if (!fileNote || fileBusy) return;
    const timer = window.setTimeout(() => setFileNote(""), 6000);
    return () => window.clearTimeout(timer);
  }, [fileNote, fileBusy]);
  const keyVk = (e: React.KeyboardEvent) => {
    if (e.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT) {
      if (e.key === "Control") return 0xa3;
      if (e.key === "Alt") return 0xa5;
      if (e.key === "Shift") return 0xa1;
    }
    if (e.location === KeyboardEvent.DOM_KEY_LOCATION_LEFT) {
      if (e.key === "Control") return 0xa2;
      if (e.key === "Alt") return 0xa4;
      if (e.key === "Shift") return 0xa0;
    }
    if (e.key === "Control") return 0xa2;
    if (e.key === "Alt") return 0xa4;
    if (e.key === "Shift") return 0xa0;
    return e.keyCode;
  };
  const releaseInputs = () => {
    generation.current++;
    clearPointer();
    send(null);
  };
  const recoverMouse = () => {
    generation.current++;
    clearPointer();
    send({ kind: "resume_mouse" });
  };
  const switchRemoteWindow = () => {
    if (!controlRef.current || !displayGate.current.canControl()) return;
    releaseInputs();
    send({ kind: "key", vk: 0xa4, down: true, repeat: false });
    send({ kind: "key", vk: 0x09, down: true, repeat: false });
    send({ kind: "key", vk: 0x09, down: false });
    send({ kind: "key", vk: 0xa4, down: false });
    screen.current?.focus({ preventScroll: true });
  };
  const resumeControl = () => {
    setRetryingInput(true);
    generation.current++;
    clearPointer();
    void invoke("remote_input", { id, input: { kind: "resume_control" } })
      .catch((e) => setProblem(errorText(e)))
      .finally(() => setRetryingInput(false));
  };
  const beginReconnect = (manual: boolean) => {
    const lease = viewerLease.current;
    onRecoveryStart();
    setReconnectFailed(false);
    setRecoveryStatus(null);
    minimumRecoveryCycle.current = recoveryCycle.current + 1;
    setReconnecting(true);
    setToolbarVisible(true);
    void invoke<{ id: string }>("viewer_reconnect", { manual })
      .then((r) => {
        if (lease.active) onReconnect(r.id);
      })
      .catch(async (e) => {
        if (!lease.active) return;
        const message = errorText(e);
        setProblem(message);
        try {
          const recovery = await invoke<RecoveryStatus>(
            "viewer_recovery_status",
          );
          if (!lease.active) return;
          setRecoveryStatus(recovery);
          setReconnectFailed(recovery.manualRetryAllowed);
        } catch {
          if (lease.active) setReconnectFailed(false);
        }
        if (!lease.active) return;
        setReconnecting(false);
      });
  };
  const reconnect = () => beginReconnect(true);
  useEffect(() => {
    if (!reconnecting && !restoringConnection) return;
    let active = true;
    let inFlight = false;
    const poll = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const next = await invoke<RecoveryStatus>("viewer_recovery_status");
        if (
          active &&
          next.cycle >= minimumRecoveryCycle.current &&
          next.cycle >= recoveryCycle.current
        ) {
          recoveryCycle.current = next.cycle;
          setRecoveryStatus(next);
        }
      } catch {
        /* Closing the window cancels this scoped status poll. */
      } finally {
        inFlight = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 300);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [reconnecting, restoringConnection]);
  const requestDisplay = (gate: ViewerDisplayGate, ticket: DisplayTicket) => {
    generation.current++;
    controlRef.current = false;
    clearPointer();
    setPicture(null);
    setFps(0);
    frameTimes.current = [];
    setText("");
    setToolbarVisible(true);
    setDisplaySnapshot(gate.snapshot());
    const intent = gate.intent();
    if (intent) onDisplayIntent(intent);
    void (async () => {
      try {
        if (status?.permission === "control")
          await invoke("remote_input", { id, input: null });
        if (!gate.current(ticket)) return;
        await invoke("media_select", { id, display: ticket.display });
        if (gate.requested(ticket)) setDisplaySnapshot(gate.snapshot());
      } catch (e) {
        if (gate.fail(ticket, "request")) {
          setDisplaySnapshot(gate.snapshot());
          setProblem(errorText(e));
        }
      }
    })();
  };
  useEffect(() => {
    const gate = displayGate.current;
    const list = status?.displays ?? [];
    currentDisplays.current = list;
    const ticket = gate.resolve(list);
    if (ticket) requestDisplay(gate, ticket);
    else {
      if (
        receivedDisplay.current != null &&
        gate.observeInitial(receivedDisplay.current, list)
      ) {
        const intent = gate.intent();
        if (intent) onDisplayIntent(intent);
      }
      setDisplaySnapshot(gate.snapshot());
    }
  }, [status?.displays, id]);
  useEffect(() => {
    if (
      displaySnapshot.stage !== "awaiting_frame" ||
      displaySnapshot.target == null
    )
      return;
    const gate = displayGate.current;
    const ticket = {
      epoch: displaySnapshot.epoch,
      display: displaySnapshot.target,
    };
    const timer = window.setTimeout(() => {
      if (gate.fail(ticket, "timeout")) setDisplaySnapshot(gate.snapshot());
    }, 10000);
    return () => window.clearTimeout(timer);
  }, [displaySnapshot.stage, displaySnapshot.epoch, displaySnapshot.target]);
  useEffect(() => {
    let live = true;
    const lease = { active: true };
    viewerLease.current = lease;
    // StrictMode replays effect setup/cleanup. Each setup needs its own gate;
    // callbacks from a stopped setup must never mutate the new one.
    const gate = new ViewerDisplayGate(rememberedDisplay, resuming);
    displayGate.current = gate;
    setDisplaySnapshot(gate.snapshot());
    last.current = 0;
    let receiving = true;
    let terminal = false;
    const finish = (s?: RemoteStatus, error?: unknown) => {
      if (!live || terminal) return;
      terminal = true;
      receiving = false;
      gate.stop();
      generation.current++;
      controlRef.current = false;
      clearPointer();
      setEnded(true);
      setPicture(null);
      setFps(0);
      frameTimes.current = [];
      if (url.current) {
        URL.revokeObjectURL(url.current);
        url.current = null;
      }
      setProblem(s?.error || (error ? errorText(error) : "连接已结束"));
      if (s?.retryable) {
        beginReconnect(false);
      }
    };
    const pollStatus = () =>
      void invoke<RemoteStatus>("remote_status", { id })
        .then((s) => {
          if (live) {
            if (
              s.state !== "closed" &&
              s.input &&
              s.input.generation < inputGeneration.current
            )
              return;
            if (
              (s.input && s.input.generation > inputGeneration.current) ||
              (s.input?.blocked && !inputBlockedRef.current)
            ) {
              generation.current++;
              controlRef.current = false;
              clearPointer();
              setText("");
              setToolbarVisible(true);
            }
            if (s.input) inputGeneration.current = s.input.generation;
            inputBlockedRef.current = s.input?.blocked ?? false;
            setStatus(s);
            if (s.state === "closed") finish(s);
          }
        })
        .catch((e) => {
          finish(undefined, e);
        });
    pollStatus();
    const statusTimer = window.setInterval(() => {
      if (!terminal) pollStatus();
    }, 750);
    const fpsTimer = window.setInterval(() => {
      const now = performance.now();
      frameTimes.current = frameTimes.current.filter((t) => now - t <= 4000);
      const times = frameTimes.current;
      setFps(
        times.length > 1
          ? Number(
              (
                ((times.length - 1) * 1000) /
                (times[times.length - 1] - times[0])
              ).toFixed(1),
            )
          : 0,
      );
    }, 1000);
    const receive = async () => {
      while (live && receiving) {
        try {
          const data = new Uint8Array(
            await invoke<ArrayBuffer>("media_next", {
              id,
              after: last.current,
            }),
          );
          if (!live || !receiving) break;
          if (data.length < 50) continue;
          const view = new DataView(
            data.buffer,
            data.byteOffset,
            data.byteLength,
          );
          if (
            String.fromCharCode(...data.slice(0, 4)) !== "FSM1" ||
            data[4] !== 1
          )
            continue;
          const sequence = Number(view.getBigUint64(17));
          if (sequence <= last.current) continue;
          last.current = sequence;
          const display = view.getUint32(5);
          const pending = !gate.canControl();
          if (!gate.acceptFrame(display)) continue;
          receivedDisplay.current = display;
          if (gate.observeInitial(display, currentDisplays.current)) {
            const intent = gate.intent();
            if (intent) onDisplayIntent(intent);
          }
          if (pending) setDisplaySnapshot(gate.snapshot());
          if (resumePending.current) {
            resumePending.current = false;
            setRestoringConnection(false);
            onRestored(id, gate.intent());
          }
          const next = URL.createObjectURL(
            new Blob([data.slice(49)], { type: "image/jpeg" }),
          );
          const old = url.current;
          url.current = next;
          frameTimes.current.push(performance.now());
          if (frameTimes.current.length > 240) frameTimes.current.shift();
          setPicture({
            url: next,
            display,
            layout: Number(view.getBigUint64(9)),
            sequence,
            width: view.getUint32(33),
            height: view.getUint32(37),
          });
          if (old) URL.revokeObjectURL(old);
        } catch (e) {
          if (live && !terminal) {
            try {
              finish(await invoke<RemoteStatus>("remote_status", { id }));
            } catch {
              finish(undefined, e);
            }
          }
          break;
        }
      }
    };
    void receive();
    const release = releaseInputs;
    window.addEventListener("blur", release);
    return () => {
      live = false;
      lease.active = false;
      gate.stop();
      window.clearInterval(statusTimer);
      window.clearInterval(fpsTimer);
      window.removeEventListener("blur", release);
      generation.current++;
      controlRef.current = false;
      clearPointer();
      if (url.current) URL.revokeObjectURL(url.current);
    };
  }, [id]);
  const point = (e: { clientX: number; clientY: number }, clamp = false) => {
    if (!picture || !image.current) return null;
    const rect = image.current.getBoundingClientRect();
    const scale = Math.min(
      rect.width / picture.width,
      rect.height / picture.height,
    );
    const w = fitMode === "fill" ? rect.width : picture.width * scale,
      h = fitMode === "fill" ? rect.height : picture.height * scale;
    const x = (e.clientX - rect.left - (rect.width - w) / 2) / w;
    const y = (e.clientY - rect.top - (rect.height - h) / 2) / h;
    if (clamp)
      return { x: Math.max(0, Math.min(1, x)), y: Math.max(0, Math.min(1, y)) };
    return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
  };
  const pointerInput = (e: React.PointerEvent<HTMLImageElement>) => {
    if (!controlRef.current || !picture || e.pointerType !== "mouse") return;
    const held = pointer.current;
    if (held.id != null && held.id !== e.pointerId) return;
    const p = point(e, held.buttons !== 0);
    const next = e.buttons & 3;
    // Hovering into the image with a button already held outside is not a new click.
    if (held.buttons === 0 && next !== 0 && e.type !== "pointerdown") return;
    if (held.buttons === 0 && next !== 0 && !p) return;
    if (p)
      held.point = { ...p, display: picture.display, layout: picture.layout };
    if (!held.point) return;
    if (next !== 0) {
      e.preventDefault();
      screen.current?.focus({ preventScroll: true });
      if (held.id == null) {
        releasedPointerLeave.current = null;
        held.id = e.pointerId;
        e.currentTarget.setPointerCapture(e.pointerId);
      }
    }
    const changed = held.buttons ^ next;
    for (const [mask, button] of [
      [1, "left"],
      [2, "right"],
    ] as const) {
      if (changed & mask)
        send({ kind: "button", ...held.point, button, down: !!(next & mask) });
    }
    held.buttons = next;
    if (e.type === "pointermove" && p) send({ kind: "move", ...held.point });
    // Clear our bookkeeping before implicit lost-capture, so a normal UP stays queued.
    if (held.id != null && next === 0) {
      const rect = e.currentTarget.getBoundingClientRect();
      clearPointer(
        e.clientX < rect.left ||
          e.clientX >= rect.right ||
          e.clientY < rect.top ||
          e.clientY >= rect.bottom,
      );
    }
  };
  return (
    <section
      className={`viewer-window ${fitMode === "fill" ? "viewer-fill" : ""} ${showToolbar ? "tools-visible" : ""} ${fullscreen ? "viewer-fullscreen" : ""}`}
      onMouseMove={(e) => {
        if (e.clientY <= (fullscreen ? 10 : 46) && !e.buttons)
          setToolbarVisible(true);
      }}
    >
      {!fullscreen && (
        <header className="window-titlebar viewer-titlebar">
          <div
            className="window-drag"
            onMouseDown={(e) => {
              if (e.button === 0 && e.detail === 1) windowAction("drag");
            }}
            onDoubleClick={() => windowAction("maximize")}
          >
            <span>
              FarSail <span className="window-subtitle">遥舟 · 远程桌面</span>
            </span>
          </div>
          <div className="window-controls">
            <button
              aria-label="最小化"
              title="最小化"
              onClick={() => windowAction("minimize")}
            >
              <Subtract16Regular />
            </button>
            <button
              aria-label="最大化或还原"
              title="最大化 / 还原"
              onClick={() => windowAction("maximize")}
            >
              <Square16Regular />
            </button>
            <button
              className="window-close"
              aria-label="关闭窗口"
              title="关闭窗口"
              onClick={onStop}
            >
              <Dismiss16Regular />
            </button>
          </div>
        </header>
      )}
      {!showToolbar && (
        <button
          className="viewer-reveal"
          onMouseEnter={() => setToolbarVisible(true)}
          onClick={(e) => {
            focusToolbarOnReveal.current = e.detail === 0;
            setToolbarVisible(true);
          }}
        >
          显示工具栏
        </button>
      )}
      <div className="viewer-body">
        <header
          ref={toolbar}
          className="viewer-toolbar"
          onFocusCapture={() => {
            toolbarFocused.current = true;
            window.clearTimeout(toolbarTimer.current);
            setToolbarVisible(true);
          }}
          onBlurCapture={(e) => {
            if (
              e.relatedTarget instanceof Node &&
              e.currentTarget.contains(e.relatedTarget)
            )
              return;
            toolbarFocused.current = false;
            scheduleHide();
          }}
          onMouseEnter={() => {
            toolbarHovered.current = true;
            window.clearTimeout(toolbarTimer.current);
          }}
          onMouseLeave={() => {
            toolbarHovered.current = false;
            scheduleHide();
          }}
        >
          <div className="viewer-status">
            <strong>
              {reconnecting
                ? "正在恢复连接"
                : displayBusy && !ended
                  ? "正在恢复屏幕"
                  : ended
                    ? "会话已结束"
                    : inputBlocked
                      ? "控制已暂停"
                      : status?.permission === "control"
                        ? "远程控制"
                        : "仅查看"}
            </strong>
            <span className="muted">
              {pathLabel(status?.state, status?.discovery)} ·{" "}
              {status?.rtt_ms ?? "—"} 毫秒 ·{" "}
              {fps > 0 ? `${fps} 帧/秒` : picture ? "画面暂未更新" : "等待画面"}
            </span>
          </div>
          <label className="viewer-display-selector">
            <span className="sr-only">对方的显示器</span>
            <select
              aria-label="对方的显示器"
              disabled={
                ended ||
                !status?.displays.length ||
                displaySnapshot.stage === "selecting" ||
                displaySnapshot.stage === "awaiting_frame"
              }
              value={
                displaySnapshot.stage === "unavailable"
                  ? ""
                  : (displaySnapshot.target ?? picture?.display ?? "")
              }
              onChange={(e) => {
                const display = status?.displays.find(
                  (d) => d.id === Number(e.target.value),
                );
                if (!display) return;
                const gate = displayGate.current;
                const ticket = gate.choose(display);
                if (ticket) requestDisplay(gate, ticket);
              }}
            >
              <option value="" disabled>
                选择要显示的屏幕
              </option>
              {status?.displays.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} · {d.width}×{d.height} · {d.dpi} DPI
                </option>
              ))}
            </select>
          </label>
          <label className="viewer-quality-selector">
            <span className="sr-only">画面分辨率</span>
            <select
              aria-label="画面分辨率"
              disabled={ended || !status?.video?.supported}
              value={
                status?.video?.supported
                  ? autoQuality
                    ? -2
                    : (profileWanted ?? status.video.profile)
                  : -1
              }
              title={
                !status?.video?.supported ? "被控端需更新后才能切换" : undefined
              }
              onChange={(e) => {
                const profile = Number(e.target.value);
                setAutoQuality(profile === -2);
                if (profile !== -2) requestProfile(profile);
              }}
            >
              <option value="-1" disabled>
                {picture ? "对方需更新" : "等待画面"}
              </option>
              <option value="-2">自动适配</option>
              <option value="0">720p · 省流</option>
              <option value="1">1080p · 高清</option>
              <option
                value="2"
                disabled={(status?.video?.max_profile ?? 1) < 2}
              >
                2K · 超清
              </option>
              <option
                value="3"
                disabled={(status?.video?.max_profile ?? 1) < 3}
              >
                4K · 超清
              </option>
            </select>
          </label>

          <div className="viewer-toolbar-actions">
            {status?.permission === "control" && (
              <button
                disabled={!control || !status.files?.supported}
                title={
                  status.files?.supported
                    ? undefined
                    : "需两端更新至支持远控文件传输的版本"
                }
                onClick={() => void pasteFiles(true)}
              >
                <Folder20Regular aria-hidden="true" />
                {fileBusy ? "正在传输…" : "传输文件"}
              </button>
            )}
            <details
              className="viewer-more"
              onToggle={(e) => setToolsOpen(e.currentTarget.open)}
            >
              <summary>更多操作</summary>
              <div className="viewer-popover">
                <label className="viewer-pin">
                  <input
                    type="checkbox"
                    checked={toolbarPinned}
                    onChange={(e) => setToolbarPinned(e.target.checked)}
                  />
                  保持工具栏显示
                </label>
                <div className="viewer-fit-settings">
                  <strong>画面显示</strong>
                  <div
                    className="section-switcher"
                    role="group"
                    aria-label="画面显示方式"
                  >
                    {(
                      [
                        ["fill", "铺满窗口"],
                        ["contain", "保持比例"],
                      ] as const
                    ).map(([mode, label]) => (
                      <button
                        type="button"
                        key={mode}
                        className={fitMode === mode ? "active" : ""}
                        aria-pressed={fitMode === mode}
                        onClick={() => {
                          releaseInputs();
                          setFitMode(mode);
                        }}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="viewer-connection-info">
                  <strong>连接信息</strong>
                  <p className="muted">
                    当前接收画面：
                    {picture
                      ? `${picture.width}×${picture.height}`
                      : "等待画面"}
                    <br />
                    连接编号：{id}
                    <br />
                    {pathLabel(status?.state, status?.discovery)} · 网络往返{" "}
                    {status?.rtt_ms ?? "—"} 毫秒 ·{" "}
                    {fps > 0 ? `${fps} 帧/秒（近 4 秒）` : "等待画面更新"}
                    <br />
                    连接核对码：{status?.verification_code ?? "正在获取"}
                    。请与对方确认两端显示一致。
                  </p>
                </div>
                {(control || (inputBlocked && !ended)) && (
                  <div className="viewer-input-actions">
                    {control && (
                      <button type="button" onClick={recoverMouse}>
                        恢复鼠标控制
                      </button>
                    )}
                    {control && (
                      <button type="button" onClick={switchRemoteWindow}>
                        切换远端窗口（Alt+Tab）
                      </button>
                    )}
                    {inputBlocked && !ended && (
                      <button
                        type="button"
                        disabled={retryingInput}
                        onClick={resumeControl}
                      >
                        重试控制
                      </button>
                    )}
                  </div>
                )}
                {control && (
                  <form
                    className="inline-form"
                    onSubmit={(e) => {
                      e.preventDefault();
                      if (text) send({ kind: "text", text });
                      setText("");
                    }}
                  >
                    <input
                      value={text}
                      onChange={(e) => setText(e.target.value)}
                      maxLength={64}
                      placeholder="向受控窗口输入文字"
                      aria-label="远端文字"
                    />
                    <button className="secondary">发送文字</button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => send(null)}
                    >
                      松开所有按键
                    </button>
                  </form>
                )}
                <p className="hint">系统授权与登录画面需在被控电脑上处理。</p>
              </div>
            </details>
            <button onClick={() => windowAction("fullscreen")}>全屏切换</button>
            <button className="danger-text" onClick={onStop}>
              {reconnecting ? "取消重连" : "结束连接"}
            </button>
          </div>
        </header>
        {profilePending != null && (
          <div className="notice-strip" role="status">
            正在切换画面分辨率…
          </div>
        )}
        {(reconnecting ||
          (displayBusy && !ended) ||
          recoveryNotice ||
          (!ended && (fileBusy || fileNote))) && (
          <div
            className="viewer-session-notice"
            role="status"
            aria-live="polite"
          >
            {reconnecting
              ? recoveryStageText(recoveryStatus)
              : displayBusy
                ? displaySnapshot.stage === "unavailable"
                  ? displaySnapshot.issue === "timeout"
                    ? "暂未收到所选屏幕的画面，请重新选择屏幕。"
                    : "原选中屏幕暂不可用或布局已变化，请选择要显示的屏幕。"
                  : "连接已建立，正在恢复所选屏幕…"
                : fileBusy || fileNote
                  ? activeFile
                    ? `正在传输 ${activeFile.name} · ${activeFile.size ? Math.round((activeFile.transferred / activeFile.size) * 100) : 0}%`
                    : fileNote
                  : recoveryNotice}
          </div>
        )}
        <FailureDialog
          error={!reconnecting ? problem : ""}
          message={
            /transport timeout/i.test(problem)
              ? "当前操作未能发送到远程电脑。你可以稍后重试。"
              : undefined
          }
          onDismiss={() => setProblem("")}
          action={
            reconnectFailed
              ? {
                  title: "暂时无法恢复远程连接",
                  retry: reconnect,
                  retryLabel: "重新尝试连接",
                }
              : /transport timeout/i.test(problem)
                ? { title: "远程操作超时" }
                : undefined
          }
        />
        <FailureDialog
          error={
            inputBlocked &&
            !ended &&
            !displayBusy &&
            !problem &&
            dismissedInput !== status?.input?.generation
              ? (status?.input?.message ??
                "系统暂时无法执行鼠标键盘操作，画面连接仍保留。")
              : ""
          }
          message="远程电脑暂时无法执行鼠标键盘操作。画面连接仍保留，你可以重试控制。"
          onDismiss={() => setDismissedInput(status?.input?.generation ?? 0)}
          action={{
            title: "控制已暂停",
            retry: resumeControl,
            retryLabel: "重试控制",
          }}
        />
        <FailureDialog
          error={
            mouseFailure !== null &&
            mouseFailure !== dismissedMouse &&
            !ended &&
            !inputBlocked &&
            !problem
              ? "暂时无法执行鼠标定位操作。画面连接保持，你可以尝试恢复鼠标控制。"
              : ""
          }
          onDismiss={() => setDismissedMouse(mouseFailure)}
          action={{
            title: "鼠标控制暂时不可用",
            retry: recoverMouse,
            retryLabel: "恢复鼠标控制",
          }}
        />
        <div
          className="remote-screen"
          ref={screen}
          onBlur={releaseInputs}
          tabIndex={control ? 0 : -1}
          onKeyDown={(e) => {
            if (!control) return;
            e.preventDefault();
            if (
              (e.ctrlKey || e.metaKey) &&
              !e.altKey &&
              !e.shiftKey &&
              e.code === "KeyV" &&
              status?.files?.supported
            ) {
              swallowPasteUp.current = true;
              if (!e.repeat) void pasteFiles();
              return;
            }
            send({ kind: "key", vk: keyVk(e), down: true, repeat: e.repeat });
          }}
          onKeyUp={(e) => {
            if (e.code === "KeyV" && swallowPasteUp.current) {
              swallowPasteUp.current = false;
              e.preventDefault();
              return;
            }
            if (!control) return;
            e.preventDefault();
            send({ kind: "key", vk: keyVk(e), down: false });
          }}
        >
          {picture ? (
            <img
              ref={image}
              src={picture.url}
              alt="远端桌面"
              style={{ cursor: inputBlocked ? "not-allowed" : "default" }}
              draggable={false}
              onPointerDown={pointerInput}
              onPointerMove={pointerInput}
              onPointerUp={pointerInput}
              onPointerCancel={releaseInputs}
              onLostPointerCapture={() => {
                if (pointer.current.buttons) releaseInputs();
              }}
              onPointerLeave={(e) => {
                // Capture release outside causes a leave after normal UP. Release
                // keys in order without invalidating that final positioned UP.
                if (releasedPointerLeave.current === e.pointerId) {
                  releasedPointerLeave.current = null;
                  send(null);
                  return;
                }
                if (!pointer.current.buttons) releaseInputs();
              }}
              onContextMenu={(e) => e.preventDefault()}
              onWheel={(e) => {
                e.preventDefault();
                const p = point(e);
                if (p)
                  send({
                    kind: "wheel",
                    display: picture.display,
                    layout: picture.layout,
                    ...p,
                    vertical: Math.max(
                      -1200,
                      Math.min(1200, Math.round(-e.deltaY)),
                    ),
                    horizontal: Math.max(
                      -1200,
                      Math.min(1200, Math.round(e.deltaX)),
                    ),
                  });
              }}
            />
          ) : (
            <div className="viewer-empty-state">
              <p>
                {reconnecting
                  ? recoveryStageText(recoveryStatus)
                  : ended
                    ? "连接已结束"
                    : displaySnapshot.stage === "unavailable"
                      ? "请选择要显示的屏幕。"
                      : restoringConnection
                        ? "正在恢复原选中的屏幕…"
                        : displayBusy
                          ? "正在切换屏幕…"
                          : "正在等待远端画面…"}
              </p>
              {reconnecting && (
                <button className="secondary" onClick={onStop}>
                  取消重连
                </button>
              )}
              {reconnectFailed && !reconnecting && (
                <button className="primary" onClick={reconnect}>
                  重新尝试连接
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function Security({
  me,
  busy,
  act,
  signOut,
  onConnectionReset,
}: {
  me: Me;
  busy: boolean;
  act: Action;
  signOut: () => Promise<void>;
  onConnectionReset: () => void;
}) {
  const [oldPassword, setOld] = useState(""),
    [newPassword, setNew] = useState(""),
    [loadError, setLoadError] = useState(""),
    [sessionsLoading, setSessionsLoading] = useState(true),
    [sessionsLoaded, setSessionsLoaded] = useState(false),
    [sessionsFailed, setSessionsFailed] = useState(false),
    [sessions, setSessions] = useState<
      {
        id: string;
        created_at: string;
        last_used_at: string;
        current: boolean;
      }[]
    >([]);
  const sessionsRequest = useRef({
    active: false,
    context: 0,
    revision: 0,
    pending: false,
  });
  const loadSessions = useCallback(async (replace = false) => {
    const request = sessionsRequest.current;
    if (!request.active || (request.pending && !replace)) return;
    const revision = ++request.revision;
    request.pending = true;
    const current = () => request.active && request.revision === revision;
    setSessionsLoading(true);
    setSessionsFailed(false);
    setLoadError("");
    try {
      const rows = await api<typeof sessions>("sessions");
      if (!current()) return;
      setSessions(rows);
      setSessionsLoaded(true);
    } catch (e) {
      if (!current()) return;
      setSessionsFailed(true);
      setLoadError(errorText(e));
    } finally {
      if (current()) {
        request.pending = false;
        setSessionsLoading(false);
      }
    }
  }, []);
  useEffect(() => {
    const request = sessionsRequest.current;
    request.active = true;
    request.context++;
    setSessions([]);
    setSessionsLoaded(false);
    void loadSessions();
    return () => {
      request.active = false;
      request.context++;
      request.revision++;
      request.pending = false;
    };
  }, [me.id, loadSessions]);
  return (
    <div className="stack">
      <FailureDialog
        error={loadError}
        onDismiss={() => setLoadError("")}
        action={{
          title: "无法加载登录记录",
          retry: () => void loadSessions(),
        }}
      />
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">PASSWORD</div>
          <h2>修改密码</h2>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(async () => {
                onConnectionReset();
                await api("password", {
                  old_password: oldPassword,
                  new_password: newPassword,
                });
                setOld("");
                setNew("");
              }, "密码修改成功");
            }}
          >
            <label>
              当前密码
              <input
                type="password"
                required
                value={oldPassword}
                onChange={(e) => setOld(e.target.value)}
              />
            </label>
            <label>
              新密码
              <input
                type="password"
                required
                minLength={12}
                aria-label="新密码"
                aria-describedby="security-password-hint"
                value={newPassword}
                onChange={(e) => setNew(e.target.value)}
              />
              <small id="security-password-hint" className="muted">
                至少 12 个字符
              </small>
            </label>
            <button className="primary" disabled={busy}>
              修改密码
            </button>
          </form>
        </div>
        <div className="card">
          <div className="eyebrow">ACCOUNT</div>
          <h2>登录状态</h2>
          <p className="muted">{me.email}</p>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void signOut()}
          >
            退出登录
          </button>
        </div>
      </div>
      <div className="card">
        <div className="section-heading">
          <div>
            <div className="eyebrow">SESSIONS</div>
            <h2>账号登录记录</h2>
          </div>
          <button
            className="text-button"
            aria-label="刷新登录记录"
            disabled={busy || sessionsLoading}
            onClick={() => void loadSessions()}
          >
            {sessionsLoading
              ? sessionsLoaded
                ? "刷新中…"
                : "加载中…"
              : "刷新"}
          </button>
        </div>
        {sessionsFailed && sessions.length > 0 && (
          <p className="hint" role="status">
            刷新失败，显示上次记录。
          </p>
        )}
        <div
          className="login-records"
          tabIndex={0}
          aria-label="账号登录记录列表"
          aria-busy={sessionsLoading}
        >
          {sessions.length ? (
            sessions.map((x) => (
              <div className="list-row" key={x.id}>
                <div>
                  <strong>
                    {x.id === me.session_id ? "当前客户端" : "其他客户端"}
                  </strong>
                  <small>
                    登录时间：{dateLabel(x.created_at)}
                    <br />
                    最近使用：{dateLabel(x.last_used_at)}
                  </small>
                  <details className="connection-details">
                    <summary>登录详情</summary>
                    <small>登录编号：{x.id}</small>
                  </details>
                </div>
                {x.id !== me.session_id && (
                  <button
                    className="danger-text"
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        const context = sessionsRequest.current.context;
                        await api("revoke_session", { id: x.id });
                        if (
                          !sessionsRequest.current.active ||
                          sessionsRequest.current.context !== context
                        )
                          return;
                        setSessions((rows) =>
                          rows.filter((row) => row.id !== x.id),
                        );
                        await loadSessions(true);
                      }, "已退出该客户端的登录")
                    }
                  >
                    退出此登录
                  </button>
                )}
              </div>
            ))
          ) : sessionsLoading && !sessionsLoaded ? (
            <div role="status">
              <Empty text="正在加载登录记录…" />
            </div>
          ) : sessionsFailed ? (
            <Empty text="登录记录暂时不可用，请刷新重试。" />
          ) : (
            <Empty text="暂无登录记录" />
          )}
        </div>
      </div>
    </div>
  );
}
function Admin({
  busy,
  act,
  invitation,
  setInvitation,
}: {
  busy: boolean;
  act: Action;
  invitation: SignupInvitation | null;
  setInvitation: React.Dispatch<React.SetStateAction<SignupInvitation | null>>;
}) {
  const [users, setUsers] = useState<User[]>([]),
    [after, setAfter] = useState(""),
    [registration, setRegistration] = useState(""),
    [email, setEmail] = useState(invitation?.email ?? ""),
    [loadError, setLoadError] = useState(""),
    [deviceId, setDeviceId] = useState(""),
    [sessions, setSessions] = useState<
      { id: string; user_id: string; created_at: string; revoked: boolean }[]
    >([]),
    [audit, setAudit] = useState<
      {
        id: number;
        actor_id: string | null;
        action: string;
        object_id: string | null;
        result: string;
        created_at: string;
      }[]
    >([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const loadRequest = useRef({ active: false, revision: 0, pending: false });
  const invite = invitation?.code ?? "";
  const inviteId = invitation?.id ?? "";
  const inviteVisible = invitation?.visible ?? true;
  const invitedEmail = invitation?.email ?? "";
  const auditLabels: Record<string, string> = {
    register: "创建账号",
    verify_email: "验证邮箱",
    login: "登录账号",
    logout: "退出账号",
    change_password: "修改密码",
    recover_password: "找回密码",
    revoke_session: "退出客户端登录",
    bind_device: "添加设备",
    unbind_device: "移除设备",
    request_remote: "申请远程连接",
    decide_remote: "处理连接申请",
    create_invitation: "创建连接邀请",
    set_user_enabled: "调整账号状态",
    admin_revoke_device: "停用设备",
    admin_enable_device: "恢复设备",
    registration_policy: "修改注册方式",
    create_signup_invitation: "创建注册邀请",
    revoke_signup_invitation: "取消注册邀请",
  };
  const resultLabel = (value: string) =>
    ({
      ok: "已完成",
      pending: "等待批准",
      approved: "已允许",
      denied: "已拒绝",
      true: "已启用",
      false: "已停用",
      open: "开放注册",
      invite_only: "仅限邀请",
      closed: "关闭注册",
    })[value] ?? "详见记录详情";
  const load = useCallback(async () => {
    const request = loadRequest.current;
    if (!request.active || request.pending) return false;
    const revision = ++request.revision;
    const current = () => request.active && request.revision === revision;
    request.pending = true;
    setLoading(true);
    setFailed(false);
    setLoadError("");
    try {
      const [u, r, s, a] = await Promise.all([
        api<User[]>("admin_users"),
        api<{ value: string }>("admin_registration"),
        api<typeof sessions>("admin_sessions"),
        api<typeof audit>("admin_audit"),
      ]);
      if (!current()) return false;
      setUsers(u);
      setHasMore(u.length === 100);
      setAfter(u.at(-1)?.id ?? "");
      setRegistration(r.value);
      setSessions(s);
      setAudit(a);
      setLoaded(true);
      return true;
    } catch (e) {
      if (current()) {
        setFailed(true);
        setLoadError(errorText(e));
      }
      return false;
    } finally {
      if (current()) {
        request.pending = false;
        setLoading(false);
      }
    }
  }, []);
  useEffect(() => {
    const request = loadRequest.current;
    request.active = true;
    void load();
    return () => {
      request.active = false;
      request.revision++;
      request.pending = false;
    };
  }, [load]);
  const loadMore = () =>
    void act(
      async () => {
        const revision = loadRequest.current.revision;
        const u = await api<User[]>("admin_users", { after });
        if (
          !loadRequest.current.active ||
          loadRequest.current.revision !== revision
        )
          return HANDLED_FAILURE;
        setUsers((x) => [...x, ...u]);
        setAfter(u.at(-1)?.id ?? "");
        setHasMore(u.length === 100);
      },
      "已加载更多用户",
      false,
    );
  return (
    <div className="stack">
      <PageActions>
        <button
          className="text-button"
          aria-label="刷新管理数据"
          disabled={busy || loading}
          onClick={() =>
            void act(
              async () => ((await load()) ? undefined : HANDLED_FAILURE),
              "管理数据已刷新",
              false,
            )
          }
        >
          <ArrowSync20Regular aria-hidden="true" />
          {loading ? (loaded ? "刷新中…" : "加载中…") : "刷新"}
        </button>
      </PageActions>
      {loadError && (
        <FailureDialog
          error={loadError}
          onDismiss={() => setLoadError("")}
          action={{ title: "无法加载管理数据", retry: () => void load() }}
        />
      )}
      {loading && !loaded && (
        <p className="hint" role="status">
          正在加载管理数据…
        </p>
      )}
      {failed && (
        <p className="hint" role="status">
          {loaded
            ? "刷新失败，当前显示上次获取的管理数据。"
            : "管理数据暂时不可用，请刷新重试。"}
        </p>
      )}
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">REGISTRATION</div>
          <h2>注册方式</h2>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              if (busy || !loaded || failed || loadRequest.current.pending)
                return;
              void act(
                () => api("admin_set_registration", { value: registration }),
                "注册方式已更新",
              );
            }}
          >
            <select
              aria-label="账号注册方式"
              value={registration}
              disabled={busy || loading || !loaded || failed}
              onChange={(e) => setRegistration(e.target.value)}
            >
              {!loaded && (
                <option value="">{loading ? "加载中…" : "暂不可用"}</option>
              )}
              <option value="open">开放注册</option>
              <option value="invite_only">仅限邀请</option>
              <option value="closed">关闭注册</option>
            </select>
            <button
              className="primary"
              disabled={busy || loading || !loaded || failed}
            >
              保存
            </button>
          </form>
        </div>
        <div className="card">
          <div className="eyebrow">INVITATION</div>
          <h2>邀请注册</h2>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                async () => {
                  const v = await api<{ code: string; id: string }>(
                    "admin_invite",
                    {
                      email,
                    },
                  );
                  setInvitation({
                    id: v.id,
                    code: v.code,
                    email,
                    visible: true,
                  });
                },
                "注册邀请码已生成",
                false,
              );
            }}
          >
            <input
              type="email"
              placeholder="受邀邮箱"
              aria-label="受邀邮箱"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
            <button className="primary" disabled={busy || !!invite}>
              生成邀请码
            </button>
          </form>
          {invite && (
            <div className="invite-code">
              <span>受邀邮箱：{invitedEmail}</span>
              <code>{inviteVisible ? invite : "邀请码已收起"}</code>
              <button
                disabled={busy}
                onClick={() =>
                  void act(
                    () =>
                      copyText(
                        `注册邮箱：${invitedEmail}\n注册邀请码：${invite}`,
                      ),
                    "注册邀请已复制",
                    false,
                  )
                }
              >
                复制注册邀请
              </button>
              <button
                onClick={() =>
                  setInvitation((previous) =>
                    previous
                      ? { ...previous, visible: !previous.visible }
                      : null,
                  )
                }
              >
                {inviteVisible ? "收起邀请码" : "显示邀请码"}
              </button>
              <button
                className="danger-text"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await api("admin_invite_revoke", { id: inviteId });
                    setInvitation(null);
                  }, "注册邀请已撤销")
                }
              >
                取消注册邀请
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="card">
        <div className="eyebrow">DEVICE POLICY</div>
        <h2>设备管理</h2>
        <div className="inline-form">
          <input
            placeholder="粘贴要管理的完整设备 ID"
            aria-label="管理员设备 ID"
            value={deviceId}
            onChange={(e) => setDeviceId(e.target.value)}
          />
          <button
            className="secondary"
            disabled={busy || !deviceId}
            onClick={() =>
              void act(
                () => api("admin_device_revoke", { id: deviceId }),
                "设备已停用",
              )
            }
          >
            停用此设备
          </button>
          <button
            className="secondary"
            disabled={busy || !deviceId}
            onClick={() =>
              void act(
                () => api("admin_device_enabled", { id: deviceId }),
                "设备已恢复，请让设备所有者在该电脑重新添加设备",
              )
            }
          >
            恢复此设备
          </button>
        </div>
      </div>
      <div className="card">
        <div className="eyebrow">USERS</div>
        <h2>用户管理</h2>
        {users.map((u) => (
          <div className="list-row" key={u.id}>
            <div>
              <strong>{u.email}</strong>
              <small>
                {u.role === "admin" ? "管理员" : "普通用户"} ·{" "}
                {u.verified ? "邮箱已验证" : "邮箱未验证"} ·{" "}
                {u.enabled ? "启用" : "停用"} · {short(u.id)}
              </small>
            </div>
            <button
              className={u.enabled ? "danger-text" : "secondary"}
              disabled={busy || loading}
              onClick={() =>
                void act(
                  async () => {
                    await api("admin_user_enabled", {
                      id: u.id,
                      enabled: !u.enabled,
                    });
                    await load();
                  },
                  u.enabled ? "用户已停用" : "用户已启用",
                )
              }
            >
              {u.enabled ? "停用账号" : "恢复账号"}
            </button>
          </div>
        ))}
        {loaded && !users.length && <Empty text="暂无用户" />}
        {hasMore && (
          <button
            className="text-button"
            disabled={busy || loading}
            onClick={loadMore}
          >
            加载更多
          </button>
        )}
      </div>
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">SESSIONS</div>
          <h2>账号登录记录</h2>
          {sessions.length ? (
            sessions.map((s) => (
              <div className="list-row" key={s.id}>
                <div>
                  <strong>
                    {users.find((u) => u.id === s.user_id)?.email ??
                      `账号 ${short(s.user_id)}`}
                  </strong>
                  <small>
                    {s.revoked ? "已退出" : "尚未退出（可能已过期）"} ·{" "}
                    {dateLabel(s.created_at)}
                  </small>
                  <details className="connection-details">
                    <summary>记录详情</summary>
                    <small>登录编号：{s.id}</small>
                    <small>账号编号：{s.user_id}</small>
                  </details>
                </div>
              </div>
            ))
          ) : loaded ? (
            <Empty text="暂无登录记录" />
          ) : null}
        </div>
        <div className="card">
          <div className="eyebrow">AUDIT</div>
          <h2>操作记录</h2>
          {audit.length ? (
            audit.map((a) => (
              <div className="list-row" key={a.id}>
                <div>
                  <strong>
                    {auditLabels[a.action] ?? "其他操作"} ·{" "}
                    {resultLabel(a.result)}
                  </strong>
                  <small>{dateLabel(a.created_at)}</small>
                  <details className="connection-details">
                    <summary>记录详情</summary>
                    <small>
                      操作：{a.action} · 结果：{a.result}
                    </small>
                    <small>操作账号：{a.actor_id ?? "系统或设备"}</small>
                    <small>关联编号：{a.object_id ?? "无"}</small>
                  </details>
                </div>
              </div>
            ))
          ) : loaded ? (
            <Empty text="暂无操作记录" />
          ) : null}
        </div>
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {new URLSearchParams(window.location.search).get("viewer") ? (
      <ViewerWindow
        initialId={new URLSearchParams(window.location.search).get("viewer")!}
      />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
