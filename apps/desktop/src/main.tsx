import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import type { Device, Me, Pending, Remote, User } from "@farsail/ui";
import "./style.css";

type Tab =
  "overview" | "devices" | "requests" | "security" | "admin" | "settings";
type PublicState = {
  server: string;
  signedIn: boolean;
  deviceId: string | null;
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
        setTab("overview");
        await refresh();
      }
    }, "已退出登录");

  return (
    <div className="app">
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
              ["settings", "设置", "☷"],
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
          <div className="status-dot" />
          账号与授权界面
          <br />
          <small>远程画面与文件通道待实现</small>
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
                    overview: "欢迎登船",
                    devices: "我的设备",
                    requests: "连接请求",
                    security: "账号安全",
                    admin: "管理控制台",
                    settings: "设置",
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
              setTab("overview");
            }}
          />
        ) : tab === "settings" ? (
          <section className="stack">
            <div className="card">
              <div className="section-heading">
                <div>
                  <div className="eyebrow">CONNECTION</div>
                  <h2>协调服务地址</h2>
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
                已接入认证加密传输；本机仍未实现远程画面、鼠标键盘控制或文件内容处理，因此注册设备时不声明被控和文件能力。
              </p>
              <div className="tag-row">
                <span className="tag ready">账号与设备</span>
                <span className="tag">画面与输入 · 后续实现</span>
                <span className="tag">文件传输 · 后续实现</span>
              </div>
            </div>
            <div className="card">
              <h2>安全传输</h2>
              <p className="muted">可使用本地直连或证书有效的自建 HTTPS 中继。启动后地址仅向获批会话的设备公开。</p>
              <form className="form" onSubmit={(e) => { e.preventDefault(); void act(async () => {
                await invoke("transport_start", { relayUrl: relayUrl || null, forceRelay, bindAddr });
                setTransportReady(true);
              }, "安全传输已启动", false); }}>
                <label>UDP 绑定地址<input aria-label="UDP 绑定地址" value={bindAddr} onChange={(e) => setBindAddr(e.target.value)} /></label>
                <label>自建中继 HTTPS 地址<input aria-label="中继地址" placeholder="https://relay.example.com/" value={relayUrl} onChange={(e) => setRelayUrl(e.target.value)} /></label>
                <label><input type="checkbox" checked={forceRelay} onChange={(e) => setForceRelay(e.target.checked)} /> 强制中继（禁用 IP 直连）</label>
                <button className="primary" disabled={busy || !publicState.deviceId}>启动传输</button>
              </form>
              <p className="hint">{transportReady ? "传输端点已启动；会话路径与 RTT 在连接请求页显示。" : "先绑定本机，再启动传输。"}</p>
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
      return act(() => onLogin({ email, password }), "欢迎回来");
    if (mode === "register")
      return act(
        () =>
          api("register", {
            email,
            password,
            invite_code: invite || undefined,
          }),
        "注册成功，请到邮箱查收验证链接",
      );
    if (mode === "verify")
      return act(() => api("verify", { token }), "邮箱已验证，现在可以登录");
    if (mode === "resend")
      return act(
        () => api("resend", { email, password }),
        "验证邮件已重新发送",
      );
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
      <section className="card auth-card">
        <div className="eyebrow">SECURE ACCESS</div>
        <h2>{titles[mode]}</h2>
        <p className="muted">连接你的设备，从可信的身份开始。</p>
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
              邮件中的一次性令牌
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
            {busy ? "请稍候…" : "继续"}
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
            .filter((x) => x !== mode)
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
          <h2>设备已就绪，授权由你掌控。</h2>
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
          <small>由心跳租约确认</small>
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
              确认后生成本机 Ed25519 身份，并保存在 Windows
              用户保护的凭据中。当前仅作为发起端，不声明屏幕或文件能力。
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
              确认绑定
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
          <span className="tag ready">身份已绑定</span>
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
  devices,
  state,
  busy,
  act,
}: {
  devices: Device[];
  state: PublicState;
  busy: boolean;
  act: Action;
}) {
  const [search, setSearch] = useState(""),
    [filter, setFilter] = useState<"all" | "host" | "files" | "online">("all");
  const shown = devices.filter(
    (x) =>
      (x.name.toLowerCase().includes(search.toLowerCase()) ||
        x.id.includes(search)) &&
      (filter === "all" ||
        (filter === "host" && x.can_host) ||
        (filter === "files" && x.can_files) ||
        (filter === "online" && x.online)),
  );
  return (
    <div className="stack">
      <div className="card">
        <div className="section-heading">
          <div>
            <div className="eyebrow">FLEET</div>
            <h2>账号下的全部设备</h2>
          </div>
          <span className="count">{devices.length} 台设备</span>
        </div>
        <div className="filters">
          <input
            placeholder="搜索名称或设备 ID"
            aria-label="搜索设备"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <select
            aria-label="设备能力筛选"
            value={filter}
            onChange={(e) => setFilter(e.target.value as typeof filter)}
          >
            <option value="all">全部设备</option>
            <option value="host">可被控</option>
            <option value="files">文件能力</option>
            <option value="online">在线</option>
          </select>
        </div>
        {shown.length ? (
          <div className="device-grid">
            {shown.map((d) => (
              <div className="device-card" key={d.id}>
                <div className="device-top">
                  <div className="device-symbol">▣</div>
                  <span className={"tag " + (d.online ? "ready" : "")}>
                    {!d.enabled ? "已停用" : d.online ? "在线" : "离线"}
                  </span>
                </div>
                <h3>{d.name}</h3>
                <p>
                  {d.platform} · {short(d.id)}{" "}
                  {d.id === state.deviceId ? "· 本机" : ""}
                </p>
                <div className="tag-row">
                  <span className="tag">
                    {d.can_host ? "可被控" : "仅控制端"}
                  </span>
                  {d.can_files && <span className="tag">文件</span>}
                </div>
                <div className="device-actions">
                  <button
                    disabled={busy}
                    onClick={() => {
                      const name = window.prompt("新的设备名称", d.name);
                      if (name?.trim())
                        void act(
                          () =>
                            api("rename_device", {
                              id: d.id,
                              name: name.trim(),
                            }),
                          "设备名称已更新",
                        );
                    }}
                  >
                    重命名
                  </button>
                  <button
                    disabled={busy}
                    className="danger-text"
                    onClick={() => {
                      if (
                        window.confirm(
                          `解绑 ${d.name}？此操作会撤销设备凭据和关联授权。`,
                        )
                      )
                        void act(
                          () => api("unbind_device", { id: d.id }),
                          "设备已解绑",
                        );
                    }}
                  >
                    解绑
                  </button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <Empty
            text={
              devices.length
                ? "没有符合条件的设备。"
                : "尚无设备，回到总览绑定本机。"
            }
          />
        )}
      </div>
    </div>
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
}: {
  devices: Device[];
  requests: Remote[];
  pending: Pending[];
  state: PublicState;
  busy: boolean;
  act: Action;
  transportReady: boolean;
}) {
  const [paths, setPaths] = useState<Record<string, { state: string; rtt_ms: number | null }>>({});
  useEffect(() => {
    if (!transportReady || !native) return;
    const poll = () => {
      for (const r of requests.filter((x) => x.state === "approved")) {
        void invoke<{ state: string; rtt_ms: number | null }>("transport_status", { id: r.id })
          .then((v) => setPaths((old) => ({ ...old, [r.id]: v }))).catch(() => {});
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
        授权后可建立认证加密通道。画面、输入和文件内容处理仍未实现；批准不会开始共享。
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
                <option value="files">文件</option>
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
                  {p.permission} · 发起设备 {short(p.source_device_id)}
                </strong>
                <small>
                  申请人 {short(p.requester_id)} · {short(p.id)}
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
                        "批准此授权请求？当前版本不会启动画面或文件传输。",
                      )
                    )
                      void act(
                        () => api("decide", { id: p.id, approve: true }),
                        "已批准授权；认证传输可连接，画面与文件尚未启用",
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
                {r.state === "approved" && <small>链路：{paths[r.id]?.state ?? "未连接"}{paths[r.id]?.rtt_ms != null ? ` · RTT ${paths[r.id].rtt_ms} ms` : ""}</small>}
              </div>
              <div className="row-actions">
              {r.state === "approved" && r.source_device_id === state.deviceId && (
                <button className="secondary" disabled={busy || !transportReady} onClick={() => void act(
                  () => invoke("transport_connect", { id: r.id, permission: r.permission }),
                  "认证通道已连接；媒体与文件处理尚未启用",
                )}>连接</button>
              )}
              {r.state === "approved" && paths[r.id]?.state !== "closed" && paths[r.id]?.state != null && (
                <button className="secondary" disabled={busy} onClick={() => void act(
                  () => invoke("transport_close", { id: r.id }), "本机会话已关闭", false,
                )}>断开本机</button>
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
    <App />
  </React.StrictMode>,
);
