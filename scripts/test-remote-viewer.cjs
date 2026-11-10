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
  const installFixture = (options = {}) => {
    const f = (window.fixture = {
      statusDelay: 0,
      statusInFlight: 0,
      statusMaxInFlight: 0,
      tabs: null,
      holdTabSnapshot: false,
      releaseTabSnapshot: null,
      tabSnapshotInFlight: 0,
      tabSnapshotMaxInFlight: 0,
      holdTabMutation: null,
      releaseTabMutation: null,
      tabMutationInFlight: 0,
      tabMutationMaxInFlight: 0,
      failTabSelect: null,
      failTabClose: null,
      appliedTabMutations: [],
      closed: false,
      endReason: null,
      endError: null,
      path: "direct",
      discovery: "direct",
      retryable: false,
      permission: "control",
      platform: "desktop",
      inputBlocked: false,
      fileSupport: false,
      pasteFilesResult: true,
      pasteFilesError: null,
      holdPaste: false,
      releasePaste: null,
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
      hideDisplaysUntil: 0,
      holdSelect: false,
      releaseSelect: null,
      holdDecode: false,
      releaseDecode: null,
      lastFrame: 0,
      layout: 1,
      deliveredLayout: 0,
      fullscreen: false,
      deferWindowAction: null,
      releaseWindowAction: null,
      calls: [],
      delay: false,
      release: null,
    });
    f.noFrames = !!options.noFrames;
    if (options.noFrames) f.platform = "android";
    if (options.viewportStartup) {
      f.platform = "android";
      f.video = {
        profile: 2,
        generation: 0,
        supported: true,
        max_profile: 2,
        viewport_supported: true,
      };
      f.holdNativeFirstFrame = true;
    }
    const decodeImage = HTMLImageElement.prototype.decode;
    HTMLImageElement.prototype.decode = function () {
      if (!f.holdDecode) return decodeImage.call(this);
      return new Promise((resolve, reject) => {
        f.releaseDecode = () => decodeImage.call(this).then(resolve, reject);
      });
    };
    let packet, v;
    f.frame = (width, height) => {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d");
      ctx.fillStyle = "#274c52";
      ctx.fillRect(0, 0, width, height);
      ctx.fillStyle = "white";
      ctx.font = "26px sans-serif";
      ctx.fillText("Synthetic remote desktop", 40, 80);
      for (const [x, y, color] of [
        [0, 0, "#c84943"],
        [width - 64, 0, "#418de0"],
        [0, height - 64, "#e3b94f"],
        [width - 64, height - 64, "#a05fd3"],
      ]) {
        ctx.fillStyle = color;
        ctx.fillRect(x, y, 64, 64);
      }
      const jpeg = Uint8Array.from(
        atob(canvas.toDataURL("image/jpeg").split(",")[1]),
        (c) => c.charCodeAt(0),
      );
      packet = new Uint8Array(49 + jpeg.length);
      v = new DataView(packet.buffer);
      packet.set([70, 83, 77, 49, 1]);
      v.setUint32(5, 1);
      v.setBigUint64(9, 1n);
      v.setBigUint64(17, 1n);
      v.setUint32(33, width);
      v.setUint32(37, height);
      packet.set(jpeg, 49);
      f.lastFrame = 0;
    };
    f.frame(800, 450);
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "viewer-test" } },
      invoke: async (cmd, args = {}) => {
        f.calls.push({ cmd, args });
        if (["remote_paste_files", "remote_send_files"].includes(cmd)) {
          if (f.holdPaste)
            await new Promise((resolve) => {
              f.releasePaste = resolve;
            });
          if (f.pasteFilesError) throw f.pasteFilesError;
          return f.pasteFilesResult;
        }
        if (cmd === "viewer_tabs") {
          // Capture before waiting, so this genuinely returns a stale native snapshot.
          const snapshot = f.tabs ? structuredClone(f.tabs) : {};
          f.tabSnapshotInFlight++;
          f.tabSnapshotMaxInFlight = Math.max(
            f.tabSnapshotMaxInFlight,
            f.tabSnapshotInFlight,
          );
          try {
            if (f.holdTabSnapshot) {
              f.holdTabSnapshot = false;
              await new Promise((resolve) => {
                f.releaseTabSnapshot = resolve;
              });
            }
            return snapshot;
          } finally {
            f.tabSnapshotInFlight--;
          }
        }
        if (
          [
            "viewer_select_tab",
            "viewer_close_tab",
            "viewer_detach_tab",
          ].includes(cmd)
        ) {
          f.tabMutationInFlight++;
          f.tabMutationMaxInFlight = Math.max(
            f.tabMutationMaxInFlight,
            f.tabMutationInFlight,
          );
          try {
            if (f.holdTabMutation === args.instance) {
              f.holdTabMutation = null;
              await new Promise((resolve) => {
                f.releaseTabMutation = resolve;
              });
            }
            if (cmd === "viewer_select_tab") {
              if (f.failTabSelect === args.instance) {
                f.failTabSelect = null;
                throw Error("测试设备标签切换失败");
              }
              f.tabs?.forEach((tab) => {
                tab.selected = tab.instance === args.instance;
              });
            } else if (f.tabs) {
              if (f.failTabClose === args.instance) {
                f.failTabClose = null;
                throw Error("测试设备标签关闭失败");
              }
              const index = f.tabs.findIndex(
                (t) => t.instance === args.instance,
              );
              const selected = f.tabs[index]?.selected;
              f.tabs = f.tabs.filter((t) => t.instance !== args.instance);
              const neighbor = f.tabs[index] ?? f.tabs.at(-1);
              if (selected && neighbor) neighbor.selected = true;
            }
            f.appliedTabMutations.push({ cmd, instance: args.instance });
            return {};
          } finally {
            f.tabMutationInFlight--;
          }
        }
        if (cmd === "remote_status") {
          f.statusInFlight++;
          f.statusMaxInFlight = Math.max(f.statusMaxInFlight, f.statusInFlight);
          try {
            if (f.statusDelay)
              await new Promise((resolve) =>
                setTimeout(resolve, f.statusDelay),
              );
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
              platform: f.platform,
              files: { supported: f.fileSupport, status: { transfers: [] } },
              // Native IPC serializes a fresh snapshot. Sharing this mutable
              // fixture object lets later mock requests rewrite React's past
              // status while a render/effect is still completing.
              video: { ...f.video },
              input: reportedInput,
              mouse: f.mouse,
              rtt_ms: 23,
              displays:
                Date.now() < f.hideDisplaysUntil
                  ? []
                  : [
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
              error: f.closed
                ? (f.endError ?? "会话已结束，画面与输入已停止")
                : null,
              reason: f.closed ? (f.endReason ?? "session_ended") : undefined,
              retryable: f.retryable,
            };
          } finally {
            f.statusInFlight--;
          }
        }
        if (cmd === "media_next") {
          await new Promise((r) => setTimeout(r, 150));
          if (f.closed) throw Error("closed");
          if (f.noFrames) return new ArrayBuffer(0);
          if (f.holdNativeFirstFrame && !f.video.viewport_edge)
            return new ArrayBuffer(0);
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
        if (cmd === "media_select" && f.holdSelect) {
          await new Promise((resolve) => {
            f.releaseSelect = resolve;
          });
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
            if (f.hideDisplaysUntil)
              f.hideDisplaysUntil = Date.now() + (f.restoreDisplayDelay ?? 400);
            f.recovery.phase = "connected";
            return { id: "recovered" };
          }
          return new Promise(() => {});
        }
        if (cmd === "viewer_recovery_status") return { ...f.recovery };
        if (cmd === "media_profile") {
          f.video.profile = args.profile;
          f.video.viewport_edge = 0;
          f.video.generation++;
          return f.video.generation;
        }
        if (cmd === "media_viewport") {
          f.video.profile = 2;
          f.video.viewport_edge = args.edge;
          f.video.generation++;
          f.frame(args.edge, Math.floor((args.edge * 9) / 16));
          return f.video.generation;
        }
        if (cmd === "viewer_window_action") {
          if (args.action === "fullscreen") f.fullscreen = !f.fullscreen;
          const snapshot = { fullscreen: f.fullscreen };
          if (args.action === f.deferWindowAction) {
            await new Promise((resolve) => {
              f.releaseWindowAction = resolve;
            });
          }
          return snapshot;
        }
        return {};
      },
    };
  };
  if (process.env.UI_FIRST_FRAME_ONLY === "1") {
    await page.addInitScript(installFixture, { noFrames: true });
    await page.goto(process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test");
    await page.getByText("正在接收远端首帧…", { exact: true }).waitFor();
    await page
      .getByText("连接已建立，但尚未收到画面。", { exact: true })
      .waitFor({ timeout: 8000 });
    await page.getByRole("button", { name: "重新连接", exact: true }).waitFor();
    assert.equal(
      await page.evaluate(
        () =>
          window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect")
            .length,
      ),
      0,
    );
    await page.evaluate(() => {
      window.fixture.noFrames = false;
    });
    await page.getByAltText("远端桌面").waitFor();
    assert.equal(
      await page
        .getByText("连接已建立，但尚未收到画面。", { exact: true })
        .count(),
      0,
    );
    assert.deepEqual(errors, []);
    await browser.close();
    console.log(
      "PASS delayed first frame has guidance and manual retry; same-session pixels clear the warning without reconnecting. Synthetic IPC only.",
    );
    return;
  }
  async function testDropdownDismissal() {
    const dropdown = await browser.newPage({
      viewport: { width: 1200, height: 800 },
    });
    await dropdown.addInitScript(installFixture);
    await dropdown.goto(
      process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test",
    );
    await dropdown.getByAltText("远端桌面").waitFor();
    await dropdown.locator(".remote-screen").focus();
    await dropdown.locator(".viewer-toolbar").waitFor({ state: "hidden" });
    await dropdown
      .getByRole("button", { name: "显示工具栏", exact: true })
      .click();
    await dropdown.getByText("更多操作", { exact: true }).click();
    await dropdown.getByLabel("远端文字", { exact: true }).waitFor();
    const menu = await dropdown.locator(".viewer-toolbar").boundingBox();
    assert(
      menu.width <= 220 && menu.height > menu.width,
      "remote controls form a narrow vertical menu",
    );
    await dropdown.mouse.move(950, 600);
    await dropdown
      .locator(".viewer-toolbar")
      .waitFor({ state: "hidden", timeout: 2500 });
    assert.equal(
      await dropdown.locator(".viewer-toolbar details[open]").count(),
      0,
      "mouse exit closes expanded sections too",
    );
    await dropdown
      .getByRole("button", { name: "显示工具栏", exact: true })
      .press("Enter");
    await dropdown.getByLabel("桌面设置", { exact: true }).waitFor();
    await dropdown.waitForTimeout(1900);
    assert(
      await dropdown.locator(".viewer-toolbar").isVisible(),
      "keyboard focus keeps the menu available",
    );
    await dropdown.locator(".remote-screen").focus();
    await dropdown.locator(".viewer-toolbar").hover();
    await dropdown.mouse.move(950, 600);
    await dropdown
      .locator(".viewer-toolbar")
      .waitFor({ state: "hidden", timeout: 2500 });
    assert.equal(
      await dropdown.locator(".viewer-toolbar details[open]").count(),
      0,
      "old keyboard modality cannot retain a menu whose focus has left",
    );
    await dropdown.close();
  }
  if (process.env.UI_DROPDOWN_ONLY === "1") {
    await testDropdownDismissal();
    await browser.close();
    console.log(
      "PASS vertical dropdown; leaving closes expanded sections; keyboard focus retains menu. Synthetic IPC only.",
    );
    return;
  }
  await page.addInitScript(installFixture);
  if (process.env.UI_ONLY_VIEWPORT === "1") {
    await testAdaptiveViewports();
    await browser.close();
    console.log(
      "PASS metadata-only startup and four independent adaptive viewports",
    );
    return;
  }
  await testDropdownDismissal();
  const load = async () => {
    await page.goto(process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test");
    await page.getByAltText("远端桌面").waitFor();
  };
  const switchWindow = () =>
    page.getByRole("button", {
      name: "切换远端窗口（Alt+Tab）",
      exact: true,
    });
  const leaveToolbar = async () => {
    // The centre and fixed coordinates can be inside a vertical dropdown.
    // The remote canvas corner stays outside it across responsive sizes.
    const canvas = await page.locator(".remote-screen").boundingBox();
    assert(canvas);
    await page.mouse.move(
      canvas.x + canvas.width - 8,
      canvas.y + canvas.height - 8,
    );
    await page.locator(".remote-screen").focus();
    await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  };
  const revealToolbar = async () => {
    await leaveToolbar();
    await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
    await page.locator(".viewer-toolbar").hover();
  };
  await load();
  // One slow decoder retains its last image and does not stall another viewer.
  const previousImage = await page.getByAltText("远端桌面").getAttribute("src");
  await page.evaluate(() => {
    window.fixture.holdDecode = true;
    window.fixture.lastFrame = 0;
  });
  await page.waitForFunction(() => !!window.fixture.releaseDecode);
  const heldCalls = await page.evaluate(
    () => window.fixture.calls.filter((c) => c.cmd === "media_next").length,
  );
  const other = await browser.newPage();
  await other.addInitScript(installFixture);
  await other.goto(process.env.UI_URL || "http://127.0.0.1:1420/?viewer=other");
  await other.getByAltText("远端桌面").waitFor();
  const otherImage = await other.getByAltText("远端桌面").getAttribute("src");
  await other.evaluate(() => {
    window.fixture.lastFrame = 0;
  });
  await other.waitForFunction(
    (src) =>
      document.querySelector('img[alt="远端桌面"]')?.getAttribute("src") !==
      src,
    otherImage,
  );
  assert.equal(
    await page.getByAltText("远端桌面").getAttribute("src"),
    previousImage,
    "retain the displayed image until its replacement is decoded",
  );
  assert.equal(
    await page.evaluate(
      () => window.fixture.calls.filter((c) => c.cmd === "media_next").length,
    ),
    heldCalls,
    "one window must not build an unbounded decode queue",
  );
  await page.evaluate(() => {
    window.fixture.holdDecode = false;
    window.fixture.releaseDecode();
  });
  await page.waitForFunction(
    (src) =>
      document.querySelector('img[alt="远端桌面"]')?.getAttribute("src") !==
      src,
    previousImage,
  );
  await other.close();
  // Updating local chrome must not restore an older, already revoked frame URL.
  const retainedFrame = await page.evaluate(() => {
    window.fixture.heartbeat = false;
    return document.querySelector('img[alt="远端桌面"]').getAttribute("src");
  });
  await page.waitForTimeout(1100); // Include the periodic status/FPS UI updates.
  assert.equal(
    await page.getByAltText("远端桌面").getAttribute("src"),
    retainedFrame,
  );
  assert(
    await page
      .getByAltText("远端桌面")
      .evaluate((image) => image.complete && image.naturalWidth > 0),
  );
  // Defer scheduled React work while two frames with changed geometry decode.
  // Pixels and input handlers must never expose different committed layouts.
  const geometryPage = await browser.newPage({
    viewport: { width: 1000, height: 900 },
  });
  const geometryErrors = [];
  geometryPage.on("pageerror", (error) => geometryErrors.push(error.message));
  await geometryPage.addInitScript(installFixture);
  await geometryPage.addInitScript(() => {
    const NativeChannel = window.MessageChannel;
    window.MessageChannel = class extends NativeChannel {
      constructor() {
        super();
        const port = this.port1;
        const descriptor = Object.getOwnPropertyDescriptor(
          MessagePort.prototype,
          "onmessage",
        );
        let handler;
        Object.defineProperty(port, "onmessage", {
          get: () => handler,
          set(fn) {
            handler = fn;
            descriptor.set.call(
              port,
              typeof fn === "function"
                ? (event) => {
                    const fixture = window.fixture;
                    if (fixture?.holdReactCommit) {
                      (fixture.reactWork ??= []).push(() =>
                        fn.call(port, event),
                      );
                    } else fn.call(port, event);
                  }
                : fn,
            );
          },
        });
      }
    };
  });
  await geometryPage.goto(
    process.env.UI_URL || "http://127.0.0.1:1420/?viewer=geometry",
  );
  await geometryPage.getByAltText("远端桌面").waitFor();
  await geometryPage.evaluate(() => {
    window.fixture.heartbeat = false;
  });
  await geometryPage.waitForTimeout(400);
  await geometryPage.evaluate(() => {
    const fixture = window.fixture;
    const image = document.querySelector('img[alt="远端桌面"]');
    const key = Object.keys(image).find((key) =>
      key.startsWith("__reactProps$"),
    );
    window.geometryOldHandler = image[key].onPointerDown;
    window.geometryDecoded = 0;
    const decode = HTMLImageElement.prototype.decode;
    HTMLImageElement.prototype.decode = async function () {
      await decode.call(this);
      window.geometryDecoded++;
    };
    fixture.holdReactCommit = true;
    fixture.layout++;
    fixture.frame(450, 800);
    fixture.lastFrame = 0;
    fixture.heartbeat = true;
  });
  await geometryPage.waitForFunction(() => window.geometryDecoded >= 2);
  const geometry = await geometryPage.evaluate(() => {
    const image = document.querySelector('img[alt="远端桌面"]');
    const key = Object.keys(image).find((key) =>
      key.startsWith("__reactProps$"),
    );
    return {
      width: image.naturalWidth,
      handlerChanged: image[key].onPointerDown !== window.geometryOldHandler,
      gated: window.fixture.reactWork?.length ?? 0,
    };
  });
  assert(geometry.gated > 0, "React scheduled work must actually be held");
  assert(
    !(geometry.width === 450 && !geometry.handlerChanged),
    "new pixels must not precede the new geometry input handlers",
  );
  await geometryPage.evaluate(() => {
    window.fixture.holdReactCommit = false;
    for (const run of window.fixture.reactWork.splice(0)) run();
  });
  await geometryPage.waitForFunction(() => {
    const image = document.querySelector('img[alt="远端桌面"]');
    return (
      image.complete &&
      image.naturalWidth === 450 &&
      image.naturalHeight === 800
    );
  });
  await geometryPage.getByAltText("远端桌面").click();
  await geometryPage.waitForFunction(() =>
    window.fixture.calls.some(
      (call) =>
        call.cmd === "remote_input" && call.args.input?.kind === "button",
    ),
  );
  assert(
    await geometryPage.evaluate(() =>
      window.fixture.calls
        .filter(
          (call) =>
            call.cmd === "remote_input" && call.args.input?.kind === "button",
        )
        .every((call) => call.args.input.layout === window.fixture.layout),
    ),
    "input must use the currently displayed layout",
  );
  assert.deepEqual(geometryErrors, []);
  await geometryPage.close();
  // A terminal local error must fence a decode that outlives native close IPC.
  const failedPage = await browser.newPage();
  const failedPageErrors = [];
  failedPage.on("pageerror", (error) => failedPageErrors.push(error.message));
  await failedPage.addInitScript(installFixture);
  await failedPage.goto(
    process.env.UI_URL || "http://127.0.0.1:1420/?viewer=late-frame",
  );
  await failedPage.getByAltText("远端桌面").waitFor();
  await failedPage.locator(".remote-screen").focus();
  await failedPage.evaluate(() => {
    const fixture = window.fixture;
    fixture.holdDecode = true;
    fixture.lastFrame = 0;
    fixture.revokedFrameURLs = [];
    const decode = HTMLImageElement.prototype.decode;
    HTMLImageElement.prototype.decode = function () {
      fixture.pendingDecodeURL = this.src;
      return decode.call(this);
    };
    const revoke = URL.revokeObjectURL.bind(URL);
    URL.revokeObjectURL = (url) => {
      fixture.revokedFrameURLs.push(url);
      revoke(url);
    };
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = (cmd, args = {}) => {
      if (cmd === "remote_input" && args.input?.kind === "key")
        return Promise.reject(Error("synthetic input transport failure"));
      if (cmd === "transport_close") {
        fixture.closeHeld = true;
        return new Promise(() => {});
      }
      return invoke(cmd, args);
    };
  });
  await failedPage.waitForFunction(() => !!window.fixture.releaseDecode);
  await failedPage.keyboard.press("j");
  await failedPage.waitForFunction(
    () =>
      window.fixture.closeHeld &&
      !document.querySelector('img[alt="远端桌面"]'),
  );
  assert(
    await failedPage.evaluate(() =>
      window.fixture.revokedFrameURLs.includes(window.fixture.pendingDecodeURL),
    ),
    "an abandoned decode URL must be revoked without waiting for native close",
  );
  const endedReads = await failedPage.evaluate(
    () =>
      window.fixture.calls.filter((call) => call.cmd === "media_next").length,
  );
  await failedPage.evaluate(() => {
    window.fixture.holdDecode = false;
    window.fixture.releaseDecode();
  });
  await failedPage.waitForTimeout(300);
  assert.equal(
    await failedPage.getByAltText("远端桌面").count(),
    0,
    "late decoding must not resurrect an ended frame",
  );
  assert.equal(
    await failedPage.evaluate(
      () =>
        window.fixture.calls.filter((call) => call.cmd === "media_next").length,
    ),
    endedReads,
    "terminal media reads must stop even if native close is pending",
  );
  assert.deepEqual(failedPageErrors, []);
  await failedPage.close();
  // u64 counters outside the JS cursor range must not be rounded and reread.
  for (const offset of [17, 9]) {
    const counterPage = await browser.newPage();
    const counterErrors = [];
    counterPage.on("pageerror", (error) => counterErrors.push(error.message));
    await counterPage.addInitScript(installFixture);
    await counterPage.goto(
      process.env.UI_URL || "http://127.0.0.1:1420/?viewer=counter",
    );
    await counterPage.getByAltText("远端桌面").waitFor();
    await counterPage.evaluate((offset) => {
      const fixture = window.fixture;
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (cmd, args = {}) => {
        if (cmd === "transport_close")
          fixture.counterClosed = (fixture.counterClosed ?? 0) + 1;
        const result = await invoke(
          cmd,
          cmd === "media_next" ? { ...args, after: 0 } : args,
        );
        if (cmd === "media_next" && result.byteLength >= 50) {
          const view = new DataView(result);
          view.setBigUint64(17, BigInt(args.after ?? 0) + 1n);
          view.setBigUint64(offset, 9007199254740993n);
          fixture.counterInjected = (fixture.counterInjected ?? 0) + 1;
        }
        return result;
      };
    }, offset);
    await counterPage.waitForFunction(
      () =>
        window.fixture.counterClosed === 1 &&
        !document.querySelector('img[alt="远端桌面"]'),
    );
    const stoppedReads = await counterPage.evaluate(
      () =>
        window.fixture.calls.filter((call) => call.cmd === "media_next").length,
    );
    await counterPage.waitForTimeout(250);
    assert.equal(
      await counterPage.evaluate(
        () =>
          window.fixture.calls.filter((call) => call.cmd === "media_next")
            .length,
      ),
      stoppedReads,
    );
    assert.equal(
      await counterPage.evaluate(() => window.fixture.counterInjected),
      1,
    );
    assert.deepEqual(counterErrors, []);
    await counterPage.close();
  }
  await page.getByAltText("远端桌面").waitFor();
  if (process.env.UI_ONLY_FRAME_HOTPATH === "1") {
    await browser.close();
    console.log(
      "PASS frame isolation, atomic geometry/input, terminal cleanup and safe counters",
    );
    return;
  }
  await load();
  // Moving a native window and opening remote tools are separate actions.
  await page.mouse.move(500, 600);
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  await page.locator(".viewer-titlebar").hover();
  assert(
    !(await page.locator(".viewer-toolbar").isVisible()),
    "titlebar hover must not reveal remote controls",
  );
  const dragHandle = await page
    .locator(".viewer-titlebar .window-drag")
    .boundingBox();
  assert(dragHandle && dragHandle.width > 0);
  const dragCallsBefore = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page.mouse.move(
    dragHandle.x + dragHandle.width / 2,
    dragHandle.y + dragHandle.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    dragHandle.x + dragHandle.width / 2 + 12,
    dragHandle.y + dragHandle.height / 2 + 4,
  );
  await page.mouse.up();
  assert(
    await page.evaluate(
      (after) =>
        window.fixture.calls
          .slice(after)
          .some(
            (call) =>
              call.cmd === "viewer_window_action" &&
              call.args.action === "drag",
          ),
      dragCallsBefore,
    ),
    "titlebar must still request native dragging",
  );
  assert(
    !(await page.locator(".viewer-toolbar").isVisible()),
    "titlebar dragging must keep remote controls hidden",
  );
  // Keyboard users can reveal the toolbar and keep it open while choosing controls.
  await page.mouse.move(500, 600);
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  await page
    .getByRole("button", { name: "显示工具栏", exact: true })
    .press("Enter");
  await page.waitForFunction(
    () => document.activeElement?.getAttribute("aria-label") === "桌面设置",
  );
  await page.getByLabel("桌面设置", { exact: true }).press("Tab");
  await page.waitForTimeout(2500);
  assert(await page.locator(".viewer-toolbar").isVisible());
  assert(
    await page
      .locator(".viewer-toolbar")
      .evaluate((element) => element.contains(document.activeElement)),
    "auto-hide must not remove a focused toolbar control",
  );
  await page.locator(".remote-screen").focus();
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
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
  await revealToolbar();
  assert(await page.locator(".viewer-titlebar").isVisible());
  assert(
    await page
      .getByRole("button", { name: "传输文件", exact: true })
      .isDisabled(),
    "old peers do not silently receive file data",
  );
  await page.evaluate(() => {
    window.fixture.fileSupport = true;
    window.fixture.holdPaste = true;
  });
  await page.waitForFunction(
    () =>
      !Array.from(document.querySelectorAll("button")).find(
        (b) => b.textContent === "传输文件",
      )?.disabled,
  );
  await page.locator(".remote-screen").focus();
  const pasteStart = await page.evaluate(() => window.fixture.calls.length);
  await page.keyboard.press("Control+v");
  await page.waitForFunction(() => window.fixture.releasePaste !== null);
  await page.keyboard.press("Control+v");
  assert.equal(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_paste_files").length,
      pasteStart,
    ),
    1,
    "repeated paste cannot start another transfer",
  );
  assert.equal(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input" && c.args.input?.vk === 0x56)
          .length,
      pasteStart,
    ),
    0,
    "file paste never sends an early V into the remote app",
  );
  assert(await page.getByAltText("远端桌面").isVisible());
  await page.screenshot({
    path: ".local/ui-verification/clipboard-paste-progress.png",
  });
  await page.evaluate(() => {
    window.fixture.holdPaste = false;
    window.fixture.releasePaste();
    window.fixture.releasePaste = null;
  });
  await page
    .getByText("已发送文件粘贴请求。", {
      exact: true,
    })
    .waitFor();
  await revealToolbar();
  await page.getByRole("button", { name: "传输文件", exact: true }).click();
  await page.waitForFunction(() =>
    window.fixture.calls.some((c) => c.cmd === "remote_send_files"),
  );
  await page.evaluate(() => {
    window.fixture.pasteFilesResult = false;
  });
  const fallbackStart = await page.evaluate(() => window.fixture.calls.length);
  await page.locator(".remote-screen").focus();
  await page.keyboard.press("Control+v");
  await page.waitForFunction(
    (start) =>
      window.fixture.calls
        .slice(start)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.input?.vk === 0xa2 &&
            c.args.input?.down === false,
        ),
    fallbackStart,
  );
  assert.equal(
    await page.evaluate(
      (start) =>
        window.fixture.calls
          .slice(start)
          .filter((c) => c.cmd === "remote_input" && c.args.input?.vk === 0x56)
          .length,
      fallbackStart,
    ),
    2,
    "no local files preserves ordinary remote clipboard paste",
  );
  await page.evaluate(() => {
    window.fixture.pasteFilesResult = true;
  });
  await page.evaluate(() => {
    window.fixture.pasteFilesError = "剪贴板读取失败测试";
  });
  await page.locator(".remote-screen").focus();
  await page.keyboard.press("Control+v");
  await page.getByText("剪贴板读取失败测试", { exact: true }).first().waitFor();
  assert(
    await page.getByAltText("远端桌面").isVisible(),
    "clipboard failure preserves screen connection",
  );
  await page.getByRole("button", { name: "知道了", exact: true }).click();
  await page.evaluate(() => {
    window.fixture.pasteFilesError = null;
    window.fixture.fileSupport = false;
  });
  await page.waitForFunction(
    () =>
      Array.from(document.querySelectorAll("button")).find(
        (b) => b.textContent === "传输文件",
      )?.disabled,
  );
  await load();
  await revealToolbar();
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
  await revealToolbar();
  if ((await page.locator(".viewer-settings").getAttribute("open")) === null)
    await page.getByLabel("桌面设置", { exact: true }).click();
  await page.getByLabel("画面分辨率").selectOption("3");
  await page.waitForFunction(() => window.fixture.video.profile === 3);
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
  await revealToolbar();
  if ((await page.locator(".viewer-settings").getAttribute("open")) === null)
    await page.getByLabel("桌面设置", { exact: true }).click();
  await page.getByLabel("画面分辨率").selectOption("2");
  await page.waitForFunction(() => window.fixture.video.profile === 2);
  await page
    .getByText("正在切换画面分辨率…", { exact: true })
    .waitFor({ state: "hidden" });
  await revealToolbar();
  if ((await page.locator(".viewer-settings").getAttribute("open")) === null)
    await page.getByLabel("桌面设置", { exact: true }).click();
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
  await revealToolbar();
  if ((await page.locator(".viewer-settings").getAttribute("open")) === null)
    await page.getByLabel("桌面设置", { exact: true }).click();
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
  await page.locator(".remote-screen").focus();
  await page.locator(".viewer-toolbar").waitFor({ state: "hidden" });
  assert((await page.locator(".remote-screen").boundingBox()).height >= 763);
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.locator(".viewer-toolbar").waitFor({ state: "visible" });
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
  assert.equal(await page.locator(".viewer-titlebar").count(), 0);
  assert((await page.locator(".remote-screen").boundingBox()).height >= 799);
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
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
  const verifyFillPixels = async (source = null, turns = 0) => {
    // This assertion measures a static frame. Avoid revoking its blob URL while img.decode() is pending.
    await page.evaluate(() => (window.fixture.heartbeat = false));
    await leaveToolbar();
    await page.getByAltText("远端桌面").evaluate((img) => img.decode());
    let area = await page.locator(".remote-screen").boundingBox();
    if (source) {
      const scale = Math.min(
        area.width / source.width,
        area.height / source.height,
      );
      const width = source.width * scale,
        height = source.height * scale;
      area = {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
      };
    }
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
    const order = [
      [0, 1, 2, 3],
      [2, 0, 3, 1],
      [3, 2, 1, 0],
      [1, 3, 0, 2],
    ][turns];
    pixels.forEach((rgb, i) =>
      assert(
        rgb.every((v, j) => Math.abs(v - expected[order[i]][j]) < 20),
        "all source corners must reach viewport corners without blank bands or cropping",
      ),
    );
    await page.evaluate(() => (window.fixture.heartbeat = true));
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
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
  await page.setViewportSize({ width: 1920, height: 1080 });
  await verifyFillPixels();
  await page.screenshot({
    path: ".local/ui-verification/viewer-fill-wide.png",
  });
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
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
  await leaveToolbar();
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
  await revealToolbar();
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
  await revealToolbar();
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
  assert.equal(await page.getByText("连接已结束").count(), 0);
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
    .locator(".viewer-title-status")
    .filter({ hasText: "控制已暂停" })
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
  await page.getByText("连接已结束", { exact: true }).waitFor();
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
  // Capture failure is manual-only: the user can request a fresh grant, but
  // persistent fullscreen/capture failures cannot create an automatic session chain.
  await load();
  await page.evaluate(() => {
    window.fixture.closed = true;
    window.fixture.endReason = "capture_failed";
    window.fixture.endError = "被控端屏幕采集中断，请重试连接";
    // Even a malformed legacy hint must not make capture failure automatic.
    window.fixture.retryable = true;
  });
  const captureDialog = () =>
    page.getByRole("alertdialog", { name: "屏幕采集已中断", exact: true });
  await captureDialog()
    .getByRole("button", { name: "重试连接", exact: true })
    .waitFor();
  await page.waitForTimeout(900);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    0,
  );
  assert.equal(await page.getByAltText("远端桌面").count(), 0);
  await page.screenshot({
    path: ".local/ui-verification/viewer-capture-manual-retry.png",
  });
  await captureDialog()
    .getByRole("button", { name: "取消", exact: true })
    .click();
  const captureCanvasRetry = page
    .locator(".viewer-empty-state")
    .getByRole("button", { name: "重试连接", exact: true });
  await captureCanvasRetry.waitFor();
  await page.evaluate(() => {
    window.fixture.reconnectFailure = "重连已取消或授权已结束";
  });
  await captureCanvasRetry.evaluate((button) => {
    button.click();
    button.click();
  });
  await page
    .getByRole("alertdialog")
    .locator("p")
    .getByText("重连已取消或授权已结束", { exact: true })
    .waitFor();
  await page.waitForTimeout(900);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    1,
    "one explicit retry, including a double click, cannot start parallel or automatic cycles",
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.find((c) => c.cmd === "viewer_reconnect").args
          .manual,
    ),
    true,
  );
  assert.equal(
    await page.getByRole("button", { name: "重试连接", exact: true }).count(),
    0,
    "an early identity/refusal failure consumes the capture-only button",
  );
  await load();
  await page.evaluate(() => {
    window.fixture.closed = true;
    window.fixture.endReason = "capture_failed";
    window.fixture.endError = "被控端屏幕采集中断，请重试连接";
    window.fixture.reconnectSuccess = true;
  });
  await captureDialog()
    .getByRole("button", { name: "重试连接", exact: true })
    .click();
  await page.getByAltText("远端桌面").waitFor();
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    1,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.find((c) => c.cmd === "viewer_reconnect").args
          .manual,
    ),
    true,
  );
  await page.evaluate(() => {
    window.fixture.closed = true;
  });
  await captureDialog()
    .getByRole("button", { name: "重试连接", exact: true })
    .waitFor();
  await page.waitForTimeout(900);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    1,
    "a new capture failure after successful manual recovery needs another explicit click",
  );
  for (const reason of [
    "authorization_failed",
    "lease_expired",
    "renewal_failed",
    "input_failed",
    "stopped",
  ]) {
    await load();
    await page.evaluate((reason) => {
      window.fixture.closed = true;
      window.fixture.endReason = reason;
    }, reason);
    await page.getByText("连接已结束", { exact: true }).waitFor();
    assert.equal(
      await page.getByRole("button", { name: "重试连接", exact: true }).count(),
      0,
      reason,
    );
    assert.equal(
      await page.evaluate(
        () =>
          window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect")
            .length,
      ),
      0,
      reason,
    );
  }
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
    .getByRole("button", { name: "取消重连" })
    .waitFor();
  assert.equal(await page.getByAltText("远端桌面").count(), 0);
  await page
    .locator(".viewer-empty-state")
    .getByRole("button", { name: "取消重连" })
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
      (c) => c.cmd === "viewer_close_tab",
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
  // One cached frame can arrive before the display list, and before the
  // media_select IPC reply. Neither ordering may strand a restored phone.
  for (const holdSelect of [false, true]) {
    await load();
    await page.evaluate((hold) => {
      const f = window.fixture;
      f.heartbeat = false;
      f.hideDisplaysUntil = Date.now() + 1800;
      f.restoreDisplayDelay = 1800;
      f.holdSelect = hold;
      f.reconnectSuccess = true;
      f.retryable = true;
      f.closed = true;
    }, holdSelect);
    await page.waitForFunction(() =>
      window.fixture.calls.some((c) => c.cmd === "viewer_reconnect"),
    );
    await page.waitForFunction(() =>
      window.fixture.calls.some(
        (c) => c.cmd === "remote_status" && c.args.id === "recovered",
      ),
    );
    await page.waitForTimeout(600);
    assert.equal(
      await page.evaluate(() => Date.now() < window.fixture.hideDisplaysUntil),
      true,
    );
    assert.equal(
      await page.evaluate(
        () =>
          window.fixture.calls.filter(
            (c) => c.cmd === "media_next" && c.args.id === "recovered",
          ).length,
      ),
      0,
      "waiting for display metadata must not repeatedly copy a cached frame",
    );
    await page.waitForFunction(() =>
      window.fixture.calls.some(
        (c) => c.cmd === "media_select" && c.args.id === "recovered",
      ),
    );
    if (holdSelect)
      await page.waitForFunction(() => !!window.fixture.releaseSelect);
    await page.getByAltText("远端桌面").waitFor({ timeout: 4000 });
    assert.equal(await page.getByRole("alertdialog").count(), 0);
    if (holdSelect) {
      await page.evaluate(() => {
        window.fixture.holdSelect = false;
        window.fixture.releaseSelect();
      });
      await page.waitForTimeout(200);
      assert.equal(await page.getByAltText("远端桌面").count(), 1);
    }
  }
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
  await page.evaluate(() => {
    window.fixture.platform = "android";
    window.fixture.video.supported = false;
    window.fixture.frame(582, 1280);
  });
  await page.waitForFunction(
    () =>
      document.querySelector(".remote-screen img")?.naturalWidth === 582 &&
      !document.querySelector(".viewer-fill"),
  );
  assert.equal(
    await page.evaluate(() => localStorage.getItem("farsail.viewer.fit")),
    "fill",
    "legacy desktop preference is retained without stretching phones",
  );
  await page.waitForFunction(() =>
    window.fixture.calls.some(
      (c) =>
        c.cmd === "viewer_window_action" && c.args.action === "phone_layout",
    ),
  );
  await page.getByRole("group", { name: "手机导航", exact: true }).waitFor();
  await leaveToolbar();
  assert(
    await page.getByRole("button", { name: "主屏幕", exact: true }).isVisible(),
    "phone home must stay visible with the toolbar hidden",
  );
  assert.equal(
    await page
      .getByRole("combobox", { name: "画面分辨率", exact: true })
      .count(),
    0,
    "phones without profile switching must not be told to update",
  );
  assert.equal(
    await page.getByRole("button", { name: "传输文件", exact: true }).count(),
    0,
    "unsupported phone transfer must not obscure navigation",
  );
  assert.equal(await switchWindow().count(), 0);
  for (const [label, vk] of [
    ["返回", 0xa6],
    ["主屏幕", 0xac],
    ["最近任务", 0x5d],
  ]) {
    const before = await page.evaluate(() => window.fixture.calls.length);
    await page.getByRole("button", { name: label, exact: true }).click();
    await page.waitForFunction(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "key",
          ).length === 2,
      before,
    );
    assert.deepEqual(
      await page.evaluate(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) => c.cmd === "remote_input" && c.args.input?.kind === "key",
            )
            .map((c) => c.args.input),
        before,
      ),
      [
        { kind: "key", vk, down: true, repeat: false },
        { kind: "key", vk, down: false },
      ],
    );
  }
  await page.evaluate(() => {
    window.fixture.permission = "view";
  });
  await page.waitForTimeout(900);
  assert.equal(
    await page
      .getByRole("button", { name: "旋转画面", exact: true })
      .isEnabled(),
    true,
    "local rotation remains available in view-only mode",
  );
  assert.equal(
    await page.getByRole("group", { name: "手机导航", exact: true }).count(),
    1,
  );
  assert.equal(
    await page.locator(".viewer-phone-navigation button:disabled").count(),
    3,
    "view-only phone navigation is visible but unavailable",
  );
  const beforeViewNavigation = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page
    .getByRole("button", { name: "主屏幕", exact: true })
    .evaluate((b) =>
      b.dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
  assert.equal(
    await page.evaluate(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "key",
          ).length,
      beforeViewNavigation,
    ),
    0,
    "view-only cannot dispatch navigation",
  );
  await page.evaluate(() => {
    window.fixture.permission = "control";
  });
  await page.waitForFunction(
    () => document.querySelector(".remote-screen").tabIndex === 0,
  );
  if (await page.locator(".viewer-more[open]").count())
    await page.getByText("更多操作", { exact: true }).click();
  for (const size of [
    { width: 1200, height: 800 },
    { width: 640, height: 900 },
    { width: 360, height: 640 },
    { width: 1920, height: 1080 },
  ]) {
    await page.setViewportSize(size);
    await verifyFillPixels({ width: 582, height: 1280 });
    const navigation = await page
      .getByRole("group", { name: "手机导航", exact: true })
      .boundingBox();
    const remoteArea = await page.locator(".remote-screen").boundingBox();
    assert(
      navigation.y >= remoteArea.y + remoteArea.height - 1,
      "navigation reserves space outside the phone pixels",
    );
    assert(
      navigation.y + navigation.height <= size.height + 1,
      "navigation stays inside small windows",
    );
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      ),
      false,
    );
    const area = await page.getByAltText("远端桌面").boundingBox();
    const scale = Math.min(area.width / 582, area.height / 1280);
    const left = area.x + (area.width - 582 * scale) / 2;
    const top = area.y + (area.height - 1280 * scale) / 2;
    const before = await page.evaluate(() => window.fixture.calls.length);
    await page.mouse.click(area.x + 2, area.y + area.height / 2);
    await page.waitForTimeout(180);
    assert.equal(
      await page.evaluate(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) =>
                c.cmd === "remote_input" && c.args.input?.kind === "button",
            ).length,
        before,
      ),
      0,
      "portrait side bands must not inject clicks",
    );
    await page.mouse.click(
      left + 582 * scale * 0.25,
      top + 1280 * scale * 0.75,
    );
    await page.waitForFunction(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
          ).length === 2,
      before,
    );
    const input = await page.evaluate(
      () =>
        window.fixture.calls
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "button",
          )
          .at(-1).args.input,
    );
    assert(
      Math.abs(input.x - 0.25) < 0.005 && Math.abs(input.y - 0.75) < 0.005,
      "portrait clicks use the fitted image rectangle",
    );
  }
  // Local rotation changes pixels and inverse pointer/wheel geometry together.
  // It must never ask Android to change its orientation or require input permission.
  for (const size of [
    { width: 360, height: 640 },
    { width: 1200, height: 800 },
  ]) {
    await page.setViewportSize(size);
    for (let turns = 1; turns <= 4; turns++) {
      const rotation = turns % 4;
      await page.getByRole("button", { name: "旋转画面", exact: true }).hover();
      await page.waitForTimeout(100);
      const beforeRotate = await page.evaluate(
        () => window.fixture.calls.length,
      );
      await page.getByRole("button", { name: "旋转画面", exact: true }).click();
      await page.waitForTimeout(100);
      assert.equal(
        await page.evaluate(
          (before) =>
            window.fixture.calls
              .slice(before)
              .filter((c) => c.cmd === "remote_input" && c.args.input != null)
              .length,
          beforeRotate,
        ),
        0,
        "rotating the view sends no device input",
      );
      await verifyFillPixels(
        rotation % 2
          ? { width: 1280, height: 582 }
          : { width: 582, height: 1280 },
        rotation,
      );
      const rect = await page.locator(".remote-screen").boundingBox();
      const width = rotation % 2 ? 1280 : 582,
        height = rotation % 2 ? 582 : 1280;
      const scale = Math.min(rect.width / width, rect.height / height);
      const bounds = {
        x: rect.x + (rect.width - width * scale) / 2,
        y: rect.y + (rect.height - height * scale) / 2,
        width: width * scale,
        height: height * scale,
      };
      const beforeClick = await page.evaluate(
        () => window.fixture.calls.length,
      );
      await page.mouse.click(
        bounds.x + bounds.width * 0.25,
        bounds.y + bounds.height * 0.75,
      );
      await page.waitForFunction(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) =>
                c.cmd === "remote_input" && c.args.input?.kind === "button",
            ).length === 2,
        beforeClick,
      );
      const inputs = await page.evaluate(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) =>
                c.cmd === "remote_input" && c.args.input?.kind === "button",
            )
            .map((c) => c.args.input),
        beforeClick,
      );
      const expected = [
        [0.25, 0.75],
        [0.75, 0.75],
        [0.75, 0.25],
        [0.25, 0.25],
      ][rotation];
      assert(
        inputs.every(
          (p) =>
            Math.abs(p.x - expected[0]) < 0.006 &&
            Math.abs(p.y - expected[1]) < 0.006,
        ),
        "rotated clicks map back to unrotated phone coordinates",
      );
      const beforeDrag = await page.evaluate(() => window.fixture.calls.length);
      await page.mouse.down();
      await page.mouse.move(
        bounds.x + bounds.width * 0.75,
        bounds.y + bounds.height * 0.25,
        { steps: 3 },
      );
      await page.mouse.up();
      await page.waitForFunction(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) =>
                c.cmd === "remote_input" && c.args.input?.kind === "button",
            ).length === 2,
        beforeDrag,
      );
      const dragEnd = await page.evaluate(
        (before) =>
          window.fixture.calls
            .slice(before)
            .filter(
              (c) =>
                c.cmd === "remote_input" && c.args.input?.kind === "button",
            )
            .at(-1).args.input,
        beforeDrag,
      );
      const expectedEnd = [
        [0.75, 0.25],
        [0.25, 0.25],
        [0.25, 0.75],
        [0.75, 0.75],
      ][rotation];
      assert(
        Math.abs(dragEnd.x - expectedEnd[0]) < 0.006 &&
          Math.abs(dragEnd.y - expectedEnd[1]) < 0.006,
        "rotated drags finish at the corresponding unrotated source point",
      );
      const beforeWheel = await page.evaluate(
        () => window.fixture.calls.length,
      );
      await page.mouse.wheel(20, 120);
      await page.waitForFunction(
        (before) =>
          window.fixture.calls
            .slice(before)
            .some(
              (c) => c.cmd === "remote_input" && c.args.input?.kind === "wheel",
            ),
        beforeWheel,
      );
      const wheel = await page.evaluate(
        (before) =>
          window.fixture.calls
            .slice(before)
            .find(
              (c) => c.cmd === "remote_input" && c.args.input?.kind === "wheel",
            ).args.input,
        beforeWheel,
      );
      const expectedWheel = [
        [20, -120],
        [120, 20],
        [-20, 120],
        [-120, -20],
      ][rotation];
      assert.deepEqual([wheel.horizontal, wheel.vertical], expectedWheel);
      // Test the black bands along whichever axis is not filled.
      const beforeBand = await page.evaluate(() => window.fixture.calls.length);
      await page.mouse.click(
        bounds.x > rect.x + 2 ? rect.x + 1 : rect.x + rect.width / 2,
        bounds.x > rect.x + 2 ? rect.y + rect.height / 2 : rect.y + 1,
      );
      await page.waitForTimeout(100);
      assert.equal(
        await page.evaluate(
          (before) =>
            window.fixture.calls
              .slice(before)
              .filter(
                (c) =>
                  c.cmd === "remote_input" && c.args.input?.kind === "button",
              ).length,
          beforeBand,
        ),
        0,
        "rotated letterbox bars reject clicks",
      );
      assert.equal(
        await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth,
        ),
        false,
      );
      if (rotation === 1 && size.width === 1200)
        await page.screenshot({
          path: ".local/ui-verification/viewer-phone-rotated.png",
        });
    }
  }
  await page.getByRole("button", { name: "旋转画面", exact: true }).click();
  await revealToolbar();
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByRole("button", { name: "铺满窗口", exact: true }).click();
  await page.getByText("更多操作", { exact: true }).click();
  await verifyFillPixels(null, 1);
  await revealToolbar();
  await page.getByText("更多操作", { exact: true }).click();
  await page.getByRole("button", { name: "还原画面方向", exact: true }).click();
  await page.getByRole("button", { name: "保持比例", exact: true }).click();
  await page.getByText("更多操作", { exact: true }).click();
  await verifyFillPixels({ width: 582, height: 1280 });
  await page.evaluate(() => {
    window.fixture.inputBlocked = true;
    window.fixture.inputGeneration++;
  });
  await page.getByRole("button", { name: "主屏幕", exact: true }).waitFor();
  await page.waitForFunction(
    () => document.querySelector(".viewer-phone-navigation button")?.disabled,
  );
  const beforePausedNavigation = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page
    .getByRole("button", { name: "主屏幕", exact: true })
    .evaluate((b) =>
      b.dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
  assert.equal(
    await page.evaluate(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "key",
          ).length,
      beforePausedNavigation,
    ),
    0,
    "paused input cannot use phone navigation",
  );
  await page.evaluate(() => {
    window.fixture.inputBlocked = false;
    window.fixture.inputGeneration++;
  });
  await page.waitForFunction(
    () => !document.querySelector(".viewer-phone-navigation button")?.disabled,
  );
  const beforeKeyboardHome = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page.getByRole("button", { name: "主屏幕", exact: true }).focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(
    (before) =>
      window.fixture.calls
        .slice(before)
        .filter((c) => c.cmd === "remote_input" && c.args.input?.kind === "key")
        .length === 2,
    beforeKeyboardHome,
  );
  const homeKeys = await page.evaluate(
    (before) =>
      window.fixture.calls
        .slice(before)
        .filter((c) => c.cmd === "remote_input" && c.args.input?.kind === "key")
        .map((c) => c.args.input.vk),
    beforeKeyboardHome,
  );
  assert.deepEqual(
    homeKeys,
    [0xac, 0xac],
    "keyboard activation cannot leak Enter into the phone",
  );
  await revealToolbar();
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
  assert(
    await page
      .getByRole("group", { name: "手机导航", exact: true })
      .isVisible(),
    "fullscreen keeps phone navigation accessible",
  );
  await page.screenshot({
    path: ".local/ui-verification/viewer-phone-navigation-fullscreen.png",
  });
  await revealToolbar();
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
  await page.setViewportSize({ width: 640, height: 900 });
  await leaveToolbar();
  await page.screenshot({
    path: ".local/ui-verification/viewer-phone-navigation.png",
  });
  await page.evaluate(() => window.fixture.frame(1280, 582));
  await page.waitForFunction(
    () => document.querySelector(".remote-screen img")?.naturalWidth === 1280,
  );
  await verifyFillPixels({ width: 1280, height: 582 });
  await page.waitForFunction(
    () =>
      window.fixture.calls
        .filter(
          (c) =>
            c.cmd === "viewer_window_action" &&
            c.args.action === "phone_layout",
        )
        .at(-1)?.args.landscape === true,
  );
  const phoneLayouts = await page.evaluate(() =>
    window.fixture.calls
      .filter(
        (c) =>
          c.cmd === "viewer_window_action" && c.args.action === "phone_layout",
      )
      .map((c) => c.args.landscape),
  );
  assert(
    phoneLayouts.includes(false) && phoneLayouts.includes(true),
    "phone and game orientations request automatic layout",
  );
  assert(
    phoneLayouts.every(
      (orientation, index) =>
        index === 0 || orientation !== phoneLayouts[index - 1],
    ),
    "native window shape changes only when orientation changes",
  );
  await page.setViewportSize({ width: 720, height: 820 });
  await page.waitForTimeout(1700);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter(
          (c) =>
            c.cmd === "viewer_window_action" &&
            c.args.action === "phone_layout",
        ).length,
    ),
    phoneLayouts.length,
    "ordinary polling must preserve the user's manual window size",
  );
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => {
    window.fixture.closed = true;
  });
  await page.waitForFunction(
    () => document.querySelector(".viewer-phone-navigation button")?.disabled,
  );
  const beforeEndedNavigation = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page
    .getByRole("button", { name: "主屏幕", exact: true })
    .evaluate((b) =>
      b.dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
  assert.equal(
    await page.evaluate(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) => c.cmd === "remote_input" && c.args.input?.kind === "key",
          ).length,
      beforeEndedNavigation,
    ),
    0,
    "ended sessions cannot use phone navigation",
  );
  // Three simultaneously connected tabs keep state and input scoped. Hidden
  // tabs stop JPEG decode work, but are not disconnected/recreated on selection.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = [
      {
        instance: "phone-a",
        id: "session-a",
        name: "测试手机 A",
        selected: true,
      },
      {
        instance: "phone-b",
        id: "session-b",
        name: "测试手机 B",
        selected: false,
      },
      {
        instance: "phone-c",
        id: "session-c",
        name: "测试手机 C",
        selected: false,
      },
    ];
  });
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  await page.getByRole("tabpanel").getByAltText("远端桌面").waitFor();
  assert.equal(await page.locator(".viewer-tab-panel").count(), 3);
  assert.equal(await page.getByRole("tabpanel").count(), 1);
  await page.getByRole("tab", { name: "测试手机 B", exact: true }).click();
  await page.getByRole("tabpanel").getByAltText("远端桌面").waitFor();
  await page.getByRole("tabpanel").getByAltText("远端桌面").click();
  const beforeKey = await page.evaluate(() => window.fixture.calls.length);
  await page.keyboard.press("J");
  await page.waitForFunction(
    (before) =>
      window.fixture.calls
        .slice(before)
        .some(
          (c) =>
            c.cmd === "remote_input" &&
            c.args.id === "session-b" &&
            c.args.input?.vk === 74,
        ),
    beforeKey,
  );
  assert.equal(
    await page.evaluate(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (c) =>
              c.cmd === "remote_input" &&
              c.args.input?.kind === "key" &&
              c.args.id !== "session-b",
          ).length,
      beforeKey,
    ),
    0,
  );
  await page.keyboard.press("Control+Tab");
  await page.waitForFunction(
    () =>
      document.querySelector('[role="tab"][aria-selected="true"]')
        ?.textContent === "测试手机 C",
  );
  await page.getByRole("tabpanel").getByAltText("远端桌面").waitFor();
  await page.screenshot({
    path: ".local/ui-verification/viewer-three-tabs.png",
  });
  await page
    .getByRole("button", { name: "关闭标签 测试手机 B", exact: true })
    .click();
  await page.waitForFunction(
    () => document.querySelectorAll(".viewer-tab-panel").length === 2,
  );
  assert.equal(
    await page
      .getByRole("tab", { name: "测试手机 C", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
  await page.keyboard.press("Control+w");
  await page.waitForFunction(
    () => document.querySelectorAll(".viewer-tab-panel").length === 1,
  );
  assert.equal(
    await page
      .getByRole("tab", { name: "测试手机 A", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    0,
    "switching/closing tabs does not reconnect other phones",
  );
  // Detaching an existing tab is a local ownership change, with no new
  // approval/connection request and no stop command for its siblings.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = [
      {
        instance: "detach-a",
        id: "session-a",
        name: "拆分手机 A",
        selected: true,
      },
      {
        instance: "detach-b",
        id: "session-b",
        name: "拆分手机 B",
        selected: false,
      },
    ];
  });
  await page.getByRole("tab", { name: "拆分手机 A", exact: true }).waitFor();
  const beforeDetach = await page.evaluate(() => window.fixture.calls.length);
  await page
    .getByRole("button", { name: "当前标签在独立窗口打开", exact: true })
    .click();
  await page.waitForFunction(() => window.fixture.tabs.length === 1);
  await page.getByRole("tab", { name: "拆分手机 B", exact: true }).waitFor();
  const detachCalls = await page.evaluate(
    (before) => window.fixture.calls.slice(before),
    beforeDetach,
  );
  assert.equal(
    detachCalls.filter((c) => c.cmd === "viewer_detach_tab").length,
    1,
  );
  assert.equal(
    detachCalls.filter((c) =>
      [
        "viewer_reconnect",
        "transport_close",
        "viewer_close_tab",
        "transport_connect",
      ].includes(c.cmd),
    ).length,
    0,
  );
  assert.equal(
    await page
      .getByRole("button", { name: "当前标签在独立窗口打开", exact: true })
      .isDisabled(),
    true,
  );
  // Five sessions remain available by mouse in fullscreen, independently of
  // the remote-command toolbar. Popup navigation must stay on the local side.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = ["A", "B", "C", "D", "E"].map((letter, index) => ({
      instance: `phone-${letter.toLowerCase()}`,
      id: `session-${letter.toLowerCase()}`,
      name: `测试手机 ${letter}`,
      selected: index === 0,
    }));
  });
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  const activePanel = () => page.getByRole("tabpanel");
  const deviceSwitcher = () =>
    page.getByRole("button", {
      name: "切换远程设备",
      exact: true,
    });
  const popup = () => page.locator(".viewer-tab-menu-list");
  await activePanel().getByAltText("远端桌面").waitFor();
  await page.mouse.move(500, 600);
  await activePanel().locator(".remote-screen").focus();
  await activePanel().locator(".viewer-toolbar").waitFor({ state: "hidden" });
  const beforeLocalMenu = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await deviceSwitcher().press("ArrowDown");
  await page.waitForFunction(
    () => document.activeElement?.getAttribute("aria-label") === "测试手机 A",
  );
  await page.keyboard.press("End");
  await page.waitForFunction(
    () => document.activeElement?.getAttribute("aria-label") === "测试手机 E",
  );
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowDown");
  await page.waitForFunction(
    () => document.activeElement?.getAttribute("aria-label") === "测试手机 B",
  );
  await page.keyboard.press("ArrowUp");
  await page.waitForFunction(
    () => document.activeElement?.getAttribute("aria-label") === "测试手机 A",
  );
  await page.keyboard.press("Escape");
  await popup().waitFor({ state: "hidden" });
  assert.equal(
    await deviceSwitcher().evaluate(
      (button) => button === document.activeElement,
    ),
    true,
  );
  assert.equal(
    await page
      .getByRole("tab", { name: "测试手机 A", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
  assert(
    !(await activePanel().locator(".viewer-toolbar").isVisible()),
    "device navigation cannot expand remote controls",
  );
  // A small portrait window must keep the full popup inside its client area.
  await page.setViewportSize({ width: 320, height: 640 });
  await deviceSwitcher().press("End");
  const normalMenuBounds = await popup().boundingBox();
  assert(
    normalMenuBounds &&
      normalMenuBounds.x >= 0 &&
      normalMenuBounds.x + normalMenuBounds.width <= 320,
  );
  assert(
    normalMenuBounds.y >= 0 &&
      normalMenuBounds.y + normalMenuBounds.height <= 640,
  );
  await page.keyboard.press("Tab");
  assert.equal(
    await popup()
      .getByRole("button", { name: "当前设备在独立窗口打开", exact: true })
      .evaluate((button) => button === document.activeElement),
    true,
  );
  await page.keyboard.press("Tab");
  await popup().waitFor({ state: "hidden" });
  assert.equal(
    await page
      .getByRole("button", { name: "连接其他设备", exact: true })
      .evaluate((button) => button === document.activeElement),
    true,
    "Tab leaves the list and closes it without taking focus back",
  );
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.getByRole("button", { name: "显示工具栏", exact: true }).click();
  // A native drag can reply after a later fullscreen action. Its old snapshot
  // must not overwrite window-level state shared by the other sessions.
  await page.evaluate(() => {
    window.fixture.deferWindowAction = "drag";
  });
  await activePanel()
    .locator(".viewer-titlebar .window-drag")
    .dispatchEvent("mousedown", {
      button: 0,
      detail: 1,
    });
  await page.waitForFunction(() => !!window.fixture.releaseWindowAction);
  await page.getByRole("button", { name: "全屏切换", exact: true }).click();
  await page.waitForFunction(
    () => document.querySelectorAll(".viewer-titlebar").length === 0,
  );
  await page.evaluate(async () => {
    window.fixture.releaseWindowAction();
    window.fixture.deferWindowAction = null;
    await new Promise(requestAnimationFrame);
  });
  assert.equal(
    await page.locator(".viewer-titlebar").count(),
    0,
    "a late non-fullscreen action cannot restore an outdated fullscreen snapshot",
  );
  await activePanel().locator(".remote-screen").focus();
  await page.getByRole("button", { name: "退出全屏", exact: true }).hover();
  await activePanel().locator(".viewer-toolbar").waitFor({ state: "hidden" });
  for (const letter of ["E", "C", "B", "D", "A"]) {
    await deviceSwitcher().click();
    await popup()
      .getByRole("button", { name: `测试手机 ${letter}`, exact: true })
      .click();
    await popup().waitFor({ state: "hidden" });
    await page.waitForFunction(
      (letter) =>
        document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
        `panel-phone-${letter.toLowerCase()}`,
      letter,
    );
    await activePanel().getByAltText("远端桌面").waitFor();
    await page.waitForFunction(
      () =>
        document.querySelector(
          ".viewer-tab-panel:not([hidden]) .viewer-tab-menu-trigger",
        ) === document.activeElement,
    );
    assert.equal(await page.evaluate(() => window.fixture.fullscreen), true);
    assert.equal(
      await page.locator(".viewer-titlebar").count(),
      0,
      "switching a fullscreen session cannot restore ordinary titlebars",
    );
    assert(
      !(await activePanel().locator(".viewer-toolbar").isVisible()),
      "switching devices keeps remote tools hidden",
    );
    const canvas = await activePanel().locator(".remote-screen").boundingBox();
    assert(
      canvas && canvas.y === 0 && canvas.height === 800,
      "fullscreen canvas must not retain the ordinary titlebar padding",
    );
    assert(
      (
        await page.locator(".viewer-fullscreen-device-name").innerText()
      ).includes(`测试手机 ${letter}`),
    );
  }
  await page.setViewportSize({ width: 440, height: 900 });
  await deviceSwitcher().click();
  const fullscreenMenuBounds = await popup().boundingBox();
  assert(
    fullscreenMenuBounds &&
      fullscreenMenuBounds.x >= 0 &&
      fullscreenMenuBounds.x + fullscreenMenuBounds.width <= 440,
  );
  assert(
    fullscreenMenuBounds.y >= 0 &&
      fullscreenMenuBounds.y + fullscreenMenuBounds.height <= 900,
  );
  await page.screenshot({
    path: ".local/ui-verification/viewer-five-fullscreen-menu.png",
  });
  // The first click on the canvas dismisses the popup; it is not a remote click.
  await page.mouse.click(350, 600);
  await popup().waitFor({ state: "hidden" });
  assert(!(await activePanel().locator(".viewer-toolbar").isVisible()));
  const localMenuCalls = await page.evaluate(
    (before) => window.fixture.calls.slice(before),
    beforeLocalMenu,
  );
  assert.equal(
    localMenuCalls.filter(
      (call) => call.cmd === "remote_input" && call.args.input != null,
    ).length,
    0,
    "menu keys and backdrop clicks must not reach any remote device",
  );
  assert.equal(
    localMenuCalls.filter((call) => call.cmd === "viewer_reconnect").length,
    0,
    "fullscreen switching keeps existing sessions connected",
  );
  assert.equal(
    localMenuCalls.filter((call) => call.cmd === "viewer_close_tab").length,
    0,
    "choosing a device must not close another connection",
  );
  await page.getByRole("button", { name: "退出全屏", exact: true }).click();
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.fixture.fullscreen), false);
  assert(
    !(await activePanel().locator(".viewer-toolbar").isVisible()),
    "independent fullscreen exit keeps remote tools hidden",
  );
  await page.setViewportSize({ width: 1200, height: 800 });
  // Overflow navigation browses tabs locally, and closing keeps the user's place.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = ["A", "B", "C", "D", "E"].map((letter, index) => ({
      instance: `phone-${letter.toLowerCase()}`,
      id: `session-${letter.toLowerCase()}`,
      name: `测试手机 ${letter}`,
      selected: index === 0,
    }));
  });
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  await activePanel().getByAltText("远端桌面").waitFor();
  await page.mouse.move(500, 600);
  await activePanel().locator(".remote-screen").focus();
  await activePanel().locator(".viewer-toolbar").waitFor({ state: "hidden" });
  const tabViewport = () =>
    page.getByRole("tablist", { name: "远程设备标签", exact: true });
  await page.waitForFunction(() => {
    const list = document.querySelector(
      ".viewer-tab-panel:not([hidden]) .viewer-tabs",
    );
    return list && list.scrollWidth <= list.clientWidth + 1;
  });
  assert.equal(
    await page
      .getByRole("button", { name: "向右滚动设备标签", exact: true })
      .count(),
    0,
    "five short device names fit without overflow in a wide window",
  );
  await page.screenshot({
    path: ".local/ui-verification/viewer-five-compact-tabs.png",
  });
  await page.setViewportSize({ width: 440, height: 900 });
  const scrollRight = () =>
    page.getByRole("button", { name: "向右滚动设备标签", exact: true });
  const scrollLeft = () =>
    page.getByRole("button", { name: "向左滚动设备标签", exact: true });
  await scrollRight().waitFor();
  const beforeBrowsing = await page.evaluate(() => window.fixture.calls.length);
  await scrollRight().click();
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden]) .viewer-tabs")
        .scrollLeft > 40,
  );
  await scrollLeft().click();
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden]) .viewer-tabs")
        .scrollLeft < 1,
  );
  await tabViewport().hover();
  await page.mouse.wheel(0, 180);
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden]) .viewer-tabs")
        .scrollLeft > 100,
  );
  assert.equal(
    await page
      .getByRole("tab", { name: "测试手机 A", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
  const browseCalls = await page.evaluate(
    (before) => window.fixture.calls.slice(before),
    beforeBrowsing,
  );
  assert.equal(
    browseCalls.filter((c) => c.cmd === "viewer_select_tab").length,
    0,
    "scroll buttons and wheel only browse; they do not switch devices",
  );
  assert.equal(
    browseCalls.filter((c) => c.cmd === "remote_input" && c.args.input != null)
      .length,
    0,
  );
  assert(!(await activePanel().locator(".viewer-toolbar").isVisible()));
  await deviceSwitcher().click();
  await popup()
    .getByRole("button", { name: "测试手机 E", exact: true })
    .click();
  const activeTab = () => page.getByRole("tab", { selected: true });
  await page.getByRole("tab", { name: "测试手机 E", exact: true }).waitFor();
  const checkSelectedInView = async () => {
    await page.waitForFunction(() => {
      const list = document.querySelector(
        ".viewer-tab-panel:not([hidden]) .viewer-tabs",
      );
      const tab = list?.querySelector(
        '[role="tab"][aria-selected="true"]',
      )?.parentElement;
      if (!list || !tab) return false;
      const outer = list.getBoundingClientRect(),
        inner = tab.getBoundingClientRect();
      return inner.left >= outer.left - 1 && inner.right <= outer.right + 1;
    });
  };
  await checkSelectedInView();
  await activeTab().press("Home");
  await page.waitForFunction(
    () => document.activeElement?.id === "tab-phone-a",
  );
  await checkSelectedInView();
  await activeTab().press("ArrowRight");
  await page.waitForFunction(
    () => document.activeElement?.id === "tab-phone-b",
  );
  await checkSelectedInView();
  await activeTab().press("End");
  await page.waitForFunction(
    () => document.activeElement?.id === "tab-phone-e",
  );
  await checkSelectedInView();
  await activePanel().getByAltText("远端桌面").waitFor();
  await page.screenshot({
    path: ".local/ui-verification/viewer-five-overflow-tabs.png",
  });
  for (const width of [440, 320]) {
    await page.setViewportSize({ width, height: 900 });
    await checkSelectedInView();
    const drag = await activePanel()
      .locator(".viewer-titlebar .window-drag")
      .boundingBox();
    assert(
      drag && drag.width >= 44,
      "native titlebar dragging keeps a usable hit area",
    );
    for (const label of [
      "连接其他设备",
      "切换远程设备",
      "最小化",
      "最大化或还原",
      "关闭窗口",
    ]) {
      const control = await page
        .getByRole("button", { name: label, exact: true })
        .boundingBox();
      assert(
        control && control.x >= 0 && control.x + control.width <= width + 1,
        `${label} stays inside a ${width}px window`,
      );
    }
  }
  await page.setViewportSize({ width: 1200, height: 800 });
  const closeAndExpect = async (closing, next) => {
    await page
      .getByRole("button", {
        name: `关闭标签 测试手机 ${closing}`,
        exact: true,
      })
      .click();
    await page.waitForFunction(
      (letter) =>
        document.querySelector('[role="tab"][aria-selected="true"]')?.id ===
        `tab-phone-${letter.toLowerCase()}`,
      next,
    );
    await page.waitForFunction(
      (letter) =>
        document.activeElement?.id === `tab-phone-${letter.toLowerCase()}`,
      next,
    );
  };
  await closeAndExpect("E", "D");
  await closeAndExpect("D", "C");
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).click();
  await closeAndExpect("B", "A");
  await closeAndExpect("A", "C");
  assert.equal(await page.locator(".viewer-tab-panel").count(), 1);
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect").length,
    ),
    0,
  );
  assert(!(await activePanel().locator(".viewer-toolbar").isVisible()));
  // Rapid local choices outrank old snapshots, and native mutations stay ordered.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = ["A", "B", "C", "D", "E"].map((letter, index) => ({
      instance: `phone-${letter.toLowerCase()}`,
      id: `session-${letter.toLowerCase()}`,
      name: `测试手机 ${letter}`,
      selected: index === 0,
    }));
  });
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  await page.evaluate(() => {
    window.fixture.holdTabSnapshot = true;
  });
  await page.waitForFunction(() => !!window.fixture.releaseTabSnapshot);
  await page.evaluate(() => {
    window.fixture.holdTabMutation = "phone-b";
  });
  const rapidStart = await page.evaluate(() => window.fixture.calls.length);
  await page.getByRole("tab", { name: "测试手机 B", exact: true }).click();
  await page.waitForFunction(() => !!window.fixture.releaseTabMutation);
  await page.getByRole("tab", { name: "测试手机 C", exact: true }).click();
  await page.getByRole("tab", { name: "测试手机 E", exact: true }).click();
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
      "panel-phone-e",
  );
  await page.evaluate(() => {
    const f = window.fixture;
    f.rapidSelections = [];
    f.selectionObserver = new MutationObserver(() => {
      f.rapidSelections.push(
        document.querySelector(".viewer-tab-panel:not([hidden])")?.id,
      );
    });
    f.selectionObserver.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ["hidden"],
    });
    f.releaseTabSnapshot();
    f.releaseTabSnapshot = null;
  });
  await page.waitForFunction(() => window.fixture.tabSnapshotInFlight === 0);
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  assert.equal(
    await page.evaluate(
      () => document.querySelector(".viewer-tab-panel:not([hidden])")?.id,
    ),
    "panel-phone-e",
    "an old A snapshot cannot replace the newer E intent while B is pending",
  );
  assert.equal(
    await page.evaluate(() => window.fixture.tabMutationInFlight),
    1,
  );
  await page.evaluate(() => {
    window.fixture.releaseTabMutation();
    window.fixture.releaseTabMutation = null;
  });
  await page.waitForFunction(
    () =>
      window.fixture.tabMutationInFlight === 0 &&
      window.fixture.tabs.find((tab) => tab.selected)?.instance === "phone-e",
  );
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
      "panel-phone-e",
  );
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  const rapidResult = await page.evaluate((before) => {
    window.fixture.selectionObserver.disconnect();
    return {
      maxMutations: window.fixture.tabMutationMaxInFlight,
      maxSnapshots: window.fixture.tabSnapshotMaxInFlight,
      selections: window.fixture.rapidSelections,
      calls: window.fixture.calls.slice(before),
      applied: window.fixture.appliedTabMutations,
    };
  }, rapidStart);
  assert.equal(
    rapidResult.maxMutations,
    1,
    "native tab commands must not overtake each other",
  );
  assert.equal(
    rapidResult.maxSnapshots,
    1,
    "poll and post-mutation refresh share one native read",
  );
  assert(
    rapidResult.selections.every((id) => id === "panel-phone-e"),
    "late native work cannot flash an older selection",
  );
  assert.deepEqual(
    rapidResult.applied.map((item) => item.instance),
    ["phone-b", "phone-c", "phone-e"],
  );
  assert.equal(
    rapidResult.calls.filter(
      (call) => call.cmd === "remote_input" && call.args.input != null,
    ).length,
    0,
    "local rapid tab choices never inject remote input",
  );
  assert.equal(
    rapidResult.calls.filter((call) => call.cmd === "viewer_reconnect").length,
    0,
  );
  await activePanel().getByAltText("远端桌面").waitFor();
  await activePanel().locator(".remote-screen").focus();
  const routedAfterRapid = await page.evaluate(
    () => window.fixture.calls.length,
  );
  await page.keyboard.press("J");
  await page.waitForFunction(
    (before) =>
      window.fixture.calls
        .slice(before)
        .filter(
          (call) =>
            call.cmd === "remote_input" && call.args.input?.kind === "key",
        ).length === 2,
    routedAfterRapid,
  );
  assert(
    await page.evaluate(
      (before) =>
        window.fixture.calls
          .slice(before)
          .filter(
            (call) =>
              call.cmd === "remote_input" && call.args.input?.kind === "key",
          )
          .every((call) => call.args.id === "session-e"),
      routedAfterRapid,
    ),
  );
  // A rejected newest choice rolls back to native truth and remains visible as an error.
  await page.evaluate(() => {
    window.fixture.failTabSelect = "phone-b";
  });
  await page.getByRole("tab", { name: "测试手机 B", exact: true }).click();
  await page
    .getByRole("alertdialog", { name: "操作未完成", exact: true })
    .waitFor();
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
      "panel-phone-e",
  );
  assert.equal(
    await page.evaluate(
      () => window.fixture.tabs.find((tab) => tab.selected)?.instance,
    ),
    "phone-e",
  );
  await page.getByRole("button", { name: "知道了", exact: true }).click();
  // Closing a queued background tab cannot overtake the older select or undo E.
  await load();
  await page.evaluate(() => {
    window.fixture.tabs = ["A", "B", "C", "D", "E"].map((letter, index) => ({
      instance: `phone-${letter.toLowerCase()}`,
      id: `session-${letter.toLowerCase()}`,
      name: `测试手机 ${letter}`,
      selected: index === 0,
    }));
    window.fixture.holdTabMutation = "phone-b";
  });
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).waitFor();
  await page.getByRole("tab", { name: "测试手机 B", exact: true }).click();
  await page.waitForFunction(() => !!window.fixture.releaseTabMutation);
  await page
    .getByRole("button", { name: "关闭标签 测试手机 C", exact: true })
    .click();
  await page.getByRole("tab", { name: "测试手机 E", exact: true }).click();
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
      "panel-phone-e",
  );
  assert.equal(
    await page.evaluate(() => window.fixture.tabMutationInFlight),
    1,
  );
  assert.equal(
    await page.locator(".viewer-tab-panel").count(),
    5,
    "a queued close waits for native execution",
  );
  await page.evaluate(() => {
    window.fixture.releaseTabMutation();
    window.fixture.releaseTabMutation = null;
  });
  await page.waitForFunction(
    () =>
      window.fixture.tabMutationInFlight === 0 &&
      window.fixture.tabs.length === 4 &&
      window.fixture.tabs.find((tab) => tab.selected)?.instance === "phone-e",
  );
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".viewer-tab-panel").length === 4 &&
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
        "panel-phone-e",
  );
  const closeOrder = await page.evaluate(
    () => window.fixture.appliedTabMutations,
  );
  assert.deepEqual(closeOrder, [
    { cmd: "viewer_select_tab", instance: "phone-b" },
    { cmd: "viewer_close_tab", instance: "phone-c" },
    { cmd: "viewer_select_tab", instance: "phone-e" },
  ]);
  assert.equal(
    await page.evaluate(() => window.fixture.tabMutationMaxInFlight),
    1,
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.fixture.calls.filter((call) => call.cmd === "viewer_reconnect")
          .length,
    ),
    0,
  );
  // A failed close must remain visible even when a later choice succeeds.
  await page.evaluate(() => {
    window.fixture.holdTabMutation = "phone-b";
    window.fixture.failTabClose = "phone-d";
    window.fixture.appliedTabMutations = [];
  });
  await page.getByRole("tab", { name: "测试手机 B", exact: true }).click();
  await page.waitForFunction(() => !!window.fixture.releaseTabMutation);
  await page
    .getByRole("button", { name: "关闭标签 测试手机 D", exact: true })
    .click();
  await page.getByRole("tab", { name: "测试手机 A", exact: true }).click();
  await page.evaluate(() => {
    window.fixture.releaseTabMutation();
    window.fixture.releaseTabMutation = null;
  });
  await page
    .getByRole("alertdialog", { name: "操作未完成", exact: true })
    .waitFor();
  await page.waitForFunction(
    () =>
      window.fixture.tabMutationInFlight === 0 &&
      window.fixture.tabs.find((tab) => tab.selected)?.instance === "phone-a",
  );
  await page.waitForFunction(
    () =>
      document.querySelector(".viewer-tab-panel:not([hidden])")?.id ===
      "panel-phone-a",
  );
  assert(
    await page
      .getByRole("alertdialog", { name: "操作未完成", exact: true })
      .locator(".failure-dialog-content p")
      .isVisible(),
  );
  await page.getByRole("button", { name: "知道了", exact: true }).click();
  assert.equal(await page.locator(".viewer-tab-panel").count(), 4);
  assert(
    await page
      .getByRole("tab", { name: "测试手机 D", exact: true })
      .isVisible(),
  );
  assert.deepEqual(
    await page.evaluate(() => window.fixture.appliedTabMutations),
    [
      { cmd: "viewer_select_tab", instance: "phone-b" },
      { cmd: "viewer_select_tab", instance: "phone-a" },
    ],
  );
  assert.equal(
    await page.evaluate(() => window.fixture.tabMutationMaxInFlight),
    1,
  );
  // Frame failure and timer polling must share a single status request, even
  // when StrictMode replays effects or a slow native status call is in flight.
  await load();
  await page.evaluate(() => {
    window.fixture.statusDelay = 1800;
    window.fixture.statusMaxInFlight = window.fixture.statusInFlight;
  });
  await page.waitForFunction(() => window.fixture.statusInFlight === 1);
  await page.evaluate(() => {
    window.fixture.retryable = true;
    window.fixture.closed = true;
  });
  await page.waitForFunction(() =>
    window.fixture.calls.some((c) => c.cmd === "viewer_reconnect"),
  );
  assert.equal(
    await page.evaluate(() => window.fixture.statusMaxInFlight),
    1,
    "periodic and frame-error status checks must share a request",
  );
  assert.deepEqual(errors, []);
  async function testAdaptiveViewports() {
    // Four independent Android windows negotiate only their own pixel budgets.
    // A congested native-size first frame must not be required to negotiate a
    // smaller stream. Metadata is already authenticated and carries geometry.
    const startup = await browser.newPage({
      viewport: { width: 640, height: 440 },
    });
    await startup.addInitScript(installFixture, { viewportStartup: true });
    await startup.goto(
      process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test",
    );
    try {
      await startup.waitForFunction(
        () => window.fixture.video.viewport_edge >= 320,
      );
    } catch (cause) {
      const snapshot = await startup.evaluate(() => ({
        video: window.fixture.video,
        requests: window.fixture.calls.filter(
          (c) => c.cmd === "media_viewport",
        ),
        hasImage: !!document.querySelector(".remote-screen img"),
      }));
      throw new Error(
        `Viewport metadata startup failed: ${JSON.stringify(snapshot)}`,
        { cause },
      );
    }
    await startup.getByAltText("远端桌面").waitFor();
    assert.equal(
      await startup.evaluate(
        () =>
          window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect")
            .length,
      ),
      0,
    );
    await startup.close();
    const windows = [];
    for (const width of [640, 720, 800, 960]) {
      const p = await browser.newPage({ viewport: { width, height: 440 } });
      p.on("pageerror", (e) => errors.push(e.message));
      await p.addInitScript(installFixture);
      await p.goto(process.env.UI_URL || "http://127.0.0.1:1420/?viewer=test");
      await p.getByAltText("远端桌面").waitFor();
      await p.waitForFunction(() => window.fixture.video.generation > 0);
      await p.waitForTimeout(300);
      await p.evaluate(() => {
        window.fixture.platform = "android";
        window.fixture.video.max_profile = 2;
        window.fixture.video.viewport_supported = true;
      });
      await p.waitForFunction(() => window.fixture.video.viewport_edge >= 320);
      windows.push(p);
    }
    const edges = await Promise.all(
      windows.map((p) => p.evaluate(() => window.fixture.video.viewport_edge)),
    );
    assert(
      edges[0] < edges[3],
      "each independent window uses its own viewport size",
    );
    await windows[0].setViewportSize({ width: 1800, height: 1000 });
    await windows[0].waitForFunction(
      (previous) => window.fixture.video.viewport_edge > previous,
      edges[0],
    );
    for (let i = 1; i < windows.length; i++) {
      assert.equal(
        await windows[i].evaluate(() => window.fixture.video.viewport_edge),
        edges[i],
      );
    }
    await windows[0]
      .getByLabel("画面分辨率")
      .selectOption("2", { force: true });
    await windows[0].waitForFunction(
      () => window.fixture.video.viewport_edge === 0,
    );
    const autoRequests = await windows[0].evaluate(
      () =>
        window.fixture.calls.filter((c) => c.cmd === "media_viewport").length,
    );
    await windows[0].setViewportSize({ width: 640, height: 440 });
    await windows[0].waitForTimeout(1100);
    assert.equal(
      await windows[0].evaluate(
        () =>
          window.fixture.calls.filter((c) => c.cmd === "media_viewport").length,
      ),
      autoRequests,
      "manual native choice survives resizing",
    );
    await windows[0]
      .getByLabel("画面分辨率")
      .selectOption("-2", { force: true });
    await windows[0].waitForFunction(
      () => window.fixture.video.viewport_edge >= 320,
    );
    for (const p of windows) {
      assert.equal(
        await p.evaluate(
          () =>
            window.fixture.calls.filter((c) => c.cmd === "viewer_reconnect")
              .length,
        ),
        0,
      );
      await p.close();
    }
    assert.deepEqual(errors, []);
  }
  await testAdaptiveViewports();
  await browser.close();
  console.log(
    "PASS repeated left/right pairs, multi-button capture, queued outside UP then release-all, external held entry, blur/lost-capture cancellation, ordered remote Alt+Tab and view/paused/ended guards; fit/quality/FPS/pause/terminal/read-only, five-session fullscreen/overflow navigation, adjacent close, rapid ordered tab changes, manual-only capture recovery and local popup isolation coverage. Synthetic IPC only.",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
