import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Device, Me, Pending, Remote, User } from "@farsail/ui";
import "./style.css";

type Tab =
  | "overview"
  | "devices"
  | "requests"
  | "viewer"
  | "security"
  | "admin"
  | "settings";
type PublicState = {
  computerName?: string | null;
  server: string;
  signedIn: boolean;
  deviceId: string | null;
  sharing: boolean;
  remoteWatch?: boolean;
};
type ConnectionInvitation = {
  id: string;
  code: string;
  target: string;
  permission: "view" | "control";
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
  if (/no interactive display/i.test(raw))
    return "无法获取本机屏幕，请先登录 Windows 桌面再开启共享。";
  if (/relay URL|relay did not become reachable/i.test(raw))
    return "无法连接中继服务器，请在高级连接设置中检查 HTTPS 地址和网络。";
  if (/invalid UDP bind/i.test(raw))
    return "本机监听地址格式不正确，可恢复为 0.0.0.0:0 后重试。";
  if (/unbound|device not bound/i.test(raw))
    return "请先将这台电脑添加到账号，再尝试连接。";
  if (/[\u3400-\u9fff]/.test(raw)) return raw;
  return "操作未完成，请检查网络和服务器设置后重试。";
};
function ErrorMessage({ error }: { error: string }) {
  const message = readableError(error);
  return (
    <div className="error-message">
      <span>{message}</span>
      {message !== error && (
        <details>
          <summary>查看错误详情</summary>
          <code>{error}</code>
        </details>
      )}
    </div>
  );
}
const copyText = async (value: string) => {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    throw new Error("无法自动复制，请选中显示的内容后手动复制。");
  }
};
const short = (id: string) => id.slice(0, 8);
const permissionLabel = (value: string) =>
  ({ view: "仅查看屏幕", control: "查看并控制", files: "文件传输" })[value] ??
  "未知权限";
const requestLabel = (value: string) =>
  ({
    pending: "等待批准",
    approved: "已批准",
    denied: "已拒绝",
    revoked: "已取消或结束",
    expired: "已过期",
  })[value] ?? "状态待确认";
const pathLabel = (value?: string) =>
  ({
    direct: "已直连",
    relay: "已通过中继连接",
    connected: "已连接",
    connecting: "正在连接",
    closed: "已断开",
  })[value ?? ""] ?? "尚未连接";
const deviceLabel = (id: string, devices: Device[], localId: string | null) =>
  `${devices.find((d) => d.id === id)?.name ?? `设备 ${short(id)}`}${id === localId ? "（本机）" : ""}`;
const dateLabel = (value: string) => new Date(value).toLocaleString();

