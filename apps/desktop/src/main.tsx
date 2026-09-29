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
  server: string;
  signedIn: boolean;
  deviceId: string | null;
  sharing: boolean;
  remoteWatch?: boolean;
};
const api = <T,>(op: string, args: Record<string, unknown> = {}): Promise<T> =>
  invoke("call", { op, args });
const native = "__TAURI_INTERNALS__" in window;
const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);
const short = (id: string) => id.slice(0, 8);

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
  const [authMode, setAuthMode] = useState<
    "login" | "register" | "verify" | "resend" | "recover" | "reset"
  >("login");
  const refreshVersion = useRef(0);

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
      setDevices(all);
      setRequests(sessions);
      const q = await invoke<PublicState>("state");
      if (version !== refreshVersion.current) return;
      setPublicState(q);
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
      setProblem(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const signOut = () =>
    act(async () => {
      refreshVersion.current++;
      try {
        await api("logout");
      } finally {
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
            <span>遥舟 · 设备与授权</span>
          </div>
        </div>
        <div className="side-label">工作空间</div>
        <nav aria-label="主导航">
          {(
            [
              ["overview", "总览", "◫"],
              ["devices", "我的设备", "▣"],
              ["requests", "连接请求", "⇄"],
              ["security", "账号安全", "◇"],
              ...(me?.role === "admin" ? [["admin", "管理控制台", "⚙"]] : []),
              ["settings", "共享与设置", "☷"],
            ] as [Tab, string, string][]
          ).map(([id, label, icon]) => (
            <button
              key={id}
              className={tab === id ? "active" : ""}
              onClick={() => setTab(id)}
            >
              <span>{icon}</span>
              {label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
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
                    requests: "连接请求",
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
            <span>{problem}</span>
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
        {!me && tab !== "settings" ? (
          <Auth
            mode={authMode}
            setMode={setAuthMode}
            busy={busy}
            act={act}
            onLogin={async (args) => {
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
                本机开发可用回环 HTTP。公网地址须为证书有效的 HTTPS，也支持
                HTTPS IP 地址。
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
              <h2>本机能力</h2>
              <p className="muted">
                Windows 可启用 JPEG
                屏幕共享和鼠标键盘控制。文件传输仍未启用。系统安全桌面、UAC
                和无人登录桌面不支持。
              </p>
              <div className="tag-row">
                <span className="tag ready">账号与设备</span>
                <span className="tag ready">Windows 画面与输入</span>
                <span className="tag">文件传输 · 后续实现</span>
              </div>
            </div>
            <div className="card">
              <h2>让另一台设备连接这台电脑</h2>
              <ol className="setup-guide">
                <li>
                  <strong>绑定电脑</strong>：在首页将本机添加到你的账号。
                </li>
                <li>
                  <strong>启动连接服务</strong>
                  ：填写管理员提供的中继地址，然后点击下方启动按钮。
                </li>
                <li>
                  <strong>允许本机共享</strong>
                  ：开启后，前往“连接请求”批准对方的连接。
                </li>
              </ol>
              <p className="muted">
                默认按服务器地址使用 8443
                中继端口；管理员提供不同地址时，请在此修改。开启共享后默认逐次批准；在本机设备面板显式开启“远程值守”后，同账号连接可自动批准。
              </p>
              <form
                className="form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void act(
                    async () => {
                      await invoke("transport_start", {
                        relayUrl: relayUrl || null,
                        forceRelay,
                        bindAddr,
                      });
                      setTransportReady(true);
                    },
                    "安全传输已启动",
                    false,
                  );
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
                <details className="advanced-settings">
                  <summary>高级连接设置（通常无需修改）</summary>
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
                </details>
                <button
                  className="primary"
                  disabled={busy || !publicState.deviceId}
                >
                  {transportReady ? "重新启动连接服务" : "启动连接服务"}
                </button>
              </form>
              <p className="hint">
                {transportReady
                  ? "连接服务已启动，下一步：点击“开启本机共享”。"
                  : "尚未启动。请先在首页绑定电脑，再填写中继地址并启动连接服务。"}
              </p>
              <div className="row-actions">
                <button
                  className={publicState.sharing ? "danger-text" : "primary"}
                  disabled={busy || !transportReady}
                  onClick={() =>
                    void act(
                      () =>
                        invoke(
                          publicState.sharing
                            ? "share_disable"
                            : "share_enable",
                        ),
                      publicState.sharing
                        ? "已停止本机共享"
                        : "已开启本机共享；每次请求仍需明确批准",
                    )
                  }
                >
                  {publicState.sharing ? "立即停止共享" : "开启本机共享"}
                </button>
                <span>
                  {publicState.sharing
                    ? "本机共享已开启 · 等待明确批准"
                    : "尚未开启：其他设备不能查看或控制这台电脑"}
                </span>
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
            prepareTransport={async () => {
              if (!transportReady) {
                await invoke("transport_start", {
                  relayUrl: relayUrl || null,
                  forceRelay,
                  bindAddr,
                });
                setTransportReady(true);
              }
            }}
            onView={(id) => {
              void invoke("viewer_open", { id }).catch((e) =>
                setProblem(errorText(e)),
              );
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
            onView={(id) => {
              void invoke("viewer_open", { id }).catch((e) =>
                setProblem(errorText(e)),
              );
            }}
          />
        ) : tab === "security" ? (
          <Security me={me!} busy={busy} act={act} signOut={signOut} />
        ) : me?.role === "admin" && tab === "admin" ? (
          <Admin busy={busy} act={act} />
        ) : null}
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
      return act(
        () => api("recover_request", { email }),
        "如果账号存在，恢复邮件已发送",
      );
    return act(
      () => api("recover_complete", { token, password: newPassword }),
      "密码已更新，请重新登录",
    );
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
            : "连接你的设备，从可信的身份开始。"}
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
                    : "继续"}
          </button>
        </form>
        <div className="auth-links">
          {(
            [
              "login",
              "register",
              "verify",
              "resend",
              "recover",
              "reset",
            ] as const
          )
            .filter(
              (x) =>
                x !== mode &&
                (mode !== "verify" || x === "resend" || x === "login"),
            )
            .map((x) => (
              <button key={x} onClick={() => setMode(x)}>
                {titles[x]}
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
  const [name, setName] = useState("这台 Windows 电脑");
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
            <h2>绑定这台电脑</h2>
            <p>
              将这台电脑添加到你的账号。绑定后可发起连接；允许其他设备连接时，请另行开启本机共享。
            </p>
          </div>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(() => api("bind", { name }), "本机设备已绑定");
            }}
          >
            <input
              aria-label="本机设备名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={80}
              required
            />
            <button className="primary" disabled={busy}>
              添加这台电脑
            </button>
          </form>
        </div>
      ) : (
        <div className="card local-device">
          <div>
            <div className="eyebrow">THIS DEVICE</div>
            <h2>{local?.name ?? "本机设备"}</h2>
            <p className="muted">
              设备 ID {short(state.deviceId)} ·{" "}
              {local?.online ? "在线" : "等待心跳"} · 仅授权客户端
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
              先启动传输，再开启共享；连接仍需你批准。
            </small>
          </div>
        </div>
      )}
      <div className="card">
        <div className="section-heading">
          <div>
            <div className="eyebrow">ACTIVITY</div>
            <h2>最近授权会话</h2>
          </div>
          <button
            className="text-button"
            onClick={() => onNavigate("requests")}
          >
            全部请求 →
          </button>
        </div>
        {requests.length ? (
          requests.slice(0, 4).map((x) => (
            <div className="list-row" key={x.id}>
              <div>
                <strong>
                  {x.permission === "view"
                    ? "查看"
                    : x.permission === "control"
                      ? "控制"
                      : "文件"}
                  申请
                </strong>
                <small>
                  {short(x.source_device_id)} → {short(x.target_device_id)}
                </small>
              </div>
              <span className={"tag " + (x.state === "pending" ? "warm" : "")}>
                {x.state}
              </span>
            </div>
          ))
        ) : (
          <Empty text="尚无授权请求。绑定设备后可在“连接请求”中发起。" />
        )}
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
  onView: (id: string) => void;
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
            onView(request.id);
            setConnectionNote("已在独立窗口打开，可返回管理设备。");
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
        setConnectionNote("等待对方批准，批准后自动打开远程画面。");
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
    ? "先在首页添加这台电脑，才能发起连接。"
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
      <div className="device-detail-pane">
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
                      开启后可接收连接请求；默认每次需要批准。
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
                    {state.sharing ? "停止共享" : "开启本机共享"}
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
                      仅本次共享有效；关闭会结束现有入站连接并恢复手动批准。需要系统已登录且应用运行。
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
                  <button
                    className="secondary"
                    disabled
                    title="此版本尚未提供文件传输"
                  >
                    文件传输 · 开发中
                  </button>
                </div>
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
            <div className="device-facts">
              <span>设备 ID</span>
              <code>{device.id}</code>
              <span>最后在线</span>
              <span>
                {device.last_seen_at
                  ? new Date(device.last_seen_at).toLocaleString()
                  : "暂无记录"}
              </span>
            </div>
            <details className="advanced-settings">
              <summary>设备管理</summary>
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
                <div className="notice-strip">
                  <p>解绑后，此设备的连接凭据和授权将被撤销。</p>
                  <button
                    className="danger-text"
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        await api("unbind_device", { id: device.id });
                        setConfirmUnbind(false);
                      }, "设备已解绑")
                    }
                  >
                    确认解绑
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
                  解绑设备
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
  onView,
}: {
  devices: Device[];
  requests: Remote[];
  pending: Pending[];
  state: PublicState;
  busy: boolean;
  act: Action;
  transportReady: boolean;
  onView: (id: string) => void;
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
    [inviteTarget, setInviteTarget] = useState(""),
    [permission, setPermission] = useState<"view" | "control" | "files">(
      "view",
    ),
    [code, setCode] = useState(""),
    [invite, setInvite] = useState(""),
    [inviteId, setInviteId] = useState("");
  const targets = devices.filter(
    (x) =>
      x.id !== state.deviceId &&
      x.enabled &&
      x.online &&
      (permission === "files" ? x.can_files : x.can_host),
  );
  return (
    <div className="stack">
      <div className="notice-strip">
        Windows 主机开启共享后，明确批准的会话可观看 JPEG
        画面；控制权限可注入鼠标键盘。文件传输尚未启用。
      </div>
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">REQUEST ACCESS</div>
          <h2>申请授权</h2>
          <p className="muted">
            目标设备须在线且声明相应能力。跨账号设备可填写其 ID 和一次性邀请码。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                () =>
                  api("request", {
                    source_device_id: state.deviceId,
                    target_device_id: target,
                    permission,
                    invitation_code: code || undefined,
                  }),
                "授权请求已提交",
              );
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
                <option value="custom">手动输入设备 ID</option>
                {targets.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </label>
            <input
              aria-label="目标设备 ID"
              placeholder="目标设备 UUID"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              required
            />
            <label>
              权限
              <select
                value={permission}
                onChange={(e) =>
                  setPermission(e.target.value as typeof permission)
                }
              >
                <option value="view">查看</option>
                <option value="control">控制</option>
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
              提交请求
            </button>
          </form>
          {!state.deviceId && <p className="hint">请先在总览绑定本机。</p>}
        </div>
        <div className="card">
          <div className="eyebrow">SHARE ACCESS</div>
          <h2>生成临时邀请</h2>
          <p className="muted">
            目标设备在线且具备对应能力时，可创建一次性邀请码。请通过可信渠道手动交给对方。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(
                async () => {
                  const v = await api<{ code: string; id: string }>("invite", {
                    target_device_id: inviteTarget,
                    permission,
                  });
                  setInvite(v.code);
                  setInviteId(v.id);
                },
                "邀请码已生成",
                false,
              );
            }}
          >
            <label>
              我的目标设备
              <select
                value={inviteTarget}
                onChange={(e) => setInviteTarget(e.target.value)}
                required
              >
                <option value="">选择设备</option>
                {devices
                  .filter(
                    (x) =>
                      (permission === "files" ? x.can_files : x.can_host) &&
                      x.enabled,
                  )
                  .map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
              </select>
            </label>
            <button disabled={busy} className="secondary">
              生成邀请码
            </button>
          </form>
          {invite && (
            <div className="invite-code">
              <span>一次性邀请码</span>
              <code>{invite}</code>
              <button onClick={() => setInvite("")}>隐藏</button>
              <button
                className="danger-text"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await api("revoke_invite", { id: inviteId });
                    setInvite("");
                    setInviteId("");
                  }, "邀请已撤销")
                }
              >
                撤销邀请
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="card">
        <div className="section-heading">
          <div>
            <div className="eyebrow">INBOX</div>
            <h2>等待本机批准</h2>
          </div>
          <span className="count">{pending.length}</span>
        </div>
        {pending.length ? (
          pending.map((p) => (
            <div className="list-row" key={p.id}>
              <div>
                <strong>
                  {p.permission === "control" ? "控制" : "仅查看"} ·{" "}
                  {p.source_device_name} ({short(p.source_device_id)})
                </strong>
                <small>
                  申请人 {p.requester_email} · {short(p.id)}
                </small>
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
                  disabled={busy}
                  className="primary"
                  onClick={() => {
                    if (
                      window.confirm(
                        `批准 ${p.requester_email} 的${p.permission === "control" ? "控制" : "查看"}请求？批准后对方可接收实时画面${p.permission === "control" ? "并操作鼠标键盘" : ""}。`,
                      )
                    )
                      void act(
                        () => api("decide", { id: p.id, approve: true }),
                        "已批准授权；连接后将提供 JPEG 画面",
                      );
                  }}
                >
                  批准
                </button>
              </div>
            </div>
          ))
        ) : (
          <Empty text="目前没有等待本机处理的请求。" />
        )}
      </div>
      <div className="card">
        <div className="eyebrow">HISTORY</div>
        <h2>授权会话</h2>
        {requests.length ? (
          requests.map((r) => (
            <div className="list-row" key={r.id}>
              <div>
                <strong>
                  {r.permission} · {r.state}
                </strong>
                <small>
                  {short(r.source_device_id)} → {short(r.target_device_id)} ·{" "}
                  {short(r.id)}
                </small>
                {r.state === "approved" && (
                  <small>
                    链路：{paths[r.id]?.state ?? "未连接"}
                    {paths[r.id]?.rtt_ms != null
                      ? ` · RTT ${paths[r.id].rtt_ms} ms`
                      : ""}
                  </small>
                )}
                {paths[r.id]?.verification_code && (
                  <small>双方校验码：{paths[r.id].verification_code}</small>
                )}
                {paths[r.id]?.error && paths[r.id]?.state === "closed" && (
                  <small>结束原因：{paths[r.id].error}</small>
                )}
              </div>
              <div className="row-actions">
                {r.state === "approved" &&
                  r.source_device_id === state.deviceId &&
                  r.permission !== "files" && (
                    <button
                      className="secondary"
                      disabled={busy || !transportReady}
                      onClick={() =>
                        void act(
                          async () => {
                            await invoke("transport_connect", {
                              id: r.id,
                              permission: r.permission,
                            });
                            onView(r.id);
                          },
                          "已打开独立远程窗口",
                          false,
                        )
                      }
                    >
                      连接并查看
                    </button>
                  )}
                {r.source_device_id === state.deviceId &&
                  paths[r.id]?.state !== "closed" &&
                  paths[r.id]?.state != null && (
                    <button className="secondary" onClick={() => onView(r.id)}>
                      查看画面
                    </button>
                  )}
                {r.state === "approved" &&
                  paths[r.id]?.state !== "closed" &&
                  paths[r.id]?.state != null && (
                    <button
                      className="secondary"
                      disabled={busy}
                      onClick={() =>
                        void act(
                          () => invoke("transport_close", { id: r.id }),
                          "本机会话已关闭",
                          false,
                        )
                      }
                    >
                      断开本机
                    </button>
                  )}
                {["pending", "approved"].includes(r.state) && (
                  <button
                    className="danger-text"
                    disabled={busy}
                    onClick={() =>
                      void act(
                        () => api("revoke_remote", { id: r.id }),
                        "请求已撤销",
                      )
                    }
                  >
                    取消 / 撤销
                  </button>
                )}
              </div>
            </div>
          ))
        ) : (
          <Empty text="尚无授权会话。" />
        )}
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
  state: string;
  rtt_ms: number | null;
  permission: "view" | "control";
  verification_code: string | null;
  displays: RemoteDisplay[];
  error: string | null;
  retryable?: boolean;
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
  const count = useRef(0);
  const url = useRef<string | null>(null);
  const inputQueue = useRef<Promise<unknown>>(Promise.resolve());
  const inputPending = useRef(0);
  const lastMove = useRef(0);
  const [ended, setEnded] = useState(false);
  const [reconnecting, setReconnecting] = useState(false);
  const screen = useRef<HTMLDivElement>(null);
  const generation = useRef(0);
  const control =
    !ended && status?.state !== "closed" && status?.permission === "control";
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
      count.current = 0;
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
      setFps(count.current);
      count.current = 0;
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
          count.current++;
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
    const w = picture.width * scale,
      h = picture.height * scale;
    const x = (e.clientX - rect.left - (rect.width - w) / 2) / w;
    const y = (e.clientY - rect.top - (rect.height - h) / 2) / h;
    return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
  };
  return (
    <section className="viewer-window">
      <div className="viewer-body">
        <div className="viewer-toolbar">
          <div>
            <strong>
              {ended
                ? "会话已结束"
                : status?.permission === "control"
                  ? "远程控制"
                  : "仅查看"}
            </strong>
            <p className="muted">
              连接 {short(id)} ·{" "}
              {status?.state === "direct"
                ? "P2P 直连"
                : status?.state === "relay"
                  ? "中继回退"
                  : ended
                    ? "已断开"
                    : "协商路径中"}{" "}
              · 网络 RTT {status?.rtt_ms ?? "—"} ms · 实际接收 {fps} FPS
            </p>
            <details>
              <summary>连接详情</summary>
              <p className="muted">
                双方校验码：
                <strong>{status?.verification_code ?? "连接中"}</strong>
                。请通过可信渠道比较。路径可在直连与中继间迁移，无需重新授权。
              </p>
            </details>
          </div>
          <div className="row-actions">
            <button
              onClick={() =>
                void invoke("viewer_window_action", { action: "minimize" })
              }
            >
              最小化
            </button>
            <button
              onClick={() =>
                void invoke("viewer_window_action", { action: "maximize" })
              }
            >
              最大化 / 还原
            </button>
            <button
              onClick={() =>
                void invoke("viewer_window_action", { action: "fullscreen" })
              }
            >
              全屏 / 退出全屏
            </button>
            <button className="danger-text" onClick={onStop}>
              {reconnecting ? "取消重连并关闭" : "结束并关闭"}
            </button>
          </div>
        </div>
        {problem && <div className="alert error">{problem}</div>}
        {status?.error && <div className="alert error">{status.error}</div>}
        <label>
          远端显示器{" "}
          {reconnecting && (
            <span role="status">正在重连（最多 3 次）；等待新的授权批准…</span>
          )}
          <select
            disabled={ended}
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
              释放按键
            </button>
          </form>
        )}
        <p className="hint">
          画面使用低帧率 JPEG；网络 RTT 不是画面延迟。系统安全桌面、UAC
          和无人登录桌面不支持。
        </p>
      </div>
    </section>
  );
}

