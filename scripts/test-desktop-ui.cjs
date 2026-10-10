// Run with Playwright available on NODE_PATH and the desktop Vite server on 1420.
// Synthetic IPC fixtures verify UI flow only; they are never bundled into the app.
const { chromium, webkit } = require("playwright");
const assert = require("node:assert/strict");
const fs = require("node:fs");
(async () => {
  const browser = await (
    process.env.UI_ENGINE === "webkit" ? webkit : chromium
  ).launch({
    ...(process.env.UI_ENGINE === "webkit"
      ? {}
      : { channel: process.env.UI_BROWSER || "chrome" }),
    ...(process.env.UI_ENGINE === "webkit" && process.env.UI_WEBKIT_EXECUTABLE
      ? { executablePath: process.env.UI_WEBKIT_EXECUTABLE }
      : {}),
    headless: true,
  });
  const page = await browser.newPage({
    viewport: { width: 1120, height: 760 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    const local = {
      id: "local-0001",
      name: "工作电脑",
      platform: "windows",
      online: true,
      enabled: true,
      can_host: false,
      can_files: false,
      last_seen_at: "2026-09-29T00:00:00Z",
    };
    const remote = {
      ...local,
      id: "remote-0002",
      name: "家里的电脑",
      can_host: true,
    };
    const offline = {
      ...remote,
      id: "offline-0003",
      name: "备用电脑",
      online: false,
    };
    window.fixture = {
      signedIn: false,
      verified: false,
      failLogin: false,
      loginFailure: "",
      bindFailure: "",
      sharing: false,
      remoteWatch: false,
      sharePreferences: { sharing: false, watch: false, restore: "idle" },
      bound: true,
      computerName: "DESKTOP-TEST",
      platform: "windows",
      canShareLocalScreen: true,
      role: "user",
      failNextOp: "",
      failTransport: false,
      clipboard: "",
      failClipboard: false,
      connected: {},
      viewerInstances: {},
      requests: [],
      calls: [],
      update: {
        currentVersion: "0.1.20",
        availableVersion: null,
        phase: "idle",
        downloaded: 0,
        total: null,
        message: "将自动检查新版，也可手动检查更新。",
        preferences: { autoCheck: true, autoInstall: false },
      },
      updateFailure: "",
      updateConnected: false,

      devices: [local, remote, offline],
      pending: [],
      sessions: [
        {
          id: "session-1",
          created_at: "2026-09-29T00:00:00Z",
          last_used_at: "2026-09-29T01:00:00Z",
        },
        {
          id: "session-2",
          created_at: "2026-09-28T00:00:00Z",
          last_used_at: "2026-09-28T01:00:00Z",
        },
      ],
    };
    const f = window.fixture;
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text) => {
          if (f.failClipboard) throw new Error("clipboard unavailable");
          f.clipboard = text;
        },
      },
    });
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" } },
      invoke: async (cmd, args = {}) => {
        f.calls.push({ cmd, args });
        if (cmd.startsWith("update_")) {
          if (cmd === "update_preferences")
            f.update.preferences = args.preferences;
          if (cmd === "update_check")
            Object.assign(f.update, {
              availableVersion: "0.1.21",
              phase: "available",
              message: "发现新版本 0.1.21。",
            });
          if (cmd === "update_download") {
            if (f.updateFailure) {
              const message = f.updateFailure;
              f.updateFailure = "";
              Object.assign(f.update, { phase: "error", message });
              throw message;
            }
            Object.assign(f.update, {
              phase: "ready",
              message: "更新已下载并通过签名校验，可以安装。",
              downloaded: 100,
              total: 100,
            });
          }
          if (cmd === "update_install")
            Object.assign(f.update, {
              phase: f.updateConnected ? "waiting" : "installing",
              message: f.updateConnected
                ? "更新已就绪，等待远程连接结束。请关闭远程窗口后安装。"
                : "正在安装更新，完成后将重新启动。",
            });
          return { ...f.update, preferences: { ...f.update.preferences } };
        }
        if (cmd === "state")
          return {
            signedIn: f.signedIn,
            server: "https://example.invalid",
            deviceId: f.signedIn && f.bound ? local.id : null,
            computerName: f.computerName,
            platform: f.platform,
            canShareLocalScreen: f.canShareLocalScreen,
            sharing: f.sharing,
            remoteWatch: f.remoteWatch,
            sharePreferences: f.sharePreferences,
            transportRunning: f.transportRunning,
          };
        if (cmd === "transport_start") {
          if (f.failTransport) throw "relay did not become reachable";
          f.sharing = false;
          local.can_host = false;
          f.connected = {};
          f.transportRunning = true;
          return {};
        }
        if (cmd === "transport_connect") {
          f.connected[args.id] = args.permission;
          return {};
        }
        if (cmd === "transport_close") {
          delete f.connected[args.id];
          return {};
        }
        if (cmd === "viewer_open") {
          const instance = `viewer-instance-${args.id}`;
          f.viewerInstances[instance] = true;
          return instance;
        }
        if (cmd === "viewer_window_active")
          return f.viewerInstances[args.instance] === true;
        if (cmd === "share_enable") {
          f.sharing = true;
          f.sharePreferences = {
            sharing: true,
            watch: f.sharePreferences.watch,
            restore: "ready",
          };
          local.can_host = true;
          return {};
        }
        if (cmd === "share_disable") {
          f.sharing = false;
          f.remoteWatch = false;
          f.sharePreferences = {
            sharing: false,
            watch: false,
            restore: "idle",
          };
          local.can_host = false;
          return {};
        }
        if (cmd === "remote_watch") {
          f.remoteWatch = args.enabled;
          f.sharePreferences.watch = args.enabled;
          return {};
        }
        if (cmd === "remote_status") {
          if (!f.connected[args.id]) throw "session not connected";
          return {
            displays: [],
            state: "connected",
            permission: f.connected[args.id],
            error: null,
          };
        }
        if (cmd === "transport_status")
          return {
            state: f.connected[args.id] ? "direct" : "closed",
            rtt_ms: null,
          };
        if (cmd === "media_next") return new Promise(() => {});
        if (cmd.startsWith("plugin:window|")) return {};
        if (cmd !== "call") return {};
        const { op, args: a = {} } = args;
        if (f.failNextOp === op) {
          f.failNextOp = "";
          throw "HTTP 503: unavailable";
        }
        if (op === "login") {
          if (f.loginFailure) throw `HTTP 401: ${f.loginFailure}`;
          if (f.failLogin || !f.verified) throw "HTTP 401: unauthorized";
          f.signedIn = true;
        }
        if (["login", "resume"].includes(op))
          return {
            id: "user-1",
            email: "tester@example.invalid",
            role: f.role,
            session_id: "session-1",
          };
        if (op === "verify") {
          if (a.token !== "012345" || a.email !== "tester@example.invalid")
            throw "HTTP 400";
          f.verified = true;
          return {};
        }
        if (op === "devices") return f.devices;
        if (op === "bind") {
          if (f.bindFailure) throw f.bindFailure;
          local.name = a.name;
          f.bound = true;
          return { id: local.id };
        }
        if (op === "rename_device") {
          f.devices.find((d) => d.id === a.id).name = a.name;
          return {};
        }
        if (op === "admin_users")
          return [
            {
              id: "user-1",
              email: "tester@example.invalid",
              enabled: true,
              verified: true,
              role: "admin",
            },
          ];
        if (op === "admin_registration") return { value: "open" };
        if (op === "admin_sessions")
          return [
            {
              id: "admin-session",
              user_id: "user-1",
              created_at: "2026-09-29T00:00:00Z",
              revoked: false,
            },
          ];
        if (op === "admin_audit")
          return [
            {
              id: 1,
              actor_id: "user-1",
              object_id: null,
              action: "registration_policy",
              result: "open",
              created_at: "2026-09-29T00:00:00Z",
            },
          ];
        if (op === "admin_invite")
          return { id: "signup-1", code: "synthetic-signup" };
        if (op === "remote_sessions") return f.requests;
        if (op === "pending") return f.pending;
        if (op === "sessions") return f.sessions;
        if (op === "revoke_session") {
          f.sessions = f.sessions.filter((s) => s.id !== a.id);
          return {};
        }
        if (op === "invite")
          return { id: "invite-1", code: "synthetic-invitation" };
        if (op === "request") {
          const r = {
            id: "request-" + (f.requests.length + 1),
            source_device_id: a.source_device_id,
            target_device_id: a.target_device_id,
            permission: a.permission,
            state: "pending",
          };
          f.requests.unshift(r);
          return r;
        }
        if (op === "revoke_remote") {
          delete f.connected[a.id];
          f.requests.find((r) => r.id === a.id).state = "revoked";
          return {};
        }
        if (op === "decide") {
          f.pending = f.pending.filter((p) => p.id !== a.id);
          return {};
        }
        if (op === "logout" || op === "password") {
          f.transportRunning = false;
          f.sharePreferences = {
            sharing: false,
            watch: false,
            restore: "idle",
          };
          f.signedIn = false;
          f.sharing = false;
          f.connected = {};
          return {};
        }
        return {};
      },
    };
  });
  await page.goto("http://127.0.0.1:1420/");
  await page.getByRole("button", { name: "创建账号", exact: true }).click();
  await page
    .getByLabel("邮箱地址", { exact: true })
    .fill("tester@example.invalid");
  await page.getByLabel("密码", { exact: true }).fill("synthetic-password-123");
  await page
    .getByRole("button", { name: "注册并验证邮箱", exact: true })
    .click();
  await page.getByLabel("邮箱验证码", { exact: true }).waitFor();
  await page.getByLabel("邮箱验证码", { exact: true }).fill("012345");
  await page
    .getByRole("button", { name: "没收到验证码？重新发送", exact: true })
    .click();
  await page
    .getByText("重发请求已提交，请检查收件箱和垃圾邮件。", { exact: true })
    .waitFor();
  assert.equal(
    await page.getByLabel("邮箱验证码", { exact: true }).inputValue(),
    "012345",
  );
  assert.equal(
    await page.locator('input[type="email"], input[type="password"]').count(),
    0,
  );
  const resend = await page.evaluate(() =>
    window.fixture.calls.findLast(
      (c) => c.cmd === "call" && c.args.op === "resend",
    ),
  );
  assert.deepEqual(resend.args.args, {
    email: "tester@example.invalid",
    password: "synthetic-password-123",
  });
  await page.evaluate(() => (window.fixture.failNextOp = "resend"));
  await page
    .getByRole("button", { name: "没收到验证码？重新发送", exact: true })
    .click();
  await page.getByRole("alertdialog").filter({ hasText: "HTTP 503" }).waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert.equal(
    await page.getByLabel("邮箱验证码", { exact: true }).inputValue(),
    "012345",
  );
  assert.equal(
    await page.locator('input[type="email"], input[type="password"]').count(),
    0,
  );
  await page.getByLabel("邮箱验证码", { exact: true }).fill("bad-code");
  await page.getByRole("button", { name: "完成验证", exact: true }).click();
  await page.getByRole("alertdialog").filter({ hasText: "HTTP 400" }).waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert(await page.getByLabel("邮箱验证码", { exact: true }).isVisible());
  await page.getByLabel("邮箱验证码", { exact: true }).fill("012345");
  await page.getByRole("button", { name: "完成验证", exact: true }).click();
  await page
    .getByRole("heading", { name: "登录 FarSail", exact: true })
    .waitFor();
  await page.evaluate(() => (window.fixture.failLogin = true));
  await page.getByLabel("密码", { exact: true }).fill("synthetic-password-123");
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await page.getByRole("dialog", { name: "登录未成功" }).waitFor();
  assert(
    (await page.getByRole("dialog").innerText()).includes("邮箱或密码不正确"),
  );
  await page.getByRole("button", { name: "返回登录", exact: true }).click();
  await page.evaluate(() => (window.fixture.failLogin = false));
  for (const [code, title, message] of [
    ["account_not_found", "账号不存在", "这个邮箱尚未注册"],
    ["invalid_password", "密码错误", "密码不正确"],
    ["email_not_verified", "邮箱尚未验证", "请先完成邮箱验证"],
    ["account_disabled", "账号已停用", "请联系管理员"],
  ]) {
    await page.evaluate((code) => (window.fixture.loginFailure = code), code);
    await page.getByRole("button", { name: "登录", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: title, exact: true });
    await dialog.waitFor();
    assert((await dialog.innerText()).includes(message));
    assert.equal(await page.getByRole("alertdialog").count(), 0);
    await page.getByRole("button", { name: "返回登录", exact: true }).click();
  }
  await page.evaluate(() => (window.fixture.loginFailure = ""));
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await page.getByRole("heading", { name: "工作电脑", exact: true }).waitFor();
  assert.equal(
    await page.getByRole("button", { name: "远程控制", exact: true }).count(),
    0,
  );
  await page.getByRole("button", { name: "开启本机共享", exact: true }).click();
  await page
    .getByRole("button", { name: "停止本机共享", exact: true })
    .waitFor();
  assert.equal(await page.evaluate(() => window.fixture.sharing), true);
  assert.equal(await page.evaluate(() => window.fixture.remoteWatch), false);
  // Startup intent must never be rendered as effective sharing. Opt-outs remain usable.
  await page.evaluate(() => {
    window.fixture.sharing = false;
    window.fixture.remoteWatch = false;
    window.fixture.sharePreferences = {
      sharing: true,
      watch: true,
      restore: "pending",
    };
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByText(/尚未开启共享/).waitFor();
  assert(
    await page
      .getByRole("button", { name: "关闭远程值守", exact: true })
      .isEnabled(),
  );
  await page.getByRole("button", { name: "关闭远程值守", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.fixture.sharePreferences.watch),
    false,
  );
  await page.evaluate(() => {
    window.fixture.sharePreferences.restore = "failed";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByText(/本次未恢复共享/).waitFor();
  const beforeRetry = await page.evaluate(() => window.fixture.calls.length);
  await page.getByRole("button", { name: "重试本机共享", exact: true }).click();
  await page.getByText("本机共享已恢复", { exact: true }).waitFor();
  const retry = await page.evaluate(
    (n) => window.fixture.calls.slice(n),
    beforeRetry,
  );
  assert(
    !retry.some((x) => x.cmd === "transport_start"),
    "reuse native restored transport",
  );
  assert(retry.some((x) => x.cmd === "share_enable"));
  await page.evaluate(() => {
    window.fixture.sharing = false;
    window.fixture.sharePreferences.restore = "pending";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "停止本机共享", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.fixture.sharePreferences.sharing),
    false,
  );
  await page.getByRole("button", { name: "开启本机共享", exact: true }).click();
  await page.getByRole("button", { name: "开启远程值守", exact: true }).click();
  await page
    .getByRole("button", { name: "关闭远程值守", exact: true })
    .waitFor();
  assert.equal(await page.evaluate(() => window.fixture.remoteWatch), true);
  await page.getByRole("button", { name: "关闭远程值守", exact: true }).click();
  await page
    .getByRole("button", { name: "开启远程值守", exact: true })
    .waitFor();
  fs.mkdirSync(".local/ui-verification", { recursive: true });
  await page.screenshot({
    path: ".local/ui-verification/local-device.png",
    fullPage: true,
  });
  await page.getByRole("button").filter({ hasText: "备用电脑" }).click();
  assert(
    await page
      .getByRole("button", { name: "远程控制", exact: true })
      .isDisabled(),
  );
  await page.getByRole("button").filter({ hasText: "家里的电脑" }).click();
  await page.screenshot({
    path: ".local/ui-verification/remote-device.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "远程控制", exact: true }).click();
  await page.getByRole("button", { name: "取消连接", exact: true }).waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((x) => x.cmd === "transport_connect")
          .length,
    ),
    0,
  );
  await page.getByRole("button", { name: "取消连接", exact: true }).click();
  await page.getByRole("button", { name: "仅查看屏幕", exact: true }).click();
  await page.getByRole("button", { name: "取消连接", exact: true }).waitFor();
  await page.evaluate(() => (window.fixture.requests[0].state = "approved"));
  await page.waitForFunction(() =>
    window.fixture.calls.some((x) => x.cmd === "viewer_open"),
  );
  const connections = await page.evaluate(() =>
    window.fixture.calls.filter((x) => x.cmd === "transport_connect"),
  );
  assert.equal(connections.length, 1);
  assert.equal(connections[0].args.permission, "view");
  const assertShell = async () => {
    const overflow = await page.evaluate(() => {
      const root = document.documentElement;
      const main = document.querySelector("main");
      return {
        horizontal: root.scrollWidth > innerWidth,
        vertical: root.scrollHeight > innerHeight,
        main: main.scrollHeight > main.clientHeight + 1,
      };
    });
    assert.deepEqual(overflow, {
      horizontal: false,
      vertical: false,
      main: false,
    });
  };
  await assertShell();
  await page.getByRole("button", { name: /我的设备/ }).click();
  await page.evaluate(() => {
    const f = window.fixture;
    f.devices.push(
      ...Array.from({ length: 40 }, (_, i) => ({
        ...f.devices[1],
        id: `many-${i}`,
        name: `测试电脑 ${i + 1}`,
      })),
    );
    f.requests.push(
      ...Array.from({ length: 45 }, (_, i) => ({
        id: `many-request-${i}`,
        source_device_id: "local-0001",
        target_device_id: "remote-0002",
        permission: i % 2 ? "control" : "view",
        state: ["pending", "approved", "denied", "revoked", "expired"][i % 5],
      })),
    );
    f.pending = [
      {
        id: "incoming-1",
        source_device_id: "remote-0002",
        source_device_name: "家里的电脑",
        requester_email: "tester@example.invalid",
        permission: "control",
      },
    ];
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page
    .getByRole("button")
    .filter({ hasText: "测试电脑 40" })
    .waitFor({ state: "attached" });
  await assertShell();
  assert(
    await page
      .locator(".device-list")
      .evaluate((el) => el.scrollHeight > el.clientHeight),
  );
  const headerBefore = await page.locator(".topbar").boundingBox();
  await page.locator(".device-list").evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  assert.deepEqual(await page.locator(".topbar").boundingBox(), headerBefore);
  await page.getByRole("button").filter({ hasText: "测试电脑 40" }).click();
  await page
    .getByRole("heading", { name: "测试电脑 40", exact: true })
    .waitFor();
  await page.screenshot({ path: ".local/ui-verification/many-devices.png" });

  await page.getByRole("button", { name: "远程连接", exact: true }).click();
  await page
    .getByRole("heading", { name: "谁想连接这台电脑", exact: true })
    .waitFor();
  await page.getByRole("button", { name: "拒绝", exact: true }).click();
  await page
    .getByText("目前没有等待本机处理的请求。", { exact: true })
    .waitFor();
  await page.getByRole("button", { name: /^当前连接/ }).click();
  assert(
    (await page.locator(".connection-list-pane:visible").innerText()).includes(
      "工作电脑（本机） → 家里的电脑",
    ),
  );
  assert(
    (await page.locator(".connection-list-pane:visible").innerText()).includes(
      "仅查看屏幕 · 已批准",
    ),
  );
  assert.equal(
    await page.getByText("view · approved", { exact: true }).count(),
    0,
  );
  await assertShell();
  assert(
    await page
      .locator(".connection-list-pane:visible .connection-list")
      .evaluate((el) => el.scrollHeight > el.clientHeight),
  );
  await page.screenshot({ path: ".local/ui-verification/connections.png" });
  await page.getByRole("button", { name: "连接记录", exact: true }).click();
  assert(
    (await page.locator(".connection-list-pane:visible").innerText()).includes(
      "已拒绝",
    ),
  );
  assert.equal(
    await page
      .getByRole("button", { name: "结束本次连接", exact: true })
      .count(),
    0,
  );

  await page.getByRole("button", { name: "连接他人电脑", exact: true }).click();
  await page.getByLabel("目标设备 ID", { exact: true }).fill("remote-0002");
  await page
    .getByLabel("连接后可以做什么", { exact: true })
    .selectOption("control");
  await page.getByRole("button", { name: "发送连接请求", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "连接请求已发送" })
    .waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.requests[0].permission),
    "control",
  );
  await page.getByRole("button", { name: "邀请他人连接", exact: true }).click();
  await page
    .getByLabel("允许对方连接哪台电脑", { exact: true })
    .selectOption("local-0001");
  // Invitation permissions must be explicit and independent from the other form.
  assert.equal(
    await page.getByLabel("允许对方做什么", { exact: true }).inputValue(),
    "view",
  );
  await page.getByRole("button", { name: "生成邀请码", exact: true }).click();
  await page.getByText("synthetic-invitation", { exact: true }).waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((x) => x.args.op === "invite").at(-1).args
          .args.permission,
    ),
    "view",
  );
  await page
    .getByLabel("允许对方连接哪台电脑", { exact: true })
    .selectOption("remote-0002");
  assert(
    (await page.locator(".invite-code").innerText()).includes("local-0001"),
  );
  await page.screenshot({ path: ".local/ui-verification/invitation.png" });
  await page.getByRole("button", { name: "复制连接信息", exact: true }).click();
  assert(
    (await page.evaluate(() => window.fixture.clipboard)).includes(
      "local-0001",
    ),
  );
  await page.getByRole("button", { name: "收起邀请码", exact: true }).click();
  assert(
    await page
      .getByRole("button", { name: "取消邀请", exact: true })
      .isVisible(),
  );
  await page
    .getByText("邀请码已收起，收起不会取消邀请", { exact: true })
    .waitFor();
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.getByRole("button", { name: "远程连接", exact: true }).click();
  await page.getByRole("button", { name: "邀请他人连接", exact: true }).click();
  await page
    .getByText("邀请码已收起，收起不会取消邀请", { exact: true })
    .waitFor();
  assert.equal(
    await page.getByLabel("允许对方连接哪台电脑", { exact: true }).inputValue(),
    "local-0001",
  );
  await page.getByRole("button", { name: "显示邀请码", exact: true }).click();
  await page.getByRole("button", { name: "取消邀请", exact: true }).click();
  await page
    .getByText("synthetic-invitation", { exact: true })
    .waitFor({ state: "hidden" });

  await page.getByRole("button", { name: "账号安全", exact: true }).click();
  await page.getByText("当前客户端（正在使用）", { exact: true }).waitFor();
  await assertShell();
  await page.screenshot({ path: ".local/ui-verification/login-records.png" });
  await page.getByRole("button", { name: "退出此登录", exact: true }).click();
  await page
    .getByText("其他客户端登录", { exact: true })
    .waitFor({ state: "hidden" });
  assert.equal(await page.evaluate(() => window.fixture.sessions.length), 1);

  for (const size of [
    { width: 1120, height: 760 },
    { width: 900, height: 580 },
    { width: 680, height: 700 },
  ]) {
    await page.setViewportSize(size);
    assert(
      await page
        .getByRole("status", { name: "本机屏幕共享中", exact: true })
        .isVisible(),
      "compact desktop sidebar must retain sharing status",
    );
    for (const name of [
      "总览",
      "我的设备",
      "远程连接",
      "账号安全",
      "共享与设置",
    ]) {
      await page
        .getByRole("navigation", { name: "主导航" })
        .getByRole("button", { name, exact: true })
        .click();
      await assertShell();
      if (["总览", "我的设备", "远程连接"].includes(name)) {
        assert.equal(
          await page
            .locator(".workspace-content")
            .evaluate((el) => el.scrollHeight > el.clientHeight + 1),
          false,
          `${name} should scroll only inside its panels`,
        );
      }
      if (size.width === 1120 && ["总览", "共享与设置"].includes(name)) {
        await page.screenshot({
          path: `.local/ui-verification/${name === "总览" ? "overview" : "settings"}-fit.png`,
        });
      }
    }
  }
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.setViewportSize({ width: 680, height: 700 });
  await page.screenshot({
    path: ".local/ui-verification/narrow.png",
    fullPage: true,
  });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  await page.setViewportSize({ width: 360, height: 580 });
  await page.getByRole("button", { name: "远程连接", exact: true }).click();
  await assertShell();
  assert(
    await page
      .locator(".connection-list-pane:visible .connection-list")
      .evaluate((el) => el.clientHeight >= 100),
    "compact window must retain usable list space",
  );
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  await page.screenshot({
    path: ".local/ui-verification/compact-connections.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1120, height: 760 });
  await page.getByRole("button", { name: "账号安全", exact: true }).click();
  await page.getByRole("button", { name: "退出登录", exact: true }).click();
  await page
    .getByRole("heading", { name: "登录 FarSail", exact: true })
    .waitFor();
  assert.equal(
    await page.getByRole("button", { name: "远程画面", exact: true }).count(),
    0,
  );
  await page.getByRole("button", { name: "找回密码", exact: true }).click();
  await page
    .getByLabel("邮箱地址", { exact: true })
    .fill("tester@example.invalid");
  await page.evaluate(() => {
    window.fixture.failNextOp = "recover_request";
  });
  await page
    .getByRole("button", { name: "发送密码重置邮件", exact: true })
    .click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "操作未完成" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert(
    await page
      .getByRole("heading", { name: "找回密码", exact: true })
      .isVisible(),
  );
  await page
    .getByRole("button", { name: "发送密码重置邮件", exact: true })
    .click();
  await page
    .getByLabel("邮件中的重置码", { exact: true })
    .fill("synthetic-reset");
  await page
    .getByLabel("新密码", { exact: true })
    .fill("synthetic-password-new");
  await page.evaluate(() => {
    window.fixture.failNextOp = "recover_complete";
  });
  await page
    .getByRole("button", { name: "保存新密码并返回登录", exact: true })
    .click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "操作未完成" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert(await page.getByLabel("邮件中的重置码", { exact: true }).isVisible());
  await page
    .getByRole("button", { name: "保存新密码并返回登录", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "登录 FarSail", exact: true })
    .waitFor();
  const loginAgain = async () => {
    await page
      .getByLabel("邮箱地址", { exact: true })
      .fill("tester@example.invalid");
    await page
      .getByLabel("密码", { exact: true })
      .fill("synthetic-password-new");
    await page.getByRole("button", { name: "登录", exact: true }).click();
    await page
      .getByRole("heading", { name: "工作电脑", exact: true })
      .waitFor();
  };
  await loginAgain();
  await page.getByRole("button", { name: "共享与设置", exact: true }).click();
  const beforeSharing = await page.evaluate(() => window.fixture.calls.length);
  await page.getByRole("switch", { name: "本机屏幕共享", exact: true }).check();
  await page.waitForFunction(() => window.fixture.sharing);
  const sharingCommands = await page.evaluate(
    (from) => window.fixture.calls.slice(from).map((x) => x.cmd),
    beforeSharing,
  );
  assert(
    sharingCommands.indexOf("transport_start") >= 0 &&
      sharingCommands.indexOf("transport_start") <
        sharingCommands.indexOf("share_enable"),
  );
  assert.equal(
    await page.getByLabel("中继地址", { exact: true }).isVisible(),
    false,
  );
  await page.screenshot({ path: ".local/ui-verification/sharing-actions.png" });
  await page
    .getByRole("switch", { name: "本机屏幕共享", exact: true })
    .uncheck();
  await page.waitForFunction(() => !window.fixture.sharing);
  assert.equal(await page.evaluate(() => window.fixture.sharing), false);
  await page.evaluate(() => {
    window.fixture.requests = [
      {
        id: "late-approved",
        source_device_id: "local-0001",
        target_device_id: "remote-0002",
        permission: "view",
        state: "approved",
      },
    ];
  });
  await page.getByRole("button", { name: "账号安全", exact: true }).click();
  await page.getByRole("button", { name: "退出登录", exact: true }).click();
  await page
    .getByRole("heading", { name: "登录 FarSail", exact: true })
    .waitFor();
  await loginAgain();
  await page.getByRole("button", { name: "远程连接", exact: true }).click();
  const approvedConnect = page.getByRole("button", {
    name: "连接并查看",
    exact: true,
  });
  assert(await approvedConnect.isEnabled());
  await page.evaluate(() => {
    window.fixture.failTransport = true;
  });
  await approvedConnect.click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "无法连接中继服务器" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert(await approvedConnect.isEnabled());
  await page.evaluate(() => {
    window.fixture.failTransport = false;
  });
  await approvedConnect.click();
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (x) => x.cmd === "viewer_open" && x.args.id === "late-approved",
    ),
  );
  await page.evaluate(() => {
    window.fixture.failNextOp = "revoke_remote";
  });
  await page.getByRole("button", { name: "结束本次连接", exact: true }).click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "操作未完成" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert(
    await page
      .getByRole("heading", { name: "远程连接", exact: true })
      .isVisible(),
  );
  await page.getByRole("button", { name: "结束本次连接", exact: true }).click();
  await page.getByRole("heading", { name: "远程连接", exact: true }).waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.requests[0].state),
    "revoked",
  );
  assert.equal(
    await page.getByRole("button", { name: "远程画面", exact: true }).count(),
    0,
  );

  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.getByText("设备信息与管理", { exact: true }).click();
  await page.getByRole("button", { name: "修改设备名称", exact: true }).click();
  await page.getByLabel("新的设备名称", { exact: true }).fill("不应保存的名称");
  await page.getByRole("button", { name: "取消改名", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "工作电脑",
  );
  await page.evaluate(() => {
    window.fixture.failClipboard = true;
  });
  await page.getByRole("button", { name: "复制设备 ID", exact: true }).click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "无法自动复制" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  await page.evaluate(() => {
    window.fixture.failClipboard = false;
  });
  await page.getByRole("button", { name: "复制设备 ID", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "设备 ID 已复制" })
    .waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.clipboard),
    "local-0001",
  );
  await page
    .getByRole("button", { name: "从账号移除设备", exact: true })
    .click();
  await page
    .getByRole("alertdialog", { name: "从账号移除设备？" })
    .getByRole("button", { name: "取消", exact: true })
    .click();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((x) => x.args.op === "unbind_device")
          .length,
    ),
    0,
  );

  await page.evaluate(() => {
    window.fixture.role = "admin";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "管理控制台", exact: true }).click();
  await page.getByText("修改注册方式 · 开放注册", { exact: true }).waitFor();
  assert.equal(await page.locator("pre.meta-row").count(), 0);
  await page
    .getByLabel("受邀邮箱", { exact: true })
    .fill("invited@example.invalid");
  await page
    .getByRole("button", { name: "生成注册邀请码", exact: true })
    .click();
  await page.getByRole("button", { name: "收起邀请码", exact: true }).click();
  assert(
    await page
      .getByRole("button", { name: "取消注册邀请", exact: true })
      .isVisible(),
  );
  await page.getByRole("button", { name: "复制注册邀请", exact: true }).click();
  assert(
    (await page.evaluate(() => window.fixture.clipboard)).includes(
      "synthetic-signup",
    ),
  );
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.getByRole("button", { name: "管理控制台", exact: true }).click();
  await page
    .getByText("邀请码已收起，收起不会取消邀请", { exact: true })
    .waitFor();
  assert.equal(
    await page.getByLabel("受邀邮箱", { exact: true }).inputValue(),
    "invited@example.invalid",
  );
  await page.getByRole("button", { name: "取消注册邀请", exact: true }).click();
  await page.screenshot({ path: ".local/ui-verification/admin-actions.png" });
  await page.locator(".page-admin").evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await page.screenshot({ path: ".local/ui-verification/admin-records.png" });
  await page
    .getByRole("button", { name: "生成注册邀请码", exact: true })
    .click();
  await page.getByText("synthetic-signup", { exact: true }).waitFor();
  await page.getByRole("button", { name: "账号安全", exact: true }).click();
  await page
    .getByLabel("当前密码", { exact: true })
    .fill("synthetic-password-new");
  await page.getByLabel("新密码").fill("synthetic-password-next");
  await page
    .getByRole("button", { name: "修改密码并退出所有登录", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "登录 FarSail", exact: true })
    .waitFor();
  assert.equal(
    await page.getByRole("button", { name: "远程画面", exact: true }).count(),
    0,
  );
  await loginAgain();
  await page.getByRole("button", { name: "管理控制台", exact: true }).click();
  assert.equal(
    await page
      .getByRole("button", { name: "取消注册邀请", exact: true })
      .count(),
    0,
  );
  // System-name defaults, legacy migration and custom aliases are separate paths.
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "工作电脑",
  );
  await page.evaluate(() => {
    window.fixture.bound = false;
    window.fixture.computerName = "DESKTOP-测试";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "总览", exact: true }).click();
  assert.equal(
    await page.getByLabel("本机设备名称", { exact: true }).inputValue(),
    "DESKTOP-测试",
  );
  await page.getByRole("button", { name: "添加这台电脑", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "这台电脑已添加" })
    .waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "DESKTOP-测试",
  );
  await page.evaluate(() => {
    window.fixture.devices[0].name = "这台 Windows 电脑";
    window.fixture.devices[1].name = "这台 Windows 电脑";
  });
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page
    .getByRole("heading", { name: "DESKTOP-测试", exact: true })
    .waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[1].name),
    "这台 Windows 电脑",
  );
  const migrations = await page.evaluate(
    () =>
      window.fixture.calls.filter(
        (c) =>
          c.args.op === "rename_device" && c.args.args.name === "DESKTOP-测试",
      ).length,
  );
  assert.equal(migrations, 1);
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter(
          (c) =>
            c.args.op === "rename_device" &&
            c.args.args.name === "DESKTOP-测试",
        ).length,
    ),
    migrations,
  );
  await page.screenshot({ path: ".local/ui-verification/computer-name.png" });

  await page.evaluate(() => {
    window.fixture.devices[0].id = "local-migration-failure";
    window.fixture.devices[0].name = "这台 Windows 电脑";
    window.fixture.failNextOp = "rename_device";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page
    .getByRole("alertdialog")
    .filter({ hasText: "未能同步本机的计算机名称" })
    .waitFor();
  await page
    .getByRole("alertdialog")
    .getByLabel("关闭错误", { exact: true })
    .click();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "这台 Windows 电脑",
  );
  await page.getByText("设备信息与管理", { exact: true }).click();
  await page.getByRole("button", { name: "修改设备名称", exact: true }).click();
  await page.getByLabel("新的设备名称", { exact: true }).fill("自定义工作站");
  await page.getByRole("button", { name: "保存名称", exact: true }).click();
  await page
    .getByRole("heading", { name: "自定义工作站", exact: true })
    .waitFor();
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "自定义工作站",
  );

  await page.evaluate(() => {
    window.fixture.bound = false;
    window.fixture.computerName = null;
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "总览", exact: true }).click();
  assert.equal(
    await page.getByLabel("本机设备名称", { exact: true }).inputValue(),
    "",
  );
  assert(
    await page
      .getByRole("button", { name: "添加这台电脑", exact: true })
      .isDisabled(),
  );
  await page.getByLabel("本机设备名称", { exact: true }).fill("手动名称");
  await page.getByRole("button", { name: "添加这台电脑", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "这台电脑已添加" })
    .waitFor();
  assert.equal(
    await page.evaluate(() => window.fixture.devices[0].name),
    "手动名称",
  );
  // Ubuntu uses the same account/device flow, but must not offer Windows host actions.
  await page.evaluate(() => {
    window.fixture.platform = "linux";
    window.fixture.canShareLocalScreen = false;
    window.fixture.devices[0].platform = "linux";
    window.fixture.devices[1].name = "家里的电脑";
    window.fixture.devices[1].online = true;
    window.fixture.devices[1].enabled = true;
    window.fixture.devices[1].can_host = true;
    window.fixture.sharing = false;
    window.fixture.sharePreferences = {
      sharing: false,
      watch: false,
      restore: "idle",
    };
  });
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "共享与设置", exact: true }).click();
  await page
    .getByText(
      "Ubuntu 客户端可连接 Windows 电脑，暂不支持共享本机屏幕或远程值守。",
      { exact: true },
    )
    .waitFor();
  assert.equal(
    await page.getByRole("switch", { name: "本机屏幕共享" }).count(),
    0,
  );
  assert.equal(
    await page.getByText("Windows 运行权限", { exact: true }).count(),
    0,
  );
  await page.getByRole("button", { name: "总览", exact: true }).click();
  await page.getByText("Ubuntu 控制端", { exact: true }).first().waitFor();
  await page.screenshot({ path: ".local/ui-verification/ubuntu-overview.png" });
  await page.getByRole("button", { name: "我的设备", exact: true }).click();
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^手动名称/ })
    .click();
  assert.equal(
    await page
      .getByRole("button", { name: "开启本机共享", exact: true })
      .count(),
    0,
  );
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^家里的电脑/ })
    .click();
  assert(
    await page
      .getByRole("button", { name: "远程控制", exact: true })
      .isEnabled(),
  );
  await page.screenshot({ path: ".local/ui-verification/ubuntu-devices.png" });
  await page.evaluate(() => {
    window.fixture.bound = false;
    window.fixture.bindFailure =
      "HTTP 422: Failed to deserialize: platform: unknown variant `linux`, expected windows/android/ios";
  });
  await page.getByRole("button", { name: "总览", exact: true }).click();
  await page.getByRole("button", { name: "刷新状态", exact: true }).click();
  await page.getByLabel("本机设备名称", { exact: true }).fill("Ubuntu-测试");
  await page.getByRole("button", { name: "添加这台电脑", exact: true }).click();
  const upgradeDialog = page.getByRole("alertdialog");
  await upgradeDialog.waitFor();
  assert((await upgradeDialog.innerText()).includes("请先升级协调服务"));
  assert.equal(await page.evaluate(() => window.fixture.bound), false);
  await upgradeDialog
    .getByRole("button", { name: "知道了", exact: true })
    .click();
  // Updates stay in settings; a rejected download cannot enable installation.
  await page.getByRole("button", { name: "共享与设置", exact: true }).click();
  const updates = page.locator(".client-updates");
  await updates.getByRole("heading", { name: "软件更新" }).waitFor();
  await updates
    .getByRole("checkbox", { name: "自动下载，并在远程连接结束后安装更新" })
    .check();
  await page.waitForFunction(
    () => window.fixture.update.preferences.autoInstall,
  );
  await updates.getByRole("button", { name: "检查更新", exact: true }).click();
  await updates.getByText("发现新版本 0.1.21。", { exact: true }).waitFor();
  await page.evaluate(
    () => (window.fixture.updateFailure = "更新签名校验失败"),
  );
  await updates.getByRole("button", { name: "下载更新", exact: true }).click();
  await updates.getByText("更新签名校验失败", { exact: true }).waitFor();
  assert.equal(
    await updates.getByRole("button", { name: "安装并重启" }).count(),
    0,
  );
  await updates.getByRole("button", { name: "下载更新", exact: true }).click();
  await updates.getByRole("button", { name: "安装并重启" }).waitFor();
  await page.evaluate(() => (window.fixture.updateConnected = true));
  await updates.getByRole("button", { name: "安装并重启" }).click();
  await updates
    .getByText("更新已就绪，等待远程连接结束。请关闭远程窗口后安装。", {
      exact: true,
    })
    .waitFor();
  await page.screenshot({ path: ".local/ui-verification/client-updates.png" });
  await page.evaluate(() => (window.fixture.updateConnected = false));
  await updates.getByRole("button", { name: "安装并重启" }).click();
  await updates
    .getByText("正在安装更新，完成后将重新启动。", { exact: true })
    .waitFor();
  assert(await updates.getByRole("button", { name: "检查更新" }).isDisabled());
  assert.deepEqual(errors, []);
  console.log(
    "PASS update preferences, signed-download failure/retry, connected-session installation deferral; six-digit verification with email context; direct resend retains verification page/input on success and failure without credential fields; distinct login reasons and legacy fallback; Linux 422 upgrade guidance; existing UI/action regressions; computer-name binding; local-only legacy rename once; custom alias preservation; migration failure and manual recovery; unavailable hostname fallback. Synthetic IPC only.",
  );
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