function App() {
  const [publicState, setPublicState] = useState<PublicState>({
    server: "http://127.0.0.1:8787",
    signedIn: false,
    deviceId: null,
    sharing: false,
  });
  const [me, setMe] = useState<Me | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [problem, setProblem] = useState("");
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
    "login" | "register" | "verify" | "resend" | "recover" | "reset"
  >("login");
  const refreshVersion = useRef(0);
  const nameMigrationAttempts = useRef(new Set<string>());

  const refresh = useCallback(async () => {
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
      setConnectionInvite(null);
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
    } catch (e) {
      if (version === refreshVersion.current) setProblem(errorText(e));
    }
  }, []);
  useEffect(() => {
    void refresh().catch((e) => setProblem(errorText(e)));
  }, [refresh]);
  useEffect(() => {
    if (!me) return;
    const timer = window.setInterval(() => {
      void refresh();
    }, 15000);
    return () => window.clearInterval(timer);
  }, [me?.id, refresh]);
  const act = async (
    work: () => Promise<unknown>,
    message: string,
    reload = true,
  ) => {
    setBusy(true);
    setProblem("");
    setNotice("");
    try {
      await work();
      setNotice(message);
      if (reload) await refresh();
    } catch (e) {
      // Failed mutations may still stop sharing or clear local credentials.
      if (reload) await refresh().catch(() => {});
      setProblem(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const resetConnection = () => {
    setTransportReady(false);
  };
  const prepareTransport = async () => {
    if (transportReady) return;
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
          <div className="window-drag" data-tauri-drag-region>
            <span data-tauri-drag-region>
              FarSail{" "}
              <span className="window-subtitle" data-tauri-drag-region>
                遥舟
              </span>
            </span>
          </div>
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
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="M1 6h10" />
              </svg>
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
              <svg width="12" height="12" viewBox="0 0 12 12">
                <rect x="1.5" y="1.5" width="9" height="9" />
              </svg>
            </button>
            <button
              className="window-close"
              aria-label="关闭窗口"
              title="关闭"
              onClick={() =>
                void getCurrentWindow()
                  .close()
                  .catch((e) => setProblem(errorText(e)))
              }
            >
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="m2 2 8 8M10 2l-8 8" />
              </svg>
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
              ["overview", "总览", "◫"],
              ["devices", "我的设备", "▣"],
              ["requests", "远程连接", "⇄"],
              ["security", "账号安全", "◇"],
              ...(me?.role === "admin" ? [["admin", "管理控制台", "⚙"]] : []),
              ["settings", "共享与设置", "☷"],
            ] as [Tab, string, string][]
          ).map(([id, label, icon]) => (
            <button
              key={id}
              className={tab === id ? "active" : ""}
              aria-label={label}
              title={label}
              onClick={() => setTab(id)}
            >
              <span>{icon}</span>
              {label}
            </button>
          ))}
        </nav>
        <div
          className="sidebar-foot"
          role="status"
          aria-label={publicState.sharing ? "本机屏幕共享中" : "本机未共享"}
          title={publicState.sharing ? "本机屏幕共享中" : "本机未共享"}
        >
          <div
            className={`status-dot ${publicState.sharing ? "is-sharing" : ""}`}
          />
          {publicState.sharing ? "本机屏幕共享中" : "本机未共享"}
          <br />
          <small>远程桌面预览版</small>
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
                    overview: "设备控制台",
                    devices: "我的设备",
                    requests: "远程连接",
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
        </header>
        {problem && (
          <div className="alert error" role="alert">
            <ErrorMessage error={problem} />
            <button onClick={() => setProblem("")} aria-label="关闭错误">
              ×
            </button>
          </div>
        )}
        {notice && (
          <div className="alert success" role="status">
            <span>{notice}</span>
            <button onClick={() => setNotice("")} aria-label="关闭提示">
              ×
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
            />
          ) : tab === "settings" ? (
            <section className="stack">
              <div className="card">
                <div className="section-heading">
                  <div>
                    <div className="eyebrow">CONNECTION</div>
                    <h2>服务器连接</h2>
                  </div>
                </div>
                <p className="muted">
                  填写管理员提供的服务器地址。通过互联网连接时，请使用 HTTPS
                  地址。
                </p>
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
                    value={server}
                    onChange={(e) => setServer(e.target.value)}
                  />
                  <button className="primary" disabled={busy || !!me}>
                    保存地址
                  </button>
                </form>
                {me && <p className="hint">请先退出登录再修改服务地址。</p>}
              </div>
              <div className="card">
                <h2>当前版本支持</h2>
                <p className="muted">
                  Windows
                  电脑可共享屏幕，也可在批准后允许对方操作鼠标键盘。暂不支持文件传输、系统权限确认界面和未登录的桌面。
                </p>
                <div className="tag-row">
                  <span className="tag ready">账号与设备</span>
                  <span className="tag ready">屏幕查看与控制</span>
                  <span className="tag">文件传输 · 暂不支持</span>
                </div>
              </div>
              <div className="card">
                <h2>让另一台设备连接这台电脑</h2>
                <ol className="setup-guide">
                  <li>
                    <strong>添加本机</strong>：先在“总览”将这台电脑添加到账号。
                  </li>
                  <li>
                    <strong>开启本机共享</strong>
                    ：点击下方按钮，应用会自动准备连接。
                  </li>
                  <li>
                    <strong>批准对方</strong>
                    ：收到请求后，在“待我批准”选择允许查看或控制。
                  </li>
                </ol>
                <p className="muted">
                  停止共享会断开正在访问本机的连接。你连接其他电脑的操作不受影响。
                </p>
                <div className="row-actions">
                  <button
                    className={publicState.sharing ? "secondary" : "primary"}
                    disabled={busy || !publicState.deviceId}
                    onClick={() =>
                      void act(
                        async () => {
                          if (!publicState.sharing) await prepareTransport();
                          await invoke(
                            publicState.sharing
                              ? "share_disable"
                              : "share_enable",
                          );
                        },
                        publicState.sharing
                          ? "已停止本机共享，对方不能继续查看或控制本机"
                          : "本机共享已开启，默认收到连接请求后需要批准",
                      )
                    }
                  >
                    {publicState.sharing ? "停止本机共享" : "开启本机共享"}
                  </button>
                  {publicState.sharing && (
                    <button
                      className="secondary"
                      onClick={() => setTab("requests")}
                    >
                      管理远程连接
                    </button>
                  )}
                </div>
                {!publicState.deviceId && (
                  <p className="hint">
                    {me
                      ? "请先添加这台电脑。"
                      : "请先登录账号，再添加这台电脑。"}
                    <button
                      className="text-button"
                      onClick={() => setTab("overview")}
                    >
                      {me ? "去添加本机" : "去登录"}
                    </button>
                  </p>
                )}
                <details className="advanced-settings connection-settings">
                  <summary>高级连接设置（通常无需修改）</summary>
                  <p className="muted">
                    默认中继地址使用服务器的 8443
                    端口。只有管理员提供了其他参数时才需修改。应用设置会断开本机当前连接并停止共享，之后可重新开启。
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
                      }, "连接设置已应用。如需让他人连接本机，请重新开启共享");
                    }}
                  >
                    <label>
                      中继服务器地址
                      <input
                        aria-label="中继地址"
                        placeholder="https://服务器IP:8443/"
                        value={relayUrl}
                        onChange={(e) => setRelayUrl(e.target.value)}
                      />
                    </label>
                    <label>
                      本机 UDP 监听地址
                      <input
                        aria-label="UDP 绑定地址"
                        value={bindAddr}
                        onChange={(e) => setBindAddr(e.target.value)}
                      />
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        checked={forceRelay}
                        onChange={(e) => setForceRelay(e.target.checked)}
                      />{" "}
                      始终通过中继连接（排查直连问题时使用）
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
              onView={async (id) => {
                await invoke("viewer_open", { id });
              }}
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
              onView={async (id) => {
                await invoke("viewer_open", { id });
              }}
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
) => Promise<void>;
function Auth({
  mode,
  setMode,
  busy,
  act,
  onLogin,
}: {
  mode: "login" | "register" | "verify" | "resend" | "recover" | "reset";
  setMode: (
    m: "login" | "register" | "verify" | "resend" | "recover" | "reset",
  ) => void;
  busy: boolean;
  act: Action;
  onLogin: (x: Record<string, unknown>) => Promise<void>;
}) {
  const loginErrorDialog = useRef<HTMLDialogElement>(null);
  const [loginError, setLoginError] = useState("");
  const [email, setEmail] = useState(""),
    [password, setPassword] = useState(""),
    [newPassword, setNewPassword] = useState(""),
    [token, setToken] = useState(""),
    [invite, setInvite] = useState("");
  const titles = {
    login: "登录 FarSail",
    register: "创建账号",
    verify: "验证邮箱",
    resend: "重发验证邮件",
    recover: "找回密码",
    reset: "设置新密码",
  };
  const submit = async () => {
    if (mode === "login")
      return act(async () => {
        try {
          await onLogin({ email, password });
        } catch (e) {
          const raw = errorText(e);
          const message = /HTTP 401\b/.test(raw)
            ? "无法登录：邮箱或密码不正确，或账号尚未验证、已被停用。新注册账号请先完成邮箱验证。"
            : /HTTP 429\b/.test(raw)
              ? "登录尝试过于频繁，请稍后再试。"
              : "暂时无法登录，请检查网络和设置中的服务器地址后重试。";
          setLoginError(message);
          loginErrorDialog.current?.showModal();
          throw new Error(message);
        }
      }, "欢迎回来");
    if (mode === "register")
      return act(async () => {
        await api("register", {
          email,
          password,
          invite_code: invite || undefined,
        });
        setToken("");
        setMode("verify");
      }, "账号已创建，请输入邮件中的验证码完成注册");
    if (mode === "verify")
      return act(async () => {
        await api("verify", { token: token.trim() });
        setToken("");
        setPassword("");
        setMode("login");
      }, "邮箱验证成功，请登录");
    if (mode === "resend")
      return act(async () => {
        await api("resend", { email, password });
        setToken("");
        setMode("verify");
      }, "验证邮件已重新发送");
    if (mode === "recover")
      return act(async () => {
        await api("recover_request", { email });
        setToken("");
        setMode("reset");
      }, "如果该邮箱可以找回密码，重置邮件将发送至邮箱，请输入邮件中的重置码");
    return act(async () => {
      await api("recover_complete", {
        token: token.trim(),
        password: newPassword,
      });
      setToken("");
      setNewPassword("");
      setPassword("");
      setMode("login");
    }, "密码已更新，请使用新密码登录");
  };
  return (
    <div className="auth-wrap">
      <dialog
        ref={loginErrorDialog}
        className="login-error-dialog"
        aria-labelledby="login-error-title"
        aria-describedby="login-error-description"
      >
        <h2 id="login-error-title">登录未成功</h2>
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
      </dialog>
      <section className="card auth-card">
        <div className="eyebrow">SECURE ACCESS</div>
        <h2>{titles[mode]}</h2>
        <p className="muted">
          {mode === "verify"
            ? `最后一步：输入${email ? `发送至 ${email} 的邮件` : "验证邮件"}中的完整验证码。`
            : mode === "reset"
              ? "输入重置邮件中的完整重置码，再设置至少 12 个字符的新密码。"
              : mode === "recover"
                ? "填写注册邮箱，我们会向符合条件的账号发送密码重置邮件。"
                : mode === "register"
                  ? "使用邮箱注册，密码至少需要 12 个字符。"
                  : mode === "resend"
                    ? "填写注册邮箱和密码，重新获取验证邮件。"
                    : "登录后即可添加本机、连接电脑或开启共享。"}
        </p>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
          className="form"
        >
          {["login", "register", "resend", "recover"].includes(mode) && (
            <label>
              邮箱地址
              <input
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
                autoComplete="email"
              />
            </label>
          )}
          {["login", "register", "resend"].includes(mode) && (
            <label>
              密码
              <input
                type="password"
                required
                minLength={mode === "register" ? 12 : 1}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete={
                  mode === "register" ? "new-password" : "current-password"
                }
              />
            </label>
          )}
          {["verify", "reset"].includes(mode) && (
            <label>
              {mode === "verify" ? "邮箱验证码" : "邮件中的重置码"}
              <input
                required
                value={token}
                onChange={(e) => setToken(e.target.value)}
                autoComplete="off"
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
              />
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
                ? "注册并验证邮箱"
                : mode === "verify"
                  ? "完成验证"
                  : mode === "login"
                    ? "登录"
                    : mode === "resend"
                      ? "重新发送验证邮件"
                      : mode === "recover"
                        ? "发送密码重置邮件"
                        : "保存新密码并返回登录"}
          </button>
        </form>
        <div className="auth-links">
          {(
            {
              login: ["register", "recover", "verify"],
              register: ["login"],
              verify: ["resend", "login"],
              resend: ["verify", "login"],
              recover: ["reset", "login"],
              reset: ["recover", "login"],
            }[mode] as (typeof mode)[]
          ).map((x) => (
            <button key={x} disabled={busy} onClick={() => setMode(x)}>
              {x === "reset"
                ? "已有重置码"
                : x === "resend"
                  ? "没收到验证邮件"
                  : x === "recover" && mode === "reset"
                    ? "重新发送重置邮件"
                    : titles[x]}
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function Overview({
  me,
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
  onRefresh: () => Promise<void>;
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
      <div className="hero">
        <div>
          <div className="eyebrow">YOUR DESK, YOUR RULES</div>
          <h2>连接你的电脑</h2>
          <p>你好，{me.email}。这里汇总你账号下的设备和等待处理的请求。</p>
          <div className="hero-actions">
            <button className="primary" onClick={() => onNavigate("devices")}>
              查看所有设备 →
            </button>
            <button className="ghost" onClick={() => void onRefresh()}>
              刷新状态
            </button>
          </div>
        </div>
        <div className="hero-mark">◈</div>
      </div>
      <div className="metrics">
        <div className="metric">
          <span>账号设备</span>
          <strong>{devices.length}</strong>
          <small>包括离线与停用</small>
        </div>
        <div className="metric">
          <span>在线设备</span>
          <strong>{devices.filter((x) => x.online).length}</strong>
          <small>当前保持连接的设备</small>
        </div>
        <div className="metric">
          <span>待处理请求</span>
          <strong>{pending.length}</strong>
          <small>需要本机设备批准</small>
        </div>
      </div>
      {!state.deviceId ? (
        <div className="card attention">
          <div>
            <div className="eyebrow">DEVICE IDENTITY</div>
            <h2>添加这台电脑</h2>
            <p>
              添加后可连接你的其他电脑。如果希望他人连接本机，还需要开启本机共享。
            </p>
          </div>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                () => api("bind", { name: name.trim() }),
                "这台电脑已添加，可前往“我的设备”开始连接",
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
              添加这台电脑
            </button>
          </form>
          <p className="muted">
            {state.computerName
              ? "已读取系统的计算机名称，你也可以自行修改。"
              : "暂时无法读取计算机名称，请手动填写后添加。"}
          </p>
        </div>
      ) : (
        <div className="card local-device">
          <div>
            <div className="eyebrow">THIS DEVICE</div>
            <h2>{local?.name ?? "本机设备"}</h2>
            <p className="muted">
              设备 ID {short(state.deviceId)} ·{" "}
              {local?.online ? "在线" : "正在更新在线状态"}
            </p>
          </div>
          <div className="stack">
            <span className={state.sharing ? "tag ready" : "tag"}>
              {state.sharing ? "本机共享已开启" : "本机共享未开启"}
            </span>
            <button className="primary" onClick={() => onNavigate("settings")}>
              {state.sharing ? "管理本机共享" : "设置本机共享"}
            </button>
            <small className="muted">
              默认每次连接需批准；本机可单独开启同账号远程值守。
            </small>
          </div>
        </div>
      )}
      <div className="card overview-history">
        <div className="section-heading">
          <div>
            <div className="eyebrow">ACTIVITY</div>
            <h2>最近的远程连接</h2>
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
            <Empty text="还没有远程连接。前往“我的设备”，选择电脑开始连接。" />
          )}
        </div>
      </div>
    </div>
  );
}
function Empty({ text }: { text: string }) {
  return (
    <div className="empty">
      <span>◎</span>
      <p>{text}</p>
    </div>
  );
}
function Devices({
  requests,
  pending,
  prepareTransport,
  onConnectionReset,
  onView,
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
  onView: (id: string) => Promise<void>;
  onRefresh: () => Promise<void>;
  onNavigate: (tab: Tab) => void;
  devices: Device[];
  state: PublicState;
  busy: boolean;
  act: Action;
}) {
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [waiting, setWaiting] = useState<{ id: string; target: string } | null>(
    null,
  );
  const [connectionNote, setConnectionNote] = useState("");
  const [connecting, setConnecting] = useState(false);
  const inFlight = useRef(false);
  const [editing, setEditing] = useState(false);
  const [newName, setNewName] = useState("");
  const [confirmUnbind, setConfirmUnbind] = useState(false);
  const shown = devices
    .filter(
      (d) =>
        (d.name.toLowerCase().includes(search.toLowerCase()) ||
          d.id.includes(search)) &&
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
    setConfirmUnbind(false);
  }, [device?.id]);
  useEffect(() => {
    if (!waiting) return;
    const timer = window.setInterval(() => void onRefresh(), 2000);
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
            await onView(request.id);
            setConnectionNote("已在独立窗口打开，可继续管理设备。");
          } catch (e) {
            setConnectionNote("连接失败，请检查双方网络和共享状态后重试。");
            throw e;
          } finally {
            inFlight.current = false;
            setConnecting(false);
          }
        },
        "已打开独立远程窗口",
        false,
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
  const connect = (permission: "control" | "view") => {
    if (!device || busy || inFlight.current || waiting) return;
    const target = device.id;
    inFlight.current = true;
    setConnecting(true);
    setConnectionNote("正在发送连接请求…");
    void act(async () => {
      try {
        await prepareTransport();
        const existing = requests.find(
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
          "等待对方批准，批准后会在此页面打开画面；切换页面后可到“远程连接”查看进度。",
        );
      } catch (e) {
        setConnectionNote("未能发起连接，请检查服务器设置和设备状态。");
        throw e;
      } finally {
        inFlight.current = false;
        setConnecting(false);
      }
    }, "已发送连接请求");
  };
  const blocked = !state.deviceId
    ? "先在“总览”添加这台电脑，才能发起连接。"
    : !device?.enabled
      ? "这台设备已停用。"
      : !device?.online
        ? "设备离线，请在对方电脑上打开 FarSail 并登录。"
        : !device?.can_host
          ? "对方尚未开启共享，请在对方电脑的本机面板中开启。"
          : "对方批准后，即可进入远程画面。";
  return (
    <section className="device-workspace">
      <div className="device-list-pane">
        <div className="section-heading">
          <h2>
            我的设备 <span className="count">{devices.length}</span>
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
                ▣
              </span>
              <span className="device-list-name">
                <strong>{d.name}</strong>
                <small>
                  {d.id === state.deviceId ? "本机 · " : ""}
                  {!d.enabled ? "已停用" : d.online ? "在线" : "离线"} ·{" "}
                  {short(d.id)}
                </small>
              </span>
              <span aria-hidden="true">›</span>
            </button>
          ))}
          {!shown.length && (
            <Empty
              text={
                devices.length
                  ? "没有匹配的设备"
                  : "还没有设备，请先添加这台电脑。"
              }
            />
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
                <div className="sharing-panel">
                  <div>
                    <h3>允许其他设备连接本机</h3>
                    <p className="muted">
                      默认每次连接需要你批准；启用下方“远程值守”后，同账号设备可自动批准。停止共享会断开正在访问本机的连接。
                    </p>
                  </div>
                  <button
                    className={state.sharing ? "secondary" : "primary"}
                    disabled={busy || !device.enabled}
                    onClick={() =>
                      void act(
                        async () => {
                          if (!state.sharing) await prepareTransport();
                          await invoke(
                            state.sharing ? "share_disable" : "share_enable",
                          );
                        },
                        state.sharing
                          ? "已停止本机共享"
                          : "本机共享已开启，等待连接请求",
                      )
                    }
                  >
                    {state.sharing ? "停止本机共享" : "开启本机共享"}
                  </button>
                </div>
                <p className="hint">
                  {state.sharing
                    ? "本机正在等待连接；你可以随时停止共享。"
                    : "共享已关闭，其他设备不能查看或控制本机。"}
                </p>
                <div className="sharing-panel">
                  <div>
                    <h3>远程值守</h3>
                    <p className="muted">
                      开启后，同账号设备可直接连接，无需逐次批准。
                    </p>
                    <p className="hint">
                      仅本次共享有效。关闭后会断开正在访问本机的连接，恢复逐次批准；需要
                      Windows 已登录且应用保持运行。
                    </p>
                  </div>
                  <button
                    disabled={busy || !state.sharing}
                    className={state.remoteWatch ? "secondary" : "primary"}
                    onClick={() =>
                      void act(
                        () =>
                          invoke("remote_watch", {
                            enabled: !state.remoteWatch,
                          }),
                        state.remoteWatch
                          ? "远程值守已关闭，恢复手动批准"
                          : "远程值守已开启",
                      )
                    }
                  >
                    {state.remoteWatch ? "关闭远程值守" : "开启远程值守"}
                  </button>
                </div>
                <h3>
                  等待批准的连接 <span className="count">{pending.length}</span>
                </h3>
                {pending.map((p) => (
                  <div className="approval-card" key={p.id}>
                    <strong>
                      {p.source_device_name} 请求
                      {p.permission === "control" ? "控制电脑" : "查看屏幕"}
                    </strong>
                    <p className="muted">{p.requester_email}</p>
                    <p className="muted">
                      {p.permission === "control"
                        ? "允许后对方能看到屏幕并操作鼠标键盘。"
                        : "允许后对方只能看到屏幕。"}
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
                        disabled={busy || !state.sharing}
                        onClick={() =>
                          void act(
                            () => api("decide", { id: p.id, approve: true }),
                            "已允许连接",
                          )
                        }
                      >
                        {p.permission === "control"
                          ? "允许查看和控制"
                          : "允许查看"}
                      </button>
                    </div>
                  </div>
                ))}
                {!pending.length && <p className="muted">暂无连接请求。</p>}
                <button
                  className="text-button"
                  onClick={() => onNavigate("settings")}
                >
                  服务器与高级连接设置 →
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
                    <span>▣</span>远程控制<small>查看屏幕并操作鼠标键盘</small>
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
                    仅查看屏幕
                  </button>
                </div>
                <p className="muted">此版本暂不支持文件传输。</p>
                <p className="muted">{blocked}</p>
                {(waiting || connecting || connectionNote) && (
                  <div className="notice-strip" role="status">
                    {connectionNote}
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
                    保存名称
                  </button>
                  <button
                    className="secondary"
                    type="button"
                    disabled={busy}
                    onClick={() => setEditing(false)}
                  >
                    取消改名
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
                  修改设备名称
                </button>
              )}
              {confirmUnbind ? (
                <div className="notice-strip">
                  <p>
                    从账号移除“{device.name}
                    ”后，与此设备相关的连接将结束。以后使用需要在该电脑重新添加。本机当前连接和共享也会停止。
                  </p>
                  <button
                    className="danger-text"
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        onConnectionReset();
                        await api("unbind_device", { id: device.id });
                        setConfirmUnbind(false);
                      }, "设备已从账号移除，需要使用时可在该电脑重新添加")
                    }
                  >
                    确认移除设备
                  </button>
                  <button
                    className="text-button"
                    onClick={() => setConfirmUnbind(false)}
                  >
                    保留设备
                  </button>
                </div>
              ) : (
                <button
                  className="danger-text"
                  onClick={() => setConfirmUnbind(true)}
                >
                  从账号移除设备
                </button>
              )}
            </details>
          </>
        ) : (
          <Empty text="选择一台设备，查看连接操作。" />
        )}
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
}: {
  devices: Device[];
  requests: Remote[];
  pending: Pending[];
  state: PublicState;
  busy: boolean;
  act: Action;
  transportReady: boolean;
  prepareTransport: () => Promise<void>;
  onRefresh: () => Promise<void>;
  invitation: ConnectionInvitation | null;
  setInvitation: React.Dispatch<
    React.SetStateAction<ConnectionInvitation | null>
  >;
  onNavigate: (tab: Tab) => void;
  onView: (id: string) => Promise<void>;
}) {
  const [paths, setPaths] = useState<
    Record<
      string,
      {
        state: string;
        rtt_ms: number | null;
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
          verification_code?: string | null;
          error?: string | null;
        }>("remote_status", { id: r.id })
          .catch(() =>
            invoke<{ state: string; rtt_ms: number | null }>(
              "transport_status",
              { id: r.id },
            ),
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
    [inviteTarget, setInviteTarget] = useState(invitation?.target ?? ""),
    [permission, setPermission] = useState<"view" | "control" | "files">(
      "view",
    ),
    [code, setCode] = useState("");
  const [section, setSection] = useState(
    pending.length ? "incoming" : "active",
  );
  const [invitePermission, setInvitePermission] = useState<"view" | "control">(
    invitation?.permission ?? "view",
  );
  const invite = invitation?.code ?? "";
  const inviteId = invitation?.id ?? "";
  const generatedInvite = invitation;
  const inviteVisible = invitation?.visible ?? true;
  const pendingCount = requests.filter((r) => r.state === "pending").length;
  useEffect(() => {
    if (!pendingCount) return;
    const timer = window.setInterval(
      () => void onRefresh().catch(() => {}),
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
  return (
    <div className="connections-workspace">
      <div className="section-heading connection-intro">
        <p className="muted">
          管理远程电脑的连接。对方允许后，才能查看或控制屏幕。
        </p>
        <div className="row-actions">
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
        </div>
      </div>
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
          <h2>连接他人电脑</h2>
          <p className="muted">
            请对方开启共享，并把设备 ID
            和一次性邀请码发给你。自己的电脑可直接从“我的设备”连接。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(async () => {
                await prepareTransport();
                await api("request", {
                  source_device_id: state.deviceId,
                  target_device_id: target.trim(),
                  permission,
                  invitation_code: code.trim() || undefined,
                });
                setSection("active");
              }, "连接请求已发送，请等待对方批准；可在“当前连接”查看进度");
            }}
          >
            <label>
              目标设备
              <select
                value={devices.some((x) => x.id === target) ? target : "custom"}
                onChange={(e) =>
                  setTarget(e.target.value === "custom" ? "" : e.target.value)
                }
              >
                <option value="custom">输入对方提供的设备 ID</option>
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
              onChange={(e) => setTarget(e.target.value)}
              required
            />
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
                <option value="control">查看屏幕并操作鼠标键盘</option>
                <option value="files" disabled>
                  文件（待实现）
                </option>
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
            <button className="primary" disabled={busy || !state.deviceId}>
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
          <h2>邀请他人连接我的电脑</h2>
          <p className="muted">
            选择已开启共享的电脑，生成一次性邀请码。对方发起连接后，你仍需在那台电脑上批准。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
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
                onChange={(e) => setInviteTarget(e.target.value)}
                required
              >
                <option value="">选择设备</option>
                {devices
                  .filter((x) => x.can_host && x.enabled && x.online)
                  .map((d) => (
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
                  setInvitePermission(e.target.value as "view" | "control")
                }
              >
                <option value="view">仅查看屏幕</option>
                <option value="control">查看屏幕并操作鼠标键盘</option>
              </select>
            </label>
            <button
              disabled={
                busy ||
                !!invite ||
                !devices.some(
                  (d) =>
                    d.id === inviteTarget &&
                    d.online &&
                    d.enabled &&
                    d.can_host,
                )
              }
              className="secondary"
            >
              生成邀请码
            </button>
          </form>
          {!devices.some((d) => d.online && d.enabled && d.can_host) && (
            <p className="hint">
              还没有开启共享的在线电脑。请先在那台电脑开启本机共享。
            </p>
          )}
          {invite && (
            <p className="muted">
              如需更换设备或权限，请先取消当前邀请，再生成新的邀请码。
            </p>
          )}
          {invite && generatedInvite && (
            <div className="invite-code">
              <span>
                {deviceLabel(generatedInvite.target, devices, state.deviceId)} ·
                将设备 ID 和邀请码一起发给对方
              </span>
              <code>{generatedInvite.target}</code>
              <span>允许：{permissionLabel(generatedInvite.permission)}</span>
              <span>一次性邀请码</span>
              <code>
                {inviteVisible ? invite : "邀请码已收起，收起不会取消邀请"}
              </code>
              <div className="row-actions">
                <button
                  disabled={busy}
                  onClick={() =>
                    void act(
                      () =>
                        copyText(
                          `设备 ID：${generatedInvite.target}\n允许：${permissionLabel(generatedInvite.permission)}\n一次性邀请码：${invite}`,
                        ),
                      "设备 ID 和邀请码已复制，请发给需要连接的人",
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
                    }, "邀请已取消，此邀请码和由它建立的连接均不能继续使用")
                  }
                >
                  取消邀请
                </button>
              </div>
              <span>取消邀请也会结束通过此邀请建立的连接。</span>
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
              <h2>谁想连接这台电脑</h2>
            </div>
            <span className="count">{pending.length}</span>
          </div>
          <p className="muted">
            只批准你认识的连接。允许控制后，对方能看到屏幕并操作鼠标键盘。
          </p>
          {!state.sharing && pending.length > 0 && (
            <p className="hint">
              本机共享尚未开启。
              <button
                className="text-button"
                onClick={() => onNavigate("settings")}
              >
                去开启共享
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
                      disabled={busy || !state.sharing}
                      className="primary"
                      onClick={() =>
                        void act(
                          () => api("decide", { id: p.id, approve: true }),
                          "已允许连接，对方现在可以连接这台电脑",
                        )
                      }
                    >
                      {p.permission === "control"
                        ? "允许查看和控制"
                        : "允许查看"}
                    </button>
                  </div>
                </div>
              ))
            ) : (
              <Empty text="目前没有等待本机处理的请求。" />
            )}
          </div>
        </div>
        <div
          className="card connection-list-pane"
          hidden={!["active", "history"].includes(section)}
        >
          <div className="eyebrow">HISTORY</div>
          <h2>{section === "history" ? "过去的连接" : "当前连接"}</h2>
          <p className="muted">
            {section === "history"
              ? "查看已取消、被拒绝或已过期的连接请求。"
              : "已批准表示对方已允许访问；点击连接按钮后才会打开画面。结束本次连接后，需要重新申请并获得批准。"}
          </p>
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
                        连接状态：{pathLabel(paths[r.id]?.state)}
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
                                await invoke("transport_connect", {
                                  id: r.id,
                                  permission: r.permission,
                                });
                                await onView(r.id);
                              },
                              "已打开独立远程窗口",
                              false,
                            )
                          }
                        >
                          {r.permission === "control"
                            ? "连接并控制"
                            : "连接并查看"}
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
                              : "本次连接已结束，再次连接需要重新申请并获得批准",
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
                text={
                  section === "history"
                    ? "还没有过去的连接记录。"
                    : "目前没有连接。点击“从我的设备连接”开始。"
                }
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
type RemoteDisplay = {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  dpi: number;
  rotation: number;
};
type RemoteStatus = {
  video?: {
    profile: number;
    generation: number;
    supported: boolean;
    max_profile?: number;
  };
  input?: { generation: number; blocked: boolean; message: string | null };
  retryable?: boolean;
  state: string;
  rtt_ms: number | null;
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
  return (
    <Viewer
      key={id}
      id={id}
      onReconnect={setId}
      onStop={() => void invoke("viewer_window_action", { action: "close" })}
    />
  );
}
function Viewer({
  id,
  onStop,
  onReconnect,
}: {
  id: string;
  onStop: () => void;
  onReconnect: (id: string) => void;
}) {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [picture, setPicture] = useState<Picture | null>(null);
  const [fps, setFps] = useState(0);
  const [text, setText] = useState("");
  const [problem, setProblem] = useState("");
  const image = useRef<HTMLImageElement>(null);
  const last = useRef(0);
  const frameTimes = useRef<number[]>([]);
  const url = useRef<string | null>(null);
  const inputQueue = useRef<Promise<unknown>>(Promise.resolve());
  const inputPending = useRef(0);
  const lastMove = useRef(0);
  const [ended, setEnded] = useState(false);
  const [reconnecting, setReconnecting] = useState(false);
  const [retryingInput, setRetryingInput] = useState(false);
  const [toolbarVisible, setToolbarVisible] = useState(true);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [toolbarPinned, setToolbarPinned] = useState(false);
  const toolbarHovered = useRef(false);
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
  const showToolbar = toolbarVisible || toolsOpen || toolbarPinned;
  const scheduleHide = () => {
    window.clearTimeout(toolbarTimer.current);
    if (!toolbarPinned && !toolsOpen && !toolbarHovered.current)
      toolbarTimer.current = window.setTimeout(
        () => setToolbarVisible(false),
        2200,
      );
  };
  useEffect(() => {
    if (toolbarVisible) scheduleHide();
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
    (d) => d.id === (picture?.display ?? 1),
  );
  const sourceEdge = selectedDisplay
    ? Math.max(selectedDisplay.width, selectedDisplay.height)
    : 0;
  useEffect(() => {
    if (
      !autoQuality ||
      ended ||
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
    status?.state !== "closed" &&
    status?.permission === "control";
  const controlRef = useRef(control);
  controlRef.current = control;
  const send = (input: Record<string, unknown> | null) => {
    if (!controlRef.current) return;
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
    inputPending.current++;
    inputQueue.current = inputQueue.current
      .then(() => {
        if (epoch === generation.current && controlRef.current)
          return invoke("remote_input", { id, input });
      })
      .catch((e) => {
        generation.current++;
        controlRef.current = false;
        setEnded(true);
        setPicture(null);
        setProblem(errorText(e));
        void invoke("transport_close", { id }).catch(() => {});
      })
      .finally(() => {
        inputPending.current--;
      });
  };
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
  useEffect(() => {
    let live = true;
    last.current = 0;
    let receiving = true;
    let terminal = false;
    const finish = (s?: RemoteStatus, error?: unknown) => {
      if (!live || terminal) return;
      terminal = true;
      receiving = false;
      generation.current++;
      controlRef.current = false;
      setEnded(true);
      setPicture(null);
      setFps(0);
      frameTimes.current = [];
      if (url.current) {
        URL.revokeObjectURL(url.current);
        url.current = null;
      }
      setProblem(
        s?.error || (error ? errorText(error) : "会话已结束，画面与输入已停止"),
      );
      if (s?.retryable) {
        setReconnecting(true);
        void invoke<{ id: string }>("viewer_reconnect")
          .then((r) => {
            if (live) onReconnect(r.id);
          })
          .catch((e) => {
            if (live) {
              setProblem(errorText(e));
              setReconnecting(false);
            }
          });
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
            if (s.input) inputGeneration.current = s.input.generation;
            if (s.input?.blocked && !inputBlockedRef.current) {
              generation.current++;
              controlRef.current = false;
              setText("");
              setToolbarVisible(true);
            }
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
          const next = URL.createObjectURL(
            new Blob([data.slice(49)], { type: "image/jpeg" }),
          );
          const old = url.current;
          url.current = next;
          last.current = sequence;
          frameTimes.current.push(performance.now());
          if (frameTimes.current.length > 240) frameTimes.current.shift();
          setPicture({
            url: next,
            display: view.getUint32(5),
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
    const release = () => {
      generation.current++;
      send(null);
    };
    window.addEventListener("blur", release);
    return () => {
      live = false;
      window.clearInterval(statusTimer);
      window.clearInterval(fpsTimer);
      window.removeEventListener("blur", release);
      generation.current++;
      controlRef.current = false;
      if (url.current) URL.revokeObjectURL(url.current);
    };
  }, [id]);
  const point = (e: { clientX: number; clientY: number }) => {
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
    return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
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
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="M1 6h10" />
              </svg>
            </button>
            <button
              aria-label="最大化或还原"
              title="最大化 / 还原"
              onClick={() => windowAction("maximize")}
            >
              <svg width="12" height="12" viewBox="0 0 12 12">
                <rect x="1.5" y="1.5" width="9" height="9" />
              </svg>
            </button>
            <button
              className="window-close"
              aria-label="关闭窗口"
              title="结束并关闭"
              onClick={onStop}
            >
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="m2 2 8 8M10 2l-8 8" />
              </svg>
            </button>
          </div>
        </header>
      )}
      {!showToolbar && (
        <button
          className="viewer-reveal"
          onMouseEnter={() => setToolbarVisible(true)}
          onClick={() => setToolbarVisible(true)}
        >
          显示工具栏
        </button>
      )}
      <div className="viewer-body">
        <header
          className="viewer-toolbar"
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
              {ended
                ? "会话已结束"
                : inputBlocked
                  ? "控制已暂停"
                  : status?.permission === "control"
                    ? "远程控制"
                    : "仅查看"}
            </strong>
            <span className="muted">
              {pathLabel(status?.state)} · {status?.rtt_ms ?? "—"} 毫秒 ·{" "}
              {fps > 0 ? `${fps} 帧/秒` : picture ? "画面暂未更新" : "等待画面"}
            </span>
          </div>
          <label className="viewer-display-selector">
            <span className="sr-only">对方的显示器</span>
            <select
              aria-label="对方的显示器"
              disabled={ended || !status?.displays.length}
              value={picture?.display ?? 1}
              onChange={(e) =>
                void invoke("media_select", {
                  id,
                  display: Number(e.target.value),
                }).catch((err) => setProblem(errorText(err)))
              }
            >
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
                !status?.video?.supported
                  ? "被控端需更新后才能切换"
                  : "自动按窗口大小适配；高分辨率取决于对方显示器，当前尺寸见更多操作"
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
                          generation.current++;
                          send(null);
                          setFitMode(mode);
                        }}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                  <p className="hint">
                    {fitMode === "fill"
                      ? "完整画面铺满窗口；两端比例不同时会拉伸。"
                      : "保持远端原始比例；两端比例不同时会留边。"}
                  </p>
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
                    {pathLabel(status?.state)} · 网络往返{" "}
                    {status?.rtt_ms ?? "—"} 毫秒 ·{" "}
                    {fps > 0 ? `${fps} 帧/秒（近 4 秒）` : "等待画面更新"}
                    <br />
                    连接核对码：{status?.verification_code ?? "正在获取"}
                    。请与对方确认两端显示一致。
                  </p>
                </div>{" "}
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
                <p className="hint">
                  高清模式优先保留文字细节；复杂画面会在数据大小上限内调整压缩。系统权限确认和未登录桌面暂不支持控制。
                </p>
              </div>
            </details>
            <button onClick={() => windowAction("fullscreen")}>全屏切换</button>
            <button className="danger-text" onClick={onStop}>
              {reconnecting ? "取消重连并关闭" : "结束并关闭"}
            </button>
          </div>
        </header>
        {profilePending != null && (
          <div className="notice-strip" role="status">
            正在切换画面分辨率…
          </div>
        )}
        {reconnecting && (
          <div className="notice-strip" role="status">
            正在重连（最多 3 次）；等待新的授权批准…
          </div>
        )}
        {problem && (
          <div className="alert error">
            <ErrorMessage error={problem} />
          </div>
        )}
        {status?.error && (
          <div className="alert error">
            <ErrorMessage error={status.error} />
          </div>
        )}
        {inputBlocked && !ended && (
          <div className="notice-strip" role="alert">
            <p>
              {status?.input?.message ?? "鼠标键盘控制已暂停，画面连接仍保留。"}
            </p>
            <button
              className="secondary"
              disabled={retryingInput}
              onClick={() => {
                setRetryingInput(true);
                generation.current++;
                void invoke("remote_input", {
                  id,
                  input: { kind: "resume_control" },
                })
                  .catch((e) => setProblem(errorText(e)))
                  .finally(() => setRetryingInput(false));
              }}
            >
              {retryingInput ? "正在请求重试…" : "重试控制"}
            </button>
          </div>
        )}
        <div
          className="remote-screen"
          ref={screen}
          onBlur={() => {
            generation.current++;
            send(null);
          }}
          tabIndex={control ? 0 : -1}
          onKeyDown={(e) => {
            if (!control) return;
            e.preventDefault();
            send({ kind: "key", vk: keyVk(e), down: true, repeat: e.repeat });
          }}
          onKeyUp={(e) => {
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
              title={
                control
                  ? "远程控制：点击画面后可使用鼠标键盘"
                  : inputBlocked
                    ? "控制已暂停，请点击重试控制"
                    : "仅查看画面"
              }
              draggable={false}
              onMouseMove={(e) => {
                const p = point(e);
                if (p)
                  send({
                    kind: "move",
                    display: picture.display,
                    layout: picture.layout,
                    ...p,
                  });
              }}
              onMouseDown={(e) => {
                e.preventDefault();
                screen.current?.focus({ preventScroll: true });
                if (e.button !== 0 && e.button !== 2) return;
                const p = point(e);
                if (p)
                  send({
                    kind: "button",
                    display: picture.display,
                    layout: picture.layout,
                    ...p,
                    button: e.button === 2 ? "right" : "left",
                    down: true,
                  });
              }}
              onMouseUp={(e) => {
                if (e.button !== 0 && e.button !== 2) return;
                const p = point(e);
                if (p)
                  send({
                    kind: "button",
                    display: picture.display,
                    layout: picture.layout,
                    ...p,
                    button: e.button === 2 ? "right" : "left",
                    down: false,
                  });
              }}
              onMouseLeave={() => {
                generation.current++;
                send(null);
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
            <span>
              {ended
                ? "会话已结束，远端画面已清除"
                : "等待远端 JPEG 画面；点击画面后使用键盘"}
            </span>
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
    [sessions, setSessions] = useState<
      {
        id: string;
        created_at: string;
        last_used_at: string;
        current: boolean;
      }[]
    >([]);
  useEffect(() => {
    void api<typeof sessions>("sessions")
      .then(setSessions)
      .catch((e) => setLoadError(errorText(e)));
  }, [me.id]);
  return (
    <div className="stack">
      {loadError && (
        <div className="alert error" role="alert">
          <ErrorMessage error={loadError} />
        </div>
      )}
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">PASSWORD</div>
          <h2>修改密码</h2>
          <p className="muted">
            修改后，所有客户端都会退出登录，当前连接和共享也会停止。请使用新密码登录，并重新添加本机。
          </p>
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
              }, "密码已更改，请重新登录");
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
                value={newPassword}
                onChange={(e) => setNew(e.target.value)}
              />
              <small className="muted">至少 12 个字符</small>
            </label>
            <button className="primary" disabled={busy}>
              修改密码并退出所有登录
            </button>
          </form>
        </div>
        <div className="card">
          <div className="eyebrow">ACCOUNT</div>
          <h2>登录状态</h2>
          <p className="muted">{me.email}</p>
          <p className="muted">
            退出会停止本机共享和当前连接。下次使用时需要重新登录。
          </p>
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
            onClick={() =>
              void api<typeof sessions>("sessions")
                .then((rows) => {
                  setSessions(rows);
                  setLoadError("");
                })
                .catch((e) => setLoadError(errorText(e)))
            }
          >
            刷新
          </button>
        </div>
        <p className="muted">
          这里显示仍可使用此账号的客户端。发现不认识的登录时，可以将其退出；对方需要重新登录才能继续使用。
        </p>
        <div
          className="login-records"
          tabIndex={0}
          aria-label="账号登录记录列表"
        >
          {sessions.length ? (
            sessions.map((x) => (
              <div className="list-row" key={x.id}>
                <div>
                  <strong>
                    {x.id === me.session_id
                      ? "当前客户端（正在使用）"
                      : "其他客户端登录"}
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
                        await api("revoke_session", { id: x.id });
                        setSessions(await api("sessions"));
                      }, "已退出该客户端的登录")
                    }
                  >
                    退出此登录
                  </button>
                )}
              </div>
            ))
          ) : (
            <Empty text="没有可显示的登录记录。" />
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
    [registration, setRegistration] = useState("open"),
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
  const load = async () => {
    const [u, r, s, a] = await Promise.all([
      api<User[]>("admin_users"),
      api<{ value: string }>("admin_registration"),
      api<typeof sessions>("admin_sessions"),
      api<typeof audit>("admin_audit"),
    ]);
    setUsers(u);
    setHasMore(u.length === 100);
    setAfter(u.at(-1)?.id ?? "");
    setRegistration(r.value);
    setSessions(s);
    setAudit(a);
  };
  useEffect(() => {
    void load().catch((e) => setLoadError(errorText(e)));
  }, []);
  const loadMore = () =>
    void act(
      async () => {
        const u = await api<User[]>("admin_users", { after });
        setUsers((x) => [...x, ...u]);
        setAfter(u.at(-1)?.id ?? "");
        setHasMore(u.length === 100);
      },
      "已加载更多用户",
      false,
    );
  return (
    <div className="stack">
      {loadError && (
        <div className="alert error" role="alert">
          <ErrorMessage error={loadError} />
        </div>
      )}
      <div className="notice-strip">
        管理账号、设备和注册方式。即使是管理员，连接他人电脑也需要对方批准。
      </div>
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">REGISTRATION</div>
          <h2>允许谁注册账号</h2>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                () => api("admin_set_registration", { value: registration }),
                "注册策略已更新",
              );
            }}
          >
            <select
              aria-label="账号注册方式"
              value={registration}
              onChange={(e) => setRegistration(e.target.value)}
            >
              <option value="open">开放注册</option>
              <option value="invite_only">仅邀请</option>
              <option value="closed">关闭注册</option>
            </select>
            <button className="primary" disabled={busy}>
              保存注册方式
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
                "注册邀请码已生成，请复制后发给受邀人；不会自动发送邮件",
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
              生成注册邀请码
            </button>
          </form>
          {invite && (
            <div className="invite-code">
              <span>
                受邀邮箱：{invitedEmail} ·
                邀请信息仅保留到退出登录或关闭应用，请先复制。
              </span>
              <code>
                {inviteVisible ? invite : "邀请码已收起，收起不会取消邀请"}
              </code>
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
        <h2>停用或恢复设备</h2>
        <p className="muted">
          停用会结束此设备的连接并阻止再次连接；恢复后，设备所有者需要在该电脑重新添加设备。
        </p>
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
                "设备已禁用",
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
        <div className="section-heading">
          <div>
            <div className="eyebrow">USERS</div>
            <h2>用户管理</h2>
          </div>
          <button
            className="text-button"
            disabled={busy}
            onClick={() =>
              void act(
                async () => {
                  await load();
                  setLoadError("");
                },
                "管理数据已刷新",
                false,
              )
            }
          >
            刷新管理数据
          </button>
        </div>
        <p className="muted">
          停用账号会结束其登录和远程连接，恢复账号后用户可重新登录。
        </p>
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
              disabled={busy}
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
        {hasMore && (
          <button className="text-button" disabled={busy} onClick={loadMore}>
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
          ) : (
            <Empty text="暂无登录记录" />
          )}
        </div>
        <div className="card">
          <div className="eyebrow">AUDIT</div>
          <h2>管理与安全操作记录</h2>
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
          ) : (
            <Empty text="暂无审计记录" />
          )}
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
