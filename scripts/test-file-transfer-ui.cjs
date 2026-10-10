// Synthetic IPC regression: no user files, credentials, or live server are used.
const { chromium, webkit } = require("playwright");
const assert = require("node:assert/strict");
const fs = require("node:fs");

(async () => {
  const engine = process.env.UI_ENGINE === "webkit" ? webkit : chromium;
  const browser = await engine.launch({
    ...(engine === chromium
      ? { channel: process.env.UI_BROWSER || "chrome" }
      : {}),
    ...(engine === webkit && process.env.UI_WEBKIT_EXECUTABLE
      ? { executablePath: process.env.UI_WEBKIT_EXECUTABLE }
      : {}),
    headless: true,
  });
  const page = await browser.newPage({
    viewport: { width: 1120, height: 760 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    const local = {
      id: "file-local",
      name: "本机测试电脑",
      platform: "linux",
      enabled: true,
      online: true,
      can_host: false,
      can_files: false,
      last_seen_at: null,
    };
    const remote = {
      ...local,
      id: "file-remote",
      name: "远端测试电脑",
      platform: "windows",
      can_host: true,
      can_files: true,
    };
    const unavailable = {
      ...local,
      id: "file-unavailable",
      name: "未接收文件电脑",
      can_host: true,
    };
    const offline = {
      ...remote,
      id: "file-offline",
      name: "离线电脑",
      online: false,
    };
    window.filesFixture = {
      signedIn: true,
      filesEnabled: false,
      transportRunning: false,
      devices: [local, remote, unavailable, offline],
      requests: [],
      pending: [],
      sessions: [],
      calls: [],
      chooseFile: false,
      chooseSave: false,
      nextFailure: "",
      readsFail: false,
    };
    const f = window.filesFixture;
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" } },
      invoke: async (command, args = {}) => {
        f.calls.push({ command, args });
        if (f.nextFailure === command) {
          f.nextFailure = "";
          throw "文件无法读取，请检查文件权限。";
        }
        if (command === "state")
          return {
            signedIn: f.signedIn,
            server: "https://example.invalid",
            deviceId: f.signedIn ? local.id : null,
            sharing: false,
            canShareLocalScreen: false,
            platform: "linux",
            filesEnabled: f.filesEnabled,
            computerName: "本机测试电脑",
            transportRunning: f.transportRunning,
          };
        if (command === "transport_start") {
          f.transportRunning = true;
          return {};
        }
        if (command === "files_enable") {
          f.filesEnabled = args.enabled;
          local.can_files = args.enabled;
          if (!args.enabled)
            f.sessions
              .filter((s) => s.host)
              .forEach((s) => {
                s.state = "closed";
              });
          return;
        }
        if (command === "transport_connect") {
          if (
            args.permission === "files" &&
            !f.sessions.some((s) => s.id === args.id)
          )
            f.sessions.push({
              id: args.id,
              host: false,
              state: "connected",
              path: "direct",
              transfers: [],
            });
          return { state: "direct", rtt_ms: 5 };
        }
        if (command === "files_status") {
          if (f.readsFail) throw "无法读取文件状态";
          return args.id
            ? f.sessions.find((s) => s.id === args.id)
            : { enabled: f.filesEnabled, sessions: f.sessions };
        }
        if (command === "files_send") {
          if (f.chooseFile)
            f.sessions
              .find((s) => s.id === args.id)
              .transfers.push({
                id: `send-${f.sessions.find((s) => s.id === args.id).transfers.length}`,
                name: "发送测试.txt",
                size: 1024,
                transferred: 0,
                direction: "send",
                state: "offered",
              });
          return;
        }
        if (
          ["files_accept", "files_reject", "files_cancel"].includes(command)
        ) {
          const transfer = f.sessions
            .find((s) => s.id === args.id)
            .transfers.find((t) => t.id === args.transferId);
          if (command === "files_accept" && f.chooseSave)
            transfer.state = "transferring";
          if (command === "files_reject") transfer.state = "rejected";
          if (command === "files_cancel") transfer.state = "cancelled";
          return;
        }
        if (command === "transport_status")
          return {
            state: f.sessions.some(
              (s) => s.id === args.id && s.state === "connected",
            )
              ? "direct"
              : "closed",
            rtt_ms: 5,
          };
        if (command === "remote_status") throw "Not a screen session";
        if (command === "viewer_open") return `viewer-${args.id}`;
        if (command === "viewer_window_active") return true;
        if (command === "call") {
          const a = args.args || {};
          if (args.op === "resume")
            return {
              id: "file-user",
              email: "files@example.invalid",
              role: "user",
              session_id: "file-login",
            };
          if (args.op === "devices") return f.devices;
          if (args.op === "remote_sessions") return f.requests;
          if (args.op === "pending") return f.pending;
          if (args.op === "request") {
            const request = {
              ...a,
              id: `request-${f.requests.length}`,
              state: "pending",
            };
            f.requests.push(request);
            return request;
          }
          if (args.op === "decide") {
            f.pending = f.pending.filter((p) => p.id !== a.id);
            const request = f.requests.find((r) => r.id === a.id);
            if (request) request.state = a.approve ? "approved" : "denied";
            return;
          }
          if (args.op === "revoke_remote") {
            f.requests.find((r) => r.id === a.id).state = "revoked";
            const session = f.sessions.find((s) => s.id === a.id);
            if (session) session.state = "closed";
            return;
          }
          if (args.op === "invite")
            return { id: "files-invite", code: "synthetic-file-invitation" };
          if (args.op === "sessions") return [];
        }
        return {};
      },
    };
  });
  const devicePanel = page.locator(".device-detail-pane");
  const nav = (name) =>
    page.locator("nav").getByRole("button", { name, exact: true });
  await page.goto("http://127.0.0.1:1420/");
  await nav("我的设备").click();
  // File receiving is a separate choice and never enables Linux screen sharing.
  await page.getByRole("switch", { name: "允许接收文件", exact: true }).check();
  await page.waitForFunction(() => window.filesFixture.filesEnabled);
  await page
    .locator(".sidebar-status")
    .getByText("本机屏幕未共享", { exact: true })
    .waitFor();
  await page
    .locator(".sidebar-status")
    .getByText("文件接收已开启", { exact: true })
    .waitFor();
  const enabling = await page.evaluate(() =>
    window.filesFixture.calls.filter((c) =>
      ["transport_start", "files_enable", "share_enable"].includes(c.command),
    ),
  );
  assert.deepEqual(
    enabling.map((c) => c.command),
    ["transport_start", "files_enable"],
  );
  assert.equal(enabling[0].args.forceRelay, false);
  // Offline and non-receiving targets cannot request files, independently of screen capability.
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^未接收文件电脑/ })
    .click();
  assert(
    await devicePanel
      .getByRole("button", { name: "文件传输", exact: true })
      .isDisabled(),
  );
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^离线电脑/ })
    .click();
  assert(
    await devicePanel
      .getByRole("button", { name: "文件传输", exact: true })
      .isDisabled(),
  );
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^远端测试电脑/ })
    .click();
  await devicePanel
    .getByRole("button", { name: "文件传输", exact: true })
    .click();
  await page.waitForFunction(() => window.filesFixture.requests.length === 1);
  assert.equal(
    await page.evaluate(() => window.filesFixture.requests[0].permission),
    "files",
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter(
          (c) => c.command === "transport_connect",
        ).length,
    ),
    0,
  );
  await page.evaluate(() => {
    window.filesFixture.requests[0].state = "approved";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page
    .getByRole("button", { name: "选择文件发送", exact: true })
    .waitFor();
  await page
    .getByText("已直连 · 文件直接在两台电脑之间传输", { exact: true })
    .waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter(
          (c) => c.command === "transport_connect",
        ).length,
    ),
    1,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter((c) => c.command === "viewer_open")
          .length,
    ),
    0,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.find((c) => c.command === "transport_connect")
          .args.permission,
    ),
    "files",
  );
  // A cancelled native open dialog creates no transfer; errors remain actionable.
  await page.getByRole("button", { name: "选择文件发送", exact: true }).click();
  assert.equal(await page.locator(".file-transfer").count(), 0);
  await page.evaluate(() => {
    window.filesFixture.nextFailure = "files_send";
  });
  await page.getByRole("button", { name: "选择文件发送", exact: true }).click();
  await page.getByRole("alertdialog").waitFor();
  assert(
    (await page.getByRole("alertdialog").innerText()).includes(
      "请检查文件权限",
    ),
  );
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "知道了", exact: true })
    .click();
  await page.evaluate(() => {
    window.filesFixture.chooseFile = true;
  });
  await page.getByRole("button", { name: "选择文件发送", exact: true }).click();
  await page.getByText(/等待对方接收/).waitFor();
  await page.getByRole("button", { name: "取消传输", exact: true }).click();
  await page.getByText(/发送 · 1.0 KB · 已取消/).waitFor();
  // Incoming basename metadata does not cause a write; save cancellation keeps the offer.
  await page.evaluate(() => {
    window.filesFixture.sessions[0].transfers.push({
      id: "receive-1",
      name: "接收测试.txt",
      size: 2048,
      transferred: 0,
      direction: "receive",
      state: "offered",
    });
  });
  const receiving = page.getByRole("article", {
    name: "接收 接收测试.txt",
    exact: true,
  });
  await receiving.getByRole("button", { name: "保存到…", exact: true }).click();
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.sessions[0].transfers.find(
          (t) => t.id === "receive-1",
        ).state,
    ),
    "offered",
  );
  await page.evaluate(() => {
    window.filesFixture.chooseSave = true;
  });
  await receiving.getByRole("button", { name: "保存到…", exact: true }).click();
  await receiving
    .getByRole("progressbar", { name: "接收测试.txt 传输进度", exact: true })
    .waitFor();
  await page.evaluate(() => {
    const t = window.filesFixture.sessions[0].transfers.find(
      (t) => t.id === "receive-1",
    );
    t.transferred = t.size;
    t.state = "completed";
    window.filesFixture.sessions[0].path = "relay";
  });
  await receiving.getByText(/传输完成/).waitFor();
  await page.getByText("已通过中继连接", { exact: true }).waitFor();
  await page.evaluate(() => {
    window.filesFixture.sessions[0].transfers.push({
      id: "receive-2",
      name: "拒绝测试.txt",
      size: 12,
      transferred: 0,
      direction: "receive",
      state: "offered",
    });
  });
  const reject = page.getByRole("article", {
    name: "接收 拒绝测试.txt",
    exact: true,
  });
  await reject.getByRole("button", { name: "拒绝", exact: true }).click();
  await reject.getByText(/已拒绝/).waitFor();
  await page.evaluate(() => {
    window.filesFixture.sessions[0].transfers.push({
      id: "failed-1",
      name: "失败测试.txt",
      size: 4096,
      transferred: 1024,
      direction: "send",
      state: "failed",
      error: "连接已中断，请重新发送文件。",
    });
  });
  await page
    .getByText("连接已中断，请重新发送文件。", { exact: true })
    .waitFor();
  const commandArgs = await page.evaluate(() =>
    window.filesFixture.calls.filter((c) => c.command === "files_accept"),
  );
  assert.deepEqual(Object.keys(commandArgs[0].args).sort(), [
    "id",
    "transferId",
  ]);
  fs.mkdirSync(".local/ui-verification", { recursive: true });
  await page.screenshot({ path: ".local/ui-verification/file-transfer.png" });
  await page.setViewportSize({ width: 660, height: 600 });
  assert(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  );
  await page.screenshot({
    path: ".local/ui-verification/file-transfer-narrow.png",
  });
  await page.setViewportSize({ width: 1120, height: 760 });
  // Incoming Files approval remains available on Linux without screen sharing.
  await nav("我的设备").click();
  await page.evaluate(() => {
    const f = window.filesFixture;
    f.filesEnabled = false;
    f.devices[0].can_files = false;
    f.pending = [
      {
        id: "host-request",
        source_device_name: "远端测试电脑",
        requester_email: "files@example.invalid",
        permission: "files",
      },
    ];
    f.requests.push({
      id: "host-request",
      source_device_id: "file-remote",
      target_device_id: "file-local",
      permission: "files",
      state: "pending",
    });
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^本机测试电脑/ })
    .click();
  assert(
    await page
      .getByRole("button", { name: "允许文件传输", exact: true })
      .isDisabled(),
  );
  await page.getByRole("switch", { name: "允许接收文件", exact: true }).check();
  await page.getByRole("button", { name: "允许文件传输", exact: true }).click();
  await page.getByRole("heading", { name: "文件传输", exact: true }).waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter((c) => c.command === "share_enable")
          .length,
    ),
    0,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter(
          (c) =>
            c.command === "transport_connect" && c.args.id === "host-request",
        ).length,
    ),
    0,
  );
  await page.evaluate(() => {
    window.filesFixture.sessions.push({
      id: "host-request",
      host: true,
      state: "connected",
      path: "direct",
      transfers: [],
    });
  });
  await page
    .getByText("已直连 · 文件直接在两台电脑之间传输", { exact: true })
    .waitFor();
  // Invites can target a Linux file receiver whose screen-host capability is false.
  await nav("远程连接").click();
  await page.getByRole("button", { name: "邀请他人连接", exact: true }).click();
  await page
    .getByLabel("允许对方做什么", { exact: true })
    .selectOption("files");
  await page
    .getByLabel("允许对方连接哪台电脑", { exact: true })
    .selectOption("file-local");
  await page.getByRole("button", { name: "生成邀请码", exact: true }).click();
  await page.getByText("synthetic-file-invitation", { exact: true }).waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.findLast(
          (c) => c.command === "call" && c.args.op === "invite",
        ).args.args.permission,
    ),
    "files",
  );
  // View/control still open the viewer and never acquire Files permission.
  await nav("我的设备").click();
  await page
    .locator(".device-list")
    .getByRole("button", { name: /^远端测试电脑/ })
    .click();
  await devicePanel
    .getByRole("button", { name: "仅查看屏幕", exact: true })
    .click();
  await page.waitForFunction(() =>
    window.filesFixture.requests.some((r) => r.permission === "view"),
  );
  await page.evaluate(() => {
    window.filesFixture.requests.find((r) => r.permission === "view").state =
      "approved";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.waitForFunction(() =>
    window.filesFixture.calls.some((c) => c.command === "viewer_open"),
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.findLast(
          (c) => c.command === "transport_connect",
        ).args.permission,
    ),
    "view",
  );
  await devicePanel
    .getByRole("button", { name: "远程控制", exact: true })
    .click();
  await page.waitForFunction(() =>
    window.filesFixture.requests.some((r) => r.permission === "control"),
  );
  await page.evaluate(() => {
    window.filesFixture.requests.find((r) => r.permission === "control").state =
      "approved";
  });
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.waitForFunction(
    () =>
      window.filesFixture.calls.filter((c) => c.command === "viewer_open")
        .length === 2,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.findLast(
          (c) => c.command === "transport_connect",
        ).args.permission,
    ),
    "control",
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.calls.filter(
          (c) =>
            c.command === "transport_connect" && c.args.permission === "files",
        ).length,
    ),
    1,
  );
  // Stop receiving ends incoming sessions without touching Linux sharing.
  await nav("共享与设置").click();
  await page
    .getByRole("switch", { name: "允许接收文件", exact: true })
    .uncheck();
  await page.waitForFunction(() => !window.filesFixture.filesEnabled);
  await page
    .locator(".sidebar-status")
    .getByText("文件接收已关闭", { exact: true })
    .waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.filesFixture.sessions.find((s) => s.id === "host-request").state,
    ),
    "closed",
  );
  assert.deepEqual(errors, []);
  await browser.close();
  process.stdout.write("file transfer UI checks passed (synthetic IPC only)\n");
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