function Security({
  me,
  busy,
  act,
  signOut,
}: {
  me: Me;
  busy: boolean;
  act: Action;
  signOut: () => Promise<void>;
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
          加载登录会话失败：{loadError}
        </div>
      )}
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">PASSWORD</div>
          <h2>修改密码</h2>
          <p className="muted">
            更新后所有登录与设备凭据都将失效，请重新登录并绑定本机。
          </p>
          <form
            className="form"
            onSubmit={(e) => {
              e.preventDefault();
              void act(async () => {
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
            </label>
            <button className="primary" disabled={busy}>
              更新密码
            </button>
          </form>
        </div>
        <div className="card">
          <div className="eyebrow">ACCOUNT</div>
          <h2>登录状态</h2>
          <p className="muted">{me.email}</p>
          <p className="muted">当前会话 {short(me.session_id)}</p>
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
            <h2>我的登录会话</h2>
          </div>
          <button
            className="text-button"
            onClick={() =>
              void api<typeof sessions>("sessions").then(setSessions)
            }
          >
            刷新
          </button>
        </div>
        {sessions.length ? (
          sessions.map((x) => (
            <div className="list-row" key={x.id}>
              <div>
                <strong>
                  {x.id === me.session_id ? "当前会话" : "其他会话"} ·{" "}
                  {short(x.id)}
                </strong>
                <small>
                  创建于 {x.created_at} · 最近使用 {x.last_used_at}
                </small>
              </div>
              {x.id !== me.session_id && (
                <button
                  className="danger-text"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      await api("revoke_session", { id: x.id });
                      setSessions(await api("sessions"));
                    }, "会话已撤销")
                  }
                >
                  撤销
                </button>
              )}
            </div>
          ))
        ) : (
          <Empty text="没有可显示的登录会话。" />
        )}
      </div>
    </div>
  );
}
function Admin({ busy, act }: { busy: boolean; act: Action }) {
  const [users, setUsers] = useState<User[]>([]),
    [after, setAfter] = useState(""),
    [registration, setRegistration] = useState("open"),
    [email, setEmail] = useState(""),
    [invite, setInvite] = useState(""),
    [inviteId, setInviteId] = useState(""),
    [loadError, setLoadError] = useState(""),
    [deviceId, setDeviceId] = useState(""),
    [sessions, setSessions] = useState<unknown[]>([]),
    [audit, setAudit] = useState<unknown[]>([]);
  const load = async () => {
    const [u, r, s, a] = await Promise.all([
      api<User[]>("admin_users"),
      api<{ value: string }>("admin_registration"),
      api<unknown[]>("admin_sessions"),
      api<unknown[]>("admin_audit"),
    ]);
    setUsers(u);
    setAfter(u.at(-1)?.id ?? "");
    setRegistration(r.value);
    setSessions(s);
    setAudit(a);
  };
  useEffect(() => {
    void load().catch((e) => setLoadError(errorText(e)));
  }, []);
  const loadMore = () =>
    void api<User[]>("admin_users", { after }).then((u) => {
      setUsers((x) => [...x, ...u]);
      setAfter(u.at(-1)?.id ?? "");
    });
  return (
    <div className="stack">
      {loadError && (
        <div className="alert error" role="alert">
          加载管理数据失败：{loadError}
        </div>
      )}
      <div className="notice-strip">
        管理员操作由服务端再次鉴权。管理员身份不能代替目标设备批准远控。
      </div>
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">REGISTRATION</div>
          <h2>注册策略</h2>
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
              value={registration}
              onChange={(e) => setRegistration(e.target.value)}
            >
              <option value="open">开放注册</option>
              <option value="invite_only">仅邀请</option>
              <option value="closed">关闭注册</option>
            </select>
            <button className="primary" disabled={busy}>
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
                  setInvite(v.code);
                  setInviteId(v.id);
                },
                "邀请已创建",
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
            <button className="primary" disabled={busy}>
              创建
            </button>
          </form>
          {invite && (
            <div className="invite-code">
              <span>仅显示一次</span>
              <code>{invite}</code>
              <button onClick={() => setInvite("")}>隐藏</button>
              <button
                className="danger-text"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await api("admin_invite_revoke", { id: inviteId });
                    setInvite("");
                    setInviteId("");
                  }, "注册邀请已撤销")
                }
              >
                撤销邀请
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="card">
        <div className="eyebrow">DEVICE POLICY</div>
        <h2>设备禁用与恢复</h2>
        <div className="inline-form">
          <input
            placeholder="设备 UUID"
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
            禁用
          </button>
          <button
            className="secondary"
            disabled={busy || !deviceId}
            onClick={() =>
              void act(
                () => api("admin_device_enabled", { id: deviceId }),
                "设备已恢复；设备所有者须重新签名绑定",
              )
            }
          >
            恢复
          </button>
        </div>
      </div>
      <div className="card">
        <div className="section-heading">
          <div>
            <div className="eyebrow">USERS</div>
            <h2>用户管理</h2>
          </div>
          <button className="text-button" onClick={() => void load()}>
            刷新
          </button>
        </div>
        {users.map((u) => (
          <div className="list-row" key={u.id}>
            <div>
              <strong>{u.email}</strong>
              <small>
                {u.role} · {u.verified ? "已验证" : "未验证"} ·{" "}
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
              {u.enabled ? "停用" : "启用"}
            </button>
          </div>
        ))}
        {users.length === 100 && (
          <button className="text-button" onClick={loadMore}>
            加载更多
          </button>
        )}
      </div>
      <div className="two-col">
        <div className="card">
          <div className="eyebrow">SESSIONS</div>
          <h2>登录会话</h2>
          {sessions.length ? (
            sessions.map((s, i) => (
              <pre className="meta-row" key={i}>
                {JSON.stringify(s)}
              </pre>
            ))
          ) : (
            <Empty text="暂无会话元数据" />
          )}
        </div>
        <div className="card">
          <div className="eyebrow">AUDIT</div>
          <h2>审计事件</h2>
          {audit.length ? (
            audit.map((a, i) => (
              <pre className="meta-row" key={i}>
                {JSON.stringify(a)}
              </pre>
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
