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
      video: { profile: 1, generation: 0, supported: true, max_profile: 3 },
      heartbeat: true,
      lastFrame: 0,
      fullscreen: false,
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
    for (const [x, y, color] of [
      [0, 0, "#c84943"],
      [736, 0, "#418de0"],
      [0, 386, "#e3b94f"],
      [736, 386, "#a05fd3"],
    ]) {
      ctx.fillStyle = color;
      ctx.fillRect(x, y, 64, 64);
    }
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
              { id: 1, name: "Display", width: 3840, height: 2160, dpi: 96 },
            ],
            error: f.closed ? "会话已结束，画面与输入已停止" : null,
            retryable: f.retryable,
          };
        }
        if (cmd === "media_next") {
          await new Promise((r) => setTimeout(r, 150));
          if (f.closed) throw Error("closed");
          if (
            args.after === 0 ||
            (f.heartbeat && Date.now() - f.lastFrame >= 1000)
          ) {
            v.setBigUint64(17, BigInt(args.after + 1));
            f.lastFrame = Date.now();
            return packet.buffer.slice(0);
          }
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
        if (cmd === "viewer_window_action") {
          if (args.action === "fullscreen") f.fullscreen = !f.fullscreen;
          return { fullscreen: f.fullscreen };
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
  assert(await page.locator(".viewer-titlebar").isVisible());
  assert.equal(await page.getByLabel("画面分辨率").inputValue(), "-2");
  for (const [width, profile] of [
    [2560, 2],
    [3840, 3],
    [1200, 0],
  ]) {
    await page.setViewportSize({ width, height: 800 });
    await page.waitForFunction(
      (p) => window.fixture.video.profile === p,
      profile,
    );
    await page
      .getByText("正在切换画面分辨率…", { exact: true })
      .waitFor({ state: "hidden" });
  }
  await page.getByLabel("画面分辨率").selectOption("3");
  await page.waitForFunction(() => window.fixture.video.profile === 3);
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
  await page.getByLabel("画面分辨率").selectOption("2");
  await page.waitForFunction(() => window.fixture.video.profile === 2);
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
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
  assert((await page.locator(".remote-screen").boundingBox()).height >= 763);
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.locator(".viewer-toolbar").waitFor({ state: "visible" });
  await page.getByText("全屏切换", { exact: true }).click();
  assert.equal(await page.locator(".viewer-titlebar").count(), 0);
  assert((await page.locator(".remote-screen").boundingBox()).height >= 799);
  await page.getByText("全屏切换", { exact: true }).click();
  assert(await page.locator(".viewer-titlebar").isVisible());
  await page.waitForFunction(() =>
    /[1-9][\d.]* 帧\/秒/.test(
      document.querySelector(".viewer-status").textContent,
    ),
  );
  await page.evaluate(() => (window.fixture.heartbeat = false));
  await page.waitForFunction(() =>
    document
      .querySelector(".viewer-status")
      .textContent.includes("画面暂未更新"),
  );
  assert(await page.getByAltText("远端桌面").isVisible());
  await page.evaluate(() => (window.fixture.heartbeat = true));
  const verifyFillPixels = async () => {
    await page.mouse.move(400, 300);
    await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
    await page.getByAltText("远端桌面").evaluate((img) => img.decode());
    const area = await page.locator(".remote-screen").boundingBox();
    const points = [
      [0.03, 0.03],
      [0.97, 0.03],
      [0.03, 0.97],
      [0.97, 0.97],
    ].map(([x, y]) => [
      Math.round(area.x + area.width * x),
      Math.round(area.y + area.height * y),
    ]);
    const png = (await page.screenshot()).toString("base64");
    const pixels = await page.evaluate(
      async ({ png, points }) => {
        const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
        const bitmap = await createImageBitmap(
          new Blob([bytes], { type: "image/png" }),
        );
        const canvas = document.createElement("canvas");
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
        const ctx = canvas.getContext("2d");
        ctx.drawImage(bitmap, 0, 0);
        const colors = points.map(([x, y]) =>
          [...ctx.getImageData(x, y, 1, 1).data].slice(0, 3),
        );
        bitmap.close();
        return colors;
      },
      { png, points },
    );
    const expected = [
      [200, 73, 67],
      [65, 141, 224],
      [227, 185, 79],
      [160, 95, 211],
    ];
    pixels.forEach((rgb, i) =>
      assert(
        rgb.every((v, j) => Math.abs(v - expected[i][j]) < 20),
        "all source corners must reach viewport corners without blank bands or cropping",
      ),
    );
  };
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
    await verifyFillPixels();
  }
  await page.setViewportSize({ width: 900, height: 1200 });
  await verifyFillPixels();
  fs.mkdirSync(".local/ui-verification", { recursive: true });
  await page.screenshot({
    path: ".local/ui-verification/viewer-fill-tall.png",
  });
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.getByText("全屏切换", { exact: true }).click();
  await page.setViewportSize({ width: 1920, height: 1080 });
  await verifyFillPixels();
  await page.screenshot({
    path: ".local/ui-verification/viewer-fill-wide.png",
  });
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.getByText("全屏切换", { exact: true }).click();
  await page.setViewportSize({ width: 1200, height: 800 });
  await verifyFillPixels();
  const filled = await page.getByAltText("远端桌面").boundingBox();
  const beforeFillClick = await page.evaluate(
    () =>
      window.fixture.calls.filter(
        (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
      ).length,
  );
  await page.mouse.click(
    filled.x + filled.width * 0.25,
    filled.y + filled.height * 0.75,
  );
  await page.waitForFunction(
    (before) =>
      window.fixture.calls.filter(
        (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
      ).length > before,
    beforeFillClick,
  );
  const mappedFill = await page.evaluate(
    () =>
      window.fixture.calls
        .filter(
          (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
        )
        .at(-1).args.input,
  );
  assert(
    Math.abs(mappedFill.x - 0.25) < 0.005 &&
      Math.abs(mappedFill.y - 0.75) < 0.005,
    "fill coordinates must follow independent axis scaling",
  );
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.getByText("更多操作", { exact: true }).click();
  assert.equal(
    await page
      .getByRole("button", { name: "铺满窗口", exact: true })
      .getAttribute("aria-pressed"),
    "true",
  );
  await page.getByRole("button", { name: "保持比例", exact: true }).click();
  await page.getByText("更多操作", { exact: true }).click();
  await page.mouse.move(400, 300);
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  const buttonCount = await page.evaluate(
    () =>
      window.fixture.calls.filter(
        (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
      ).length,
  );
  await page.mouse.click(200, 50);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter(
          (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
        ).length,
    ),
    buttonCount,
    "letterbox must not inject clicks",
  );
  const paint = await page.getByAltText("远端桌面").boundingBox();
  const scale = Math.min(paint.width / 800, paint.height / 450);
  const beforeContainClick = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page.mouse.click(
    paint.x + (paint.width - 800 * scale) / 2 + 800 * scale * 0.25,
    paint.y + (paint.height - 450 * scale) / 2 + 450 * scale * 0.75,
  );
  await page.waitForFunction(
    (before) =>
      window.fixture.calls
        .slice(before)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.kind === "button" &&
            Math.abs(c.args.input.x - 0.25) < 0.005 &&
            Math.abs(c.args.input.y - 0.75) < 0.005,
        ),
    beforeContainClick,
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
  await page
    .getByText("控制已暂停", { exact: true })
    .waitFor({ state: "attached" });
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
    .waitFor({ state: "detached" });
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
  await page.evaluate(() => (window.fixture.video.max_profile = 1));
  await page.waitForFunction(
    () => document.querySelector('option[value="3"]').disabled,
  );
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
  await load();
  assert(
    await page.locator(".viewer-window:not(.viewer-fill)").count(),
    "aspect preference should survive reconnect/reload",
  );
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByRole("button", { name: "铺满窗口", exact: true }).click();
  await load();
  assert(
    await page.locator(".viewer-fill").count(),
    "fill preference should survive reconnect/reload",
  );
  assert.deepEqual(errors, []);
  await browser.close();
  console.log(
    "PASS screenshot corner coverage in fill mode for wide/tall/fullscreen/restored windows; fill and letterbox coordinates/blank rejection; fit preference retention; custom titlebar/fullscreen; adaptive 2K/4K; FPS/pause/terminal/read-only regressions. Synthetic IPC only.",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
