// Run with Playwright available on NODE_PATH and the desktop Vite server on 1420.
// Synthetic IPC fixtures verify UI flow only; they are never bundled into the app.
const { chromium } = require("playwright");
const assert = require("node:assert/strict");
const fs = require("node:fs");
(async () => {
  const browser = await chromium.launch({
    channel: process.env.UI_BROWSER || "chrome",
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
      sharing: false,
      requests: [],
      calls: [],
      devices: [local, remote, offline],
      pending: [],
    };
    const f = window.fixture;
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" } },
      invoke: async (cmd, args = {}) => {
        f.calls.push({ cmd, args });
        if (cmd === "state")
          return {
            signedIn: f.signedIn,
            server: "https://example.invalid",
            deviceId: f.signedIn ? local.id : null,
            sharing: f.sharing,
          };
        if (cmd === "transport_start" || cmd === "transport_connect") return {};
        if (cmd === "share_enable") {
          f.sharing = true;
          local.can_host = true;
          return {};
        }
        if (cmd === "share_disable") {
          f.sharing = false;
          local.can_host = false;
          return {};
        }
        if (cmd === "remote_status")
          return { displays: [], state: "connected", error: null };
        if (cmd === "media_next") return new Promise(() => {});
        if (cmd.startsWith("plugin:window|")) return {};
        if (cmd !== "call") return {};
        const { op, args: a = {} } = args;
        if (op === "login") {
          if (f.failLogin || !f.verified) throw "HTTP 401: unauthorized";
          f.signedIn = true;
        }
        if (["login", "resume"].includes(op))
          return {
            id: "user-1",
            email: "tester@example.invalid",
            role: "user",
            session_id: "session-1",
          };
        if (op === "verify") {
          if (a.token !== "synthetic-code") throw "HTTP 400";
          f.verified = true;
          return {};
        }
        if (op === "devices") return f.devices;
        if (op === "remote_sessions") return f.requests;
        if (op === "pending") return f.pending;
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
          f.requests.find((r) => r.id === a.id).state = "revoked";
          return {};
        }
        if (op === "decide") {
          f.pending = f.pending.filter((p) => p.id !== a.id);
          return {};
        }
        if (op === "logout") {
          f.signedIn = false;
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
  await page.getByLabel("邮箱验证码", { exact: true }).fill("bad-code");
  await page.getByRole("button", { name: "完成验证", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "HTTP 400" }).waitFor();
  assert(await page.getByLabel("邮箱验证码", { exact: true }).isVisible());
  await page.getByLabel("邮箱验证码", { exact: true }).fill("synthetic-code");
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
  await page.getByRole("button", { name: "登录", exact: true }).click();
  await page.getByRole("heading", { name: "工作电脑", exact: true }).waitFor();
  assert.equal(
    await page.getByRole("button", { name: "远程控制", exact: true }).count(),
    0,
  );
  await page.getByRole("button", { name: "开启本机共享", exact: true }).click();
  await page.getByRole("button", { name: "停止共享", exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.fixture.sharing), true);
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
  await page.waitForFunction(() => window.fixture.calls.some(x => x.cmd === "viewer_open"));
  const connections = await page.evaluate(() =>
    window.fixture.calls.filter((x) => x.cmd === "transport_connect"),
  );
  assert.equal(connections.length, 1);
  assert.equal(connections[0].args.permission, "view");
  await page.getByRole("button", { name: /我的设备/ }).click();
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
  assert.deepEqual(errors, []);
  console.log(
    "PASS registration -> verification -> login; 401 dialog; local sharing; offline guard; cancel; view approval -> exactly one connection; narrow layout. Synthetic IPC only.",
  );
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
