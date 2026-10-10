// Synthetic UI contract tests; actual Windows input/IPC are separate checks.
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
    viewport: { width: 1200, height: 800 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    const f = (window.fixture = {
      closed: false,
      path: "direct",
      discovery: "direct",
      retryable: false,
      permission: "control",
      inputBlocked: false,
      inputGeneration: 0,
      staleInput: null,
      mouse: { generation: 0, rejected: false },
      reconnectFailure: null,
      reconnectSuccess: false,
      recovery: {
        cycle: 0,
        phase: "idle",
        attempt: 0,
        maxAttempts: 3,
        retryInMs: null,
        manualRetryAllowed: false,
      },
      video: { profile: 1, generation: 0, supported: true, max_profile: 3 },
      heartbeat: true,
      lastFrame: 0,
      layout: 1,
      deliveredLayout: 0,
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
            state: f.closed ? "closed" : f.path,
            discovery: f.discovery,
            permission: f.permission,
            video: f.video,
            input: reportedInput,
            mouse: f.mouse,
            rtt_ms: 23,
            displays: [
              {
                id: 1,
                name: "Display",
                x: 0,
                y: 0,
                width: 3840,
                height: 2160,
                dpi: 96,
                rotation: 0,
              },
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
            v.setBigUint64(9, BigInt(f.layout));
            f.deliveredLayout = f.layout;
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
        if (cmd === "remote_input" && args.input?.kind === "resume_mouse") {
          f.layout++;
          f.lastFrame = 0;
          f.mouse = { generation: f.mouse.generation + 1, rejected: false };
          return {};
        }
        if (cmd === "viewer_reconnect") {
          f.recovery = {
            cycle: f.recovery.cycle + 1,
            phase: "awaiting_approval",
            attempt: 1,
            maxAttempts: 3,
            retryInMs: null,
            manualRetryAllowed: false,
          };
          if (f.reconnectFailure) {
            const exhausted = f.reconnectFailure.includes("自动重连未成功");
            f.recovery.phase = exhausted ? "failed" : "cancelled";
            f.recovery.attempt = exhausted ? 3 : 1;
            f.recovery.manualRetryAllowed = exhausted;
            throw Error(f.reconnectFailure);
          }
          if (f.reconnectSuccess) {
            f.closed = false;
            f.retryable = false;
            f.recovery.phase = "connected";
            return { id: "recovered" };
          }
          return new Promise(() => {});
        }
        if (cmd === "viewer_recovery_status") return { ...f.recovery };
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
    await page.goto(process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test");
    await page.getByAltText("远端桌面").waitFor();
  };
  const switchWindow = () =>
    page.getByRole("button", {
      name: "切换远端窗口（Alt+Tab）",
      exact: true,
    });
  await load();
  for (const [discovery, label] of [
    ["trying_direct", "正在尝试直连"],
    ["relay_fallback", "定时重试直连"],
    ["refresh_failed", "地址刷新失败，将重试"],
    ["forced_relay", "已按设置强制中继"],
  ]) {
    await page.evaluate((discovery) => {
      window.fixture.path = "relay";
      window.fixture.discovery = discovery;
    }, discovery);
    await page.waitForFunction(
      (label) =>
        document.querySelector(".viewer-status").textContent.includes(label),
      label,
    );
    assert(
      await page.getByAltText("远端桌面").isVisible(),
      "discovery states preserve the live picture",
    );
  }
  await page.evaluate(() => {
    window.fixture.path = "direct";
    window.fixture.discovery = "direct";
  });
  await page.waitForFunction(() =>
    document.querySelector(".viewer-status").textContent.includes("已直连"),
  );
  if (!(await page.locator(".viewer-toolbar").isVisible())) {
    await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
  }
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
  await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
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
  await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
  await page.getByText("全屏切换", { exact: true }).click();
  await page.setViewportSize({ width: 1920, height: 1080 });
  await verifyFillPixels();
  await page.screenshot({
    path: ".local/ui-verification/viewer-fill-wide.png",
  });
  await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
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
  await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
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
  // A click sequence must retain control and never leave a remote button down.
  const mouseStart = await page.evaluate(() => window.fixture.calls.length);
  const inside = await page.getByAltText("远端桌面").boundingBox();
  const cx = inside.x + inside.width / 2,
    cy = inside.y + inside.height / 2;
  for (let n = 0; n < 20; n++)
    await page.mouse.click(cx + n, cy, { button: n % 2 ? "right" : "left" });
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .filter(
          (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
        ).length >= 40,
    mouseStart,
  );
  const pairs = await page.evaluate(
    (start) =>
      window.fixture.calls
        .slice(start)
        .filter(
          (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
        )
        .map((c) => c.args.input),
    mouseStart,
  );
  assert.equal(pairs.length, 40);
  for (let n = 0; n < pairs.length; n += 2) {
    assert(pairs[n].down);
    assert(!pairs[n + 1].down);
    assert.equal(pairs[n].button, pairs[n + 1].button);
  }
  const released = async (start) => {
    await page.waitForFunction((start) => {
      const pressed = { left: false, right: false };
      const inputs = window.fixture.calls
        .slice(start)
        .filter((c) => c.cmd === "remote_input")
        .map((c) => c.args.input);
      for (const input of inputs) {
        if (input === null) {
          pressed.left = false;
          pressed.right = false;
        } else if (input.kind === "button") pressed[input.button] = input.down;
      }
      return (
        inputs.some((i) => i?.kind === "button" && i.down) &&
        !pressed.left &&
        !pressed.right
      );
    }, start);
  };
  // Pointer capture covers release outside the image and simultaneous mouse buttons.
  let start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(inside.x + inside.width + 40, cy, { steps: 4 });
  await page.mouse.up();
  await released(start);
  // A normal release outside must keep its final position even while DOWN is
  // still awaiting IPC. Lost capture / pointerleave must not cancel that UP.
  await page.mouse.move(cx, cy);
  await page.waitForTimeout(50);
  await page.keyboard.down("Shift");
  start = await page.evaluate(() => {
    window.fixture.delay = true;
    return window.fixture.calls.length;
  });
  await page.mouse.down();
  await page.waitForFunction(() => !!window.fixture.release);
  await page.mouse.move(inside.x + inside.width + 40, cy, { steps: 4 });
  await page.mouse.up();
  await page.evaluate(() => {
    window.fixture.release();
    window.fixture.release = null;
  });
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.kind === "button" &&
            !c.args.input.down &&
            c.args.input.x === 1,
        ),
    start,
    { timeout: 3000 },
  );
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some((c) => c.cmd === "remote_input" && c.args.input === null),
    start,
  );
  const outsideInputs = await page.evaluate(
    (start) =>
      window.fixture.calls
        .slice(start)
        .filter((c) => c.cmd === "remote_input")
        .map((c) => c.args.input),
    start,
  );
  assert.equal(
    outsideInputs.at(-1),
    null,
    "normal leave must release held keyboard state after UP",
  );
  assert.equal(outsideInputs.filter((i) => i === null).length, 1);
  assert(outsideInputs.at(-2)?.kind === "button" && !outsideInputs.at(-2).down);
  assert.equal(outsideInputs.filter((i) => i?.kind === "button").length, 2);
  await page.keyboard.up("Shift");
  // Holding a button that began outside cannot start a remote gesture by
  // re-entering. A subsequent fresh click must still be accepted.
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.down();
  await page.mouse.move(cx, cy, { steps: 3 });
  await page.mouse.up();
  await page.waitForTimeout(100);
  assert.equal(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
          ).length,
      start,
    ),
    0,
    "a held button from outside is not an owned remote gesture",
  );
  await page.mouse.click(cx, cy);
  await released(start);
  // Suppressing the leave caused by normal UP is one-shot; ordinary hover
  // leaving still releases remote keyboard state.
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.keyboard.down("H");
  await page.mouse.move(inside.x + inside.width + 40, cy);
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some((c) => c.cmd === "remote_input" && c.args.input === null),
    start,
  );
  await page.keyboard.up("H");
  // Abnormal focus/capture loss keeps cancellation: queued work is discarded,
  // a held-button re-entry cannot resurrect it, and a fresh click still works.
  for (const loss of ["blur", "capture"]) {
    await page.mouse.move(cx, cy);
    await page.waitForTimeout(50);
    start = await page.evaluate(() => {
      window.fixture.delay = true;
      return window.fixture.calls.length;
    });
    if (loss === "capture")
      await page.getByAltText("远端桌面").evaluate((img) => {
        window.fixture.captureLost = false;
        img.addEventListener(
          "pointerdown",
          (e) => (window.fixture.pointerId = e.pointerId),
          { once: true },
        );
      });
    await page.mouse.down();
    await page.waitForFunction(() => !!window.fixture.release);
    if (loss === "blur")
      await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    else {
      // Capture becomes active at the next pointer event. Cancelling a pending
      // capture immediately after DOWN does not emit lostpointercapture.
      await page.mouse.move(cx + 10, cy);
      await page.getByAltText("远端桌面").evaluate((img) => {
        const id = window.fixture.pointerId;
        if (!img.hasPointerCapture(id))
          throw Error("pointer capture not active");
        img.addEventListener(
          "lostpointercapture",
          () => (window.fixture.captureLost = true),
          { once: true },
        );
        img.releasePointerCapture(id);
      });
    }
    await page.mouse.move(cx + 40, cy);
    await page.mouse.up();
    await page.evaluate(() => {
      window.fixture.release();
      window.fixture.release = null;
    });
    await page.waitForFunction(
      (start) =>
        window.fixture.calls
          .slice(start)
          .some((c) => c.cmd === "remote_input" && c.args.input === null),
      start,
    );
    if (loss === "capture")
      assert(await page.evaluate(() => window.fixture.captureLost));
    const cancelledInputs = await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input")
          .map((c) => c.args.input),
      start,
    );
    assert.equal(
      cancelledInputs.filter((i) => i?.kind === "button").length,
      1,
      `${loss} must discard queued UP and require a fresh gesture`,
    );
    assert(cancelledInputs[0].down);
    await page.mouse.click(cx, cy);
    await released(start);
  }
  // System switching is sent to the remote input queue rather than handled by
  // the local desktop. A delayed release must finish before any shortcut key.
  if (!(await page.locator(".viewer-toolbar").isVisible()))
    await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
  await page.getByText("更多操作", { exact: true }).click();
  await switchWindow().waitFor();
  await page.waitForTimeout(100);
  start = await page.evaluate(() => {
    window.fixture.delay = true;
    return window.fixture.calls.length;
  });
  await switchWindow().click();
  await page.waitForFunction(() => !!window.fixture.release);
  assert.deepEqual(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input")
          .map((c) => c.args.input),
      start,
    ),
    [null],
    "the shortcut must wait for release-all in the serial queue",
  );
  await page.evaluate(() => {
    window.fixture.release();
    window.fixture.release = null;
  });
  await page.waitForFunction(
    (start) =>
      window.fixture.calls.slice(start).filter((c) => c.cmd === "remote_input")
        .length === 5,
    start,
  );
  assert.deepEqual(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input")
          .map((c) => c.args.input),
      start,
    ),
    [
      null,
      { kind: "key", vk: 0xa4, down: true, repeat: false },
      { kind: "key", vk: 0x09, down: true, repeat: false },
      { kind: "key", vk: 0x09, down: false },
      { kind: "key", vk: 0xa4, down: false },
    ],
    "Alt+Tab must release old gestures, switch once, and release both keys",
  );
  assert(
    await page.evaluate(
      () => document.activeElement === document.querySelector(".remote-screen"),
    ),
    "window switching returns local focus to the remote screen",
  );
  await page.getByText("更多操作", { exact: true }).click();
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.down({ button: "right" });
  await page.mouse.up();
  await page.mouse.up({ button: "right" });
  await released(start);
  const combo = await page.evaluate(
    (start) =>
      window.fixture.calls
        .slice(start)
        .filter(
          (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
        )
        .map((c) => c.args.input),
    start,
  );
  assert(combo.some((i) => i.button === "right" && i.down));
  assert(combo.some((i) => i.button === "right" && !i.down));
  // A changed frame must not detach capture or swallow the final release.
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.down();
  await page.evaluate(() => {
    window.fixture.layout++;
    window.fixture.lastFrame = 0;
  });
  await page.waitForFunction(
    () => window.fixture.deliveredLayout === window.fixture.layout,
  );
  await page.evaluate(() => new Promise(requestAnimationFrame));
  await page.mouse.up();
  await released(start);
  const layoutUp = await page.evaluate(
    (start) =>
      window.fixture.calls
        .slice(start)
        .find(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.kind === "button" &&
            !c.args.input.down,
        )?.args.input.layout,
    start,
  );
  assert.equal(
    layoutUp,
    2,
    "changed frame must be rendered into the new release metadata",
  );
  // Losing focus/capture while a button is held releases it without reconnecting.
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.down();
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await page.mouse.move(cx + 40, cy);
  await page.mouse.up();
  await released(start);
  start = await page.evaluate(() => window.fixture.calls.length);
  await page.mouse.down();
  await page.getByAltText("远端桌面").evaluate((img) => {
    if (img.hasPointerCapture(1)) img.releasePointerCapture(1);
  });
  await page.mouse.move(cx + 10, cy);
  await page.mouse.up();
  await released(start);
  await page.mouse.click(cx, cy);
  await page.keyboard.press("F");
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) =>
        c.cmd === "remote_input" &&
        c.args.input?.kind === "key" &&
        c.args.input.vk === 70,
    ),
  );
  assert(await page.getByAltText("远端桌面").isVisible());
  assert.equal(await page.getByText("控制已暂停", { exact: true }).count(), 0);
  await page.screenshot({ path: ".local/ui-verification/viewer.png" });
  // Geometry rejection affects mouse only; explicit recovery obtains a new layout.
  await page.evaluate(() => {
    window.fixture.mouse = { generation: 1, rejected: true };
  });
  const mouseWarning = page.getByRole("alertdialog", {
    name: "鼠标控制暂时不可用",
  });
  await mouseWarning.waitFor();
  const beforeMouseRecovery = await page.evaluate(
    () => window.fixture.calls.length,
  );
  const beforeDialogKeys = await page.evaluate(
    () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
  );
  await page.keyboard.press("G");
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
    ),
    beforeDialogKeys,
    "dialog keys must not leak into remote input",
  );
  assert(await page.getByAltText("远端桌面").isVisible());
  await mouseWarning
    .getByRole("button", { name: "恢复鼠标控制", exact: true })
    .click();
  await mouseWarning.waitFor({ state: "hidden" });
  await page.waitForFunction(
    () => window.fixture.deliveredLayout === window.fixture.layout,
  );
  await page.getByAltText("远端桌面").click();
  await page.keyboard.press("G");
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.kind === "key" &&
            c.args.input.vk === 71,
        ),
    beforeMouseRecovery,
  );
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.kind === "button" &&
            c.args.input.layout === window.fixture.layout,
        ),
    beforeMouseRecovery,
  );
  // If native control is paused while release-all is pending, the shortcut's
  // old generation must not deliver any queued modifier or Tab events.
  if (!(await page.locator(".viewer-toolbar").isVisible()))
    await page.getByRole("button", { name: "显示工具栏", exact: true }).hover();
  await page.getByText("更多操作", { exact: true }).click();
  await switchWindow().waitFor();
  await page.waitForTimeout(100);
  const cancelledShortcut = await page.evaluate(() => {
    window.fixture.delay = true;
    return window.fixture.calls.length;
  });
  await switchWindow().click();
  await page.waitForFunction(() => !!window.fixture.release);
  await page.evaluate(() => {
    window.fixture.inputBlocked = true;
    window.fixture.inputGeneration++;
  });
  await page
    .getByRole("alertdialog", { name: "控制已暂停", exact: true })
    .waitFor({ state: "attached" });
  assert(await page.getByAltText("远端桌面").isVisible());
  assert.equal(await switchWindow().count(), 0, "paused control cannot switch");
  await page.evaluate(() => {
    window.fixture.release();
    window.fixture.release = null;
  });
  await page.waitForTimeout(100);
  assert.deepEqual(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input")
          .map((c) => c.args.input),
      cancelledShortcut,
    ),
    [null],
    "a paused input generation must discard the queued remote shortcut",
  );
  assert.equal(await page.getByText("会话已结束，远端画面已清除").count(), 0);
  const blockedBefore = await page.evaluate(
    () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
  );
  await page.keyboard.press("Z");
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "remote_input").length,
    ),
    blockedBefore,
  );
  await page.screenshot({ path: ".local/ui-verification/input-paused.png" });
  await page
    .getByRole("alertdialog", { name: "控制已暂停", exact: true })
    .getByRole("button", { name: "重试控制", exact: true })
    .click();
  await page
    .getByText("控制已暂停", { exact: true })
    .waitFor({ state: "detached" });
  assert(await page.getByAltText("远端桌面").isVisible());
  await page.getByText("更多操作", { exact: true }).click();
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
  assert.equal(await switchWindow().count(), 0, "ended control cannot switch");
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
  await page
    .locator(".viewer-empty-state")
    .getByRole("button", { name: "取消重连并关闭" })
    .waitFor();
  assert.equal(await page.getByAltText("远端桌面").count(), 0);
  await page
    .locator(".viewer-empty-state")
    .getByRole("button", { name: "取消重连并关闭" })
    .click();
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
  await page.evaluate(() => {
    window.fixture.retryable = true;
    window.fixture.closed = true;
    window.fixture.reconnectFailure =
      "自动重连未成功，请检查网络并重新发起连接";
  });
  const retryButton = page.getByRole("alertdialog").getByRole("button", {
    name: "重新尝试连接",
    exact: true,
  });
  await retryButton.waitFor();
  await retryButton.click();
  await retryButton.waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    2,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").at(-1)
          .args.manual,
    ),
    true,
  );
  await page.evaluate(() => {
    window.fixture.reconnectFailure = null;
    window.fixture.reconnectSuccess = true;
  });
  await retryButton.click();
  await page.getByAltText("远端桌面").waitFor();
  assert.equal(await retryButton.count(), 0);
  await page.getByAltText("远端桌面").click();
  await page.keyboard.press("H");
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) =>
        c.cmd === "remote_input" &&
        c.args.id === "recovered" &&
        c.args.input?.vk === 72,
    ),
  );
  await load();
  await page.evaluate(() => {
    window.fixture.retryable = true;
    window.fixture.closed = true;
    window.fixture.reconnectFailure = "重连已取消或授权已结束";
  });
  await page
    .getByRole("alertdialog")
    .locator("p")
    .getByText("重连已取消或授权已结束", { exact: true })
    .waitFor();
  assert.equal(
    await retryButton.count(),
    0,
    "authorization refusal must not offer another recovery cycle",
  );
  await load();
  await page.evaluate(() => (window.fixture.permission = "view"));
  await page.waitForFunction(
    () => document.querySelector(".remote-screen").tabIndex === -1,
  );
  assert.equal(
    await switchWindow().count(),
    0,
    "view permission cannot switch",
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
    "PASS repeated left/right pairs, multi-button capture, queued outside UP then release-all, external held entry, blur/lost-capture cancellation, ordered remote Alt+Tab and view/paused/ended guards; fit/quality/FPS/pause/terminal/read-only and screenshot coverage. Synthetic IPC only.",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
