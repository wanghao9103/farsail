// Synthetic UI contract tests; actual Windows input/IPC are separate checks.
const { chromium } = require("playwright");
const assert = require("node:assert/strict");
const fs = require("node:fs");
(async () => {
  const browser = await chromium.launch({
    channel: process.env.UI_BROWSER || "chrome",
    headless: true,
  });
  const page = await browser.newPage({
    viewport: { width: 1200, height: 800 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    const f = (window.fixture = {
      closed: false,
      retryable: false,
      permission: "control",
      inputBlocked: false,
      inputGeneration: 0,
      staleInput: null,
      video: { profile: 1, generation: 0, supported: true },
      calls: [],
      delay: false,
      release: null,
    });
    const canvas = document.createElement("canvas");
    canvas.width = 800;
    canvas.height = 450;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#274c52";
    ctx.fillRect(0, 0, 800, 450);
    ctx.fillStyle = "white";
    ctx.font = "26px sans-serif";
    ctx.fillText("Synthetic remote desktop", 40, 80);
    const jpeg = Uint8Array.from(
      atob(canvas.toDataURL("image/jpeg").split(",")[1]),
      (c) => c.charCodeAt(0),
    );
    const packet = new Uint8Array(49 + jpeg.length),
      v = new DataView(packet.buffer);
    packet.set([70, 83, 77, 49, 1]);
    v.setUint32(5, 1);
    v.setBigUint64(9, 1n);
    v.setBigUint64(17, 1n);
    v.setUint32(33, 800);
    v.setUint32(37, 450);
    packet.set(jpeg, 49);
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "viewer-test" } },
      invoke: async (cmd, args = {}) => {
        f.calls.push({ cmd, args });
        if (cmd === "remote_status") {
          const reportedInput = f.staleInput ?? {
            generation: f.inputGeneration,
            blocked: f.inputBlocked,
            message: f.inputBlocked
              ? "Windows 拒绝输入，控制已暂停；画面连接仍保留。"
              : null,
          };
          f.staleInput = null;
          return {
            state: f.closed ? "closed" : "direct",
            permission: f.permission,
            video: f.video,
            input: reportedInput,
            rtt_ms: 23,
            displays: [
              { id: 1, name: "Display", width: 800, height: 450, dpi: 96 },
            ],
            error: f.closed ? "会话已结束，画面与输入已停止" : null,
            retryable: f.retryable,
          };
        }
        if (cmd === "media_next") {
          if (args.after === 0 && !f.closed) return packet.buffer;
          await new Promise((r) => setTimeout(r, 150));
          if (f.closed) throw Error("closed");
          return new ArrayBuffer(0);
        }
        if (cmd === "remote_input" && f.delay) {
          f.delay = false;
          await new Promise((r) => {
            f.release = r;
          });
        }
        if (cmd === "remote_input" && args.input?.kind === "resume_control") {
          f.inputBlocked = false;
          f.inputGeneration++;
          return {};
        }
        if (cmd === "viewer_reconnect") return new Promise(() => {});
        if (cmd === "media_profile") {
          f.video.profile = args.profile;
          f.video.generation++;
          return f.video.generation;
        }
        return {};
      },
    };
  });
  const load = async () => {
    await page.goto("http://127.0.0.1:1420/?viewer=test");
    await page.getByAltText("远端桌面").waitFor();
  };
  await load();
  await page.getByLabel("画面分辨率").selectOption("0");
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) => c.cmd === "media_profile" && c.args.profile === 0,
    ),
  );
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
  assert.equal(await page.getByLabel("画面分辨率").inputValue(), "0");
  await page.getByLabel("画面分辨率").selectOption("1");
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
  const largeScreen = await page.locator(".remote-screen").boundingBox();
  assert(
    largeScreen.height >= 730,
    "normal toolbar should leave over 90% of an 800px viewport to the desktop",
  );
  assert.equal(await page.getByLabel("远端文字").isVisible(), false);
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByLabel("保持工具栏显示").check();
  assert(await page.getByLabel("远端文字").isVisible());
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByLabel("保持工具栏显示").uncheck();
  await page.getByText("更多操作", { exact: true }).click();
  await page.mouse.move(900, 600);
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  assert((await page.locator(".remote-screen").boundingBox()).height >= 799);
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.locator(".viewer-toolbar").waitFor({ state: "visible" });
  for (const size of [
    { width: 640, height: 420 },
    { width: 1920, height: 1080 },
  ]) {
    await page.setViewportSize(size);
    const area = await page.locator(".remote-screen").boundingBox();
    assert(
      area.height >= size.height - 60 && area.width >= size.width - 2,
      "desktop should use remaining client area without permanent forms/borders",
    );
    assert.equal(
      await page.evaluate(
        () =>
          document.documentElement.scrollWidth > innerWidth ||
          document.documentElement.scrollHeight > innerHeight,
      ),
      false,
    );
  }
  await page.setViewportSize({ width: 1200, height: 800 });
  const paint = await page.getByAltText("远端桌面").boundingBox();
  const scale = Math.min(paint.width / 800, paint.height / 450);
  await page.mouse.click(
    paint.x + (paint.width - 800 * scale) / 2 + 800 * scale * 0.25,
    paint.y + (paint.height - 450 * scale) / 2 + 450 * scale * 0.75,
  );
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) =>
        c.cmd === "remote_input" &&
        c.args.input?.kind === "button" &&
        Math.abs(c.args.input.x - 0.25) < 0.005 &&
        Math.abs(c.args.input.y - 0.75) < 0.005,
    ),
  );
  await page.getByAltText("远端桌面").click();
  assert.equal(
    await page.evaluate(() => document.activeElement.className),
    "remote-screen",
  );
  await page.keyboard.press("Shift+A");
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) => c.cmd === "remote_input" && c.args.input?.vk === 65,
    ),
  );
  assert(
    (await page.evaluate(() => window.fixture.calls)).some(
      (c) => c.args.input?.kind === "button",
    ),
  );
  fs.mkdirSync(".local/ui-verification", { recursive: true });
  await page.screenshot({ path: ".local/ui-verification/viewer.png" });
  await page.evaluate(() => {
    window.fixture.inputBlocked = true;
    window.fixture.inputGeneration++;
  });
  await page.getByText("控制已暂停", { exact: true }).waitFor();
  assert(await page.getByAltText("远端桌面").isVisible());
  assert.equal(await page.getByText("会话已结束，远端画面已清除").count(), 0);
  const blockedBefore = await page.evaluate(
    () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
  );
  await page.getByAltText("远端桌面").click();
  await page.keyboard.press("Z");
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
    ),
    blockedBefore,
  );
  await page.screenshot({ path: ".local/ui-verification/input-paused.png" });
  await page.getByRole("button", { name: "重试控制", exact: true }).click();
  await page
    .getByText("控制已暂停", { exact: true })
    .waitFor({ state: "hidden" });
  assert(await page.getByAltText("远端桌面").isVisible());
  await page.getByAltText("远端桌面").click();
  await page.keyboard.press("Z");
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) => c.cmd === "remote_input" && c.args.input?.vk === 90,
    ),
  );
  await page.evaluate(() => {
    window.fixture.staleInput = {
      generation: 1,
      blocked: true,
      message: "late pause",
    };
  });
  await page.waitForFunction(() => window.fixture.staleInput === null);
  assert.equal(await page.getByText("控制已暂停", { exact: true }).count(), 0);
  await page.evaluate(() => (window.fixture.delay = true));
  await page.keyboard.down("B");
  await page.keyboard.down("C");
  await page.keyboard.down("D");
  await page.waitForFunction(() => !!window.fixture.release);
  const before = await page.evaluate(
    () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
  );
  await page.evaluate(() => (window.fixture.closed = true));
  await page.getByText("会话已结束，远端画面已清除").waitFor();
  assert.equal(await page.getByAltText("远端桌面").count(), 0);
  await page.evaluate(() => window.fixture.release());
  await page.keyboard.press("E");
  await page.waitForTimeout(250);
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
    ),
    before,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    0,
  );
  await load();
  await page.evaluate(() => {
    window.fixture.retryable = true;
    window.fixture.closed = true;
  });
  await page.getByRole("button", { name: "取消重连并关闭" }).waitFor();
  assert.equal(await page.getByAltText("远端桌面").count(), 0);
  await page.getByRole("button", { name: "取消重连并关闭" }).click();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    1,
  );
  assert(
    (await page.evaluate(() => window.fixture.calls)).some(
      (c) => c.cmd === "viewer_window_action" && c.args.action === "close",
    ),
  );
  await load();
  await page.evaluate(() => (window.fixture.permission = "view"));
  await page.waitForFunction(
    () => document.querySelector(".remote-screen").tabIndex === -1,
  );
  await page.getByAltText("远端桌面").click();
  await page.keyboard.press("A");
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
    ),
    0,
  );
  assert.deepEqual(errors, []);
  await browser.close();
  console.log(
    "PASS compact viewer area at 640/1200/1920 widths; on-demand tools; letterbox pointer mapping; focus/input; pause/retry; terminal frame/queue cleanup; network retries/cancel; read-only input. Synthetic IPC only.",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
