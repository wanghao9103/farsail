// Synthetic native IPC checks. No real account, update package, or installer is used.
// Run against the desktop Vite server on 1420 with Playwright on NODE_PATH.
const { chromium, webkit } = require("playwright");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const engineName = process.env.UI_ENGINE === "webkit" ? "webkit" : "chromium";
const directory = path.join(".local", "update-ui", engineName);
const results = [];
fs.mkdirSync(directory, { recursive: true });
fs.writeFileSync(
  path.join(directory, "verification.json"),
  JSON.stringify(
    {
      verification: "pending",
      engine: engineName,
      cases: [],
    },
    null,
    2,
  ) + "\n",
);

function nativeFixture(initial) {
  const local = {
    id: "update-local",
    name: "更新测试电脑",
    platform: "linux",
    enabled: true,
    online: true,
    can_host: false,
    can_files: false,
    last_seen_at: null,
  };
  const remote = {
    ...local,
    id: "update-remote",
    name: "远端测试电脑",
    platform: "windows",
    can_host: true,
  };
  const fixture = (window.updateFixture = {
    signedIn: initial.signedIn !== false,
    calls: [],
    modalShows: [],
    failDevices: false,
    loginError: "HTTP 401: invalid_password",
    nextDownloadError: "",
    pendingDownload: null,
    update: {
      currentVersion: "0.1.21",
      availableVersion: null,
      phase: "idle",
      downloaded: 0,
      total: null,
      message: "将自动检查新版，也可手动检查更新。",
      canPrompt: initial.canPrompt !== false,
      preferences: { autoCheck: true, autoInstall: !!initial.autoInstall },
      ...initial.update,
    },
  });
  const snapshot = () => ({
    ...fixture.update,
    preferences: { ...fixture.update.preferences },
  });
  const originalShowModal = HTMLDialogElement.prototype.showModal;
  HTMLDialogElement.prototype.showModal = function (...args) {
    fixture.modalShows.push(
      this.getAttribute("aria-label") || this.querySelector("h2")?.textContent,
    );
    return originalShowModal.apply(this, args);
  };
  fixture.finishDownload = (failure = "") => {
    if (!fixture.pendingDownload)
      throw Error("No synthetic download in flight");
    const pending = fixture.pendingDownload;
    fixture.pendingDownload = null;
    Object.assign(fixture.update, {
      phase: failure ? "error" : "ready",
      downloaded: failure ? fixture.update.downloaded : 1000,
      total: 1000,
      message: failure || "更新已下载并通过签名校验，可以安装。",
    });
    if (failure) pending.reject(failure);
    else pending.resolve(snapshot());
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" } },
    invoke: async (command, args = {}) => {
      fixture.calls.push({ command, args });
      if (command === "update_status") return snapshot();
      if (command === "update_check") {
        Object.assign(fixture.update, {
          availableVersion: "0.1.22",
          phase: "available",
          message: "发现新版本 0.1.22。",
        });
        return snapshot();
      }
      if (command === "update_preferences") {
        fixture.update.preferences = { ...args.preferences };
        return snapshot();
      }
      if (command === "update_download") {
        if (fixture.nextDownloadError) {
          const failure = fixture.nextDownloadError;
          fixture.nextDownloadError = "";
          Object.assign(fixture.update, { phase: "error", message: failure });
          throw failure;
        }
        Object.assign(fixture.update, {
          phase: "downloading",
          downloaded: 250,
          total: 1000,
          message: "正在下载更新。",
        });
        return new Promise((resolve, reject) => {
          fixture.pendingDownload = { resolve, reject };
        });
      }
      if (command === "update_install") {
        Object.assign(fixture.update, {
          phase: fixture.update.canPrompt ? "installing" : "waiting",
          message: fixture.update.canPrompt
            ? "正在安装更新，完成后将重新启动。"
            : "更新已就绪，等待远程连接结束。请关闭远程窗口后安装。",
        });
        return snapshot();
      }
      if (command === "state") {
        return {
          signedIn: fixture.signedIn,
          server: "https://example.invalid",
          deviceId: fixture.signedIn ? local.id : null,
          computerName: local.name,
          platform: "linux",
          canShareLocalScreen: false,
          sharing: false,
          filesEnabled: false,
          remoteAccessEnabled: false,
          remoteWatch: false,
          transportRunning: true,
          sharePreferences: { sharing: false, watch: false, restore: "idle" },
        };
      }
      if (command.startsWith("plugin:window|")) return {};
      if (command === "files_status") return { enabled: false, sessions: [] };
      if (command !== "call") return {};
      if (args.op === "login") throw fixture.loginError;
      if (args.op === "resume")
        return {
          id: "update-user",
          email: "updates@example.invalid",
          role: "user",
          session_id: "update-login",
        };
      if (args.op === "devices") {
        if (fixture.failDevices) {
          fixture.failDevices = false;
          throw "HTTP 503: unavailable";
        }
        return [local, remote];
      }
      if (["remote_sessions", "pending", "sessions"].includes(args.op))
        return [];
      return {};
    },
  };
}

const updateDialog = (page) =>
  page.getByRole("dialog", { name: "软件更新", exact: true });
const notice = (page) =>
  page.locator("main > .alert").filter({
    has: page.getByRole("button", { name: "查看更新", exact: true }),
  });
async function polls(page, count = 2) {
  const before = await page.evaluate(
    () =>
      window.updateFixture.calls.filter((c) => c.command === "update_status")
        .length,
  );
  await page.waitForFunction(
    ({ before, count }) =>
      window.updateFixture.calls.filter((c) => c.command === "update_status")
        .length >=
      before + count,
    { before, count },
  );
}
async function noDialog(page) {
  assert.equal(await updateDialog(page).isVisible(), false);
}
async function showCount(page) {
  return page.evaluate(
    () =>
      window.updateFixture.modalShows.filter((title) => title === "软件更新")
        .length,
  );
}
async function available(page, version = "0.1.22") {
  await page.evaluate((version) => {
    Object.assign(window.updateFixture.update, {
      availableVersion: version,
      phase: "available",
      message: `发现新版本 ${version}。`,
    });
  }, version);
}
async function untouchedPreferences(page, autoInstall = false) {
  assert.deepEqual(
    await page.evaluate(() => window.updateFixture.update.preferences),
    { autoCheck: true, autoInstall },
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.updateFixture.calls.filter(
          (c) => c.command === "update_preferences",
        ).length,
    ),
    0,
    "Opening, dismissing, downloading or installing must not change update preferences",
  );
}
async function alignedActions(actions) {
  const buttons = await actions.locator("button").all();
  assert.equal(
    buttons.length,
    2,
    "Expected exactly two parallel update actions",
  );
  const boxes = await Promise.all(
    buttons.map((button) => button.boundingBox()),
  );
  for (const box of boxes) assert(box);
  assert(
    Math.max(...boxes.map((b) => b.height)) -
      Math.min(...boxes.map((b) => b.height)) <=
      1,
    "Action buttons must have equal heights",
  );
  assert(
    Math.abs(boxes[0].y - boxes[1].y) <= 1,
    "Update actions must align on one row",
  );
  const parent = await actions.boundingBox();
  assert(parent);
  for (let i = 0; i < boxes.length; i++) {
    assert(
      boxes[i].x >= parent.x - 1 &&
        boxes[i].x + boxes[i].width <= parent.x + parent.width + 1,
      "Every action must remain inside its container",
    );
    if (i === 0) continue;
    assert(
      boxes[i].x - boxes[i - 1].x - boxes[i - 1].width >= 8,
      "Adjacent actions need horizontal spacing",
    );
  }
}

(async () => {
  const engine = engineName === "webkit" ? webkit : chromium;
  const browser = await engine.launch({
    ...(engine === chromium
      ? { channel: process.env.UI_BROWSER || "chrome" }
      : {}),
    ...(engine === webkit && process.env.UI_WEBKIT_EXECUTABLE
      ? { executablePath: process.env.UI_WEBKIT_EXECUTABLE }
      : {}),
    headless: true,
  });
  async function scenario(
    name,
    initial,
    work,
    viewport = { width: 1280, height: 720 },
  ) {
    const page = await browser.newPage({ viewport });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript(nativeFixture, initial);
    try {
      await page.goto(process.env.UI_URL || "http://127.0.0.1:1420/");
      if (initial.signedIn === false)
        await page
          .getByRole("heading", { name: "登录 FarSail", exact: true })
          .waitFor();
      else {
        await page
          .getByRole("heading", { name: "总览", exact: true })
          .waitFor();
        await page.waitForFunction(() =>
          window.updateFixture.calls.some(
            (call) => call.command === "call" && call.args.op === "pending",
          ),
        );
      }
      await work(page);
      assert.deepEqual(errors, []);
      results.push({ name, verification: "passed", viewport });
      console.log(`PASS ${name}`);
    } catch (error) {
      await page
        .screenshot({ path: path.join(directory, `${name}-failed.png`) })
        .catch(() => {});
      throw error;
    } finally {
      await page.close();
    }
  }
  try {
    for (const viewport of [
      { width: 1280, height: 720 },
      { width: 1080, height: 700 },
    ]) {
      await scenario(
        `layout-${viewport.width}x${viewport.height}`,
        {},
        async (page) => {
          await page
            .getByRole("button", { name: "共享与设置", exact: true })
            .click();
          const settings = page.locator(".client-updates");
          await settings
            .getByRole("heading", { name: "软件更新", exact: true })
            .waitFor();
          await available(page);
          await updateDialog(page).waitFor();
          await alignedActions(
            updateDialog(page).locator(".update-dialog-actions"),
          );
          const box = await updateDialog(page).boundingBox();
          assert(
            box &&
              box.x >= 0 &&
              box.y >= 0 &&
              box.x + box.width <= viewport.width &&
              box.y + box.height <= viewport.height,
            "Update dialog must fit the viewport",
          );
          await page.screenshot({
            path: path.join(
              directory,
              `dialog-${viewport.width}x${viewport.height}.png`,
            ),
          });
          await updateDialog(page)
            .getByRole("button", { name: "稍后", exact: true })
            .click();
          await alignedActions(settings.locator(".actions"));
          assert(
            await settings
              .getByRole("button", { name: "检查更新", exact: true })
              .isEnabled(),
          );
          assert(
            await settings
              .getByRole("button", { name: "查看更新", exact: true })
              .isEnabled(),
          );
          assert(
            await settings
              .getByRole("button", { name: "查看更新", exact: true })
              .evaluate((button) => button.classList.contains("primary")),
          );
          const alert = notice(page);
          await alert.waitFor();
          const alertButton = alert.getByRole("button", {
            name: "查看更新",
            exact: true,
          });
          const geometry = await alertButton.evaluate((button) => {
            const range = document.createRange();
            range.selectNodeContents(button);
            const rects = [...range.getClientRects()];
            return {
              noWrap: getComputedStyle(button).whiteSpace,
              ys: rects.map((r) => r.y),
              clipped: button.scrollWidth > button.clientWidth + 1,
              rect: button.getBoundingClientRect().toJSON(),
              parent: button.parentElement.getBoundingClientRect().toJSON(),
            };
          });
          assert.equal(geometry.noWrap, "nowrap");
          assert(
            Math.max(...geometry.ys) - Math.min(...geometry.ys) <= 1,
            "Update notice action must stay on one line",
          );
          assert.equal(geometry.clipped, false);
          assert(
            geometry.rect.right <= geometry.parent.right + 1 &&
              geometry.rect.left >= geometry.parent.left - 1,
          );
          await page.screenshot({
            path: path.join(
              directory,
              `settings-${viewport.width}x${viewport.height}.png`,
            ),
          });
          await settings
            .getByRole("button", { name: "查看更新", exact: true })
            .click();
          await updateDialog(page).waitFor();
          assert(
            await page
              .getByRole("heading", { name: "共享与设置", exact: true })
              .isVisible(),
            "Settings opens the update dialog without changing the page",
          );
          await updateDialog(page)
            .getByRole("button", { name: "关闭更新窗口", exact: true })
            .click();
          await untouchedPreferences(page);
        },
        viewport,
      );
    }
    await scenario("automatic-once-and-manual-reopen", {}, async (page) => {
      await available(page);
      await updateDialog(page).waitFor();
      assert.equal(await showCount(page), 1);
      await updateDialog(page)
        .getByRole("button", { name: "稍后", exact: true })
        .click();
      await polls(page);
      await noDialog(page);
      assert.equal(
        await showCount(page),
        1,
        "A dismissed version must not reopen on status polling",
      );
      const noticeAction = notice(page).getByRole("button", {
        name: "查看更新",
        exact: true,
      });
      await noticeAction.click();
      await updateDialog(page).waitFor();
      assert.equal(
        await page
          .getByRole("heading", { name: "总览", exact: true })
          .isVisible(),
        true,
        "Notice action must keep the current page",
      );
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        () => !document.querySelector("dialog.update-dialog")?.open,
      );
      assert(
        await noticeAction.evaluate(
          (button) => document.activeElement === button,
        ),
        "Escape must restore focus to the notice action that opened the dialog",
      );
      await polls(page);
      await noDialog(page);
      await available(page, "0.1.23");
      await updateDialog(page).waitFor();
      assert.equal(
        await showCount(page),
        3,
        "A newer version may automatically remind again",
      );
      await available(page, "0.1.24");
      await updateDialog(page)
        .getByText("FarSail 0.1.24 已可更新。", { exact: true })
        .waitFor();
      await updateDialog(page)
        .getByRole("button", { name: "稍后", exact: true })
        .click();
      await polls(page);
      await noDialog(page);
      assert.equal(
        await showCount(page),
        3,
        "Dismissal must acknowledge the version currently displayed, even if it changed while open",
      );
      await untouchedPreferences(page);
    });
    await scenario("download-progress-and-phase-deferral", {}, async (page) => {
      await available(page);
      const dialog = updateDialog(page);
      await dialog.waitFor();
      await dialog
        .getByRole("button", { name: "立即更新", exact: true })
        .click();
      await page.waitForFunction(() => !!window.updateFixture.pendingDownload);
      await dialog
        .getByRole("button", { name: "正在下载…", exact: true })
        .waitFor();
      assert(
        await dialog
          .getByRole("button", { name: "正在下载…", exact: true })
          .isDisabled(),
      );
      await page.evaluate(() => (window.updateFixture.update.downloaded = 500));
      const progress = dialog.locator(".update-progress progress");
      await page.waitForFunction(
        () => document.querySelector("dialog[open] progress")?.value === 500,
      );
      assert.equal(await progress.evaluate((p) => p.max), 1000);
      await dialog.getByText("50%", { exact: true }).waitFor();
      await page.screenshot({
        path: path.join(directory, "download-progress.png"),
      });
      await page.evaluate(() =>
        Object.assign(window.updateFixture.update, {
          total: null,
          downloaded: 1536,
        }),
      );
      await dialog.getByText("2 KB", { exact: true }).waitFor();
      assert.equal(await progress.getAttribute("value"), null);
      assert(
        await progress.evaluate((p) => p.matches(":indeterminate")),
        "Unknown download total must show indeterminate progress rather than a full bar",
      );
      await page.screenshot({
        path: path.join(directory, "unknown-progress.png"),
      });
      await page.evaluate(() =>
        Object.assign(window.updateFixture.update, {
          total: 1000,
          downloaded: 500,
        }),
      );
      await dialog.getByRole("button", { name: "稍后", exact: true }).click();
      await polls(page);
      await noDialog(page);
      await page.evaluate(() => window.updateFixture.finishDownload());
      await polls(page);
      await noDialog(page);
      await notice(page)
        .getByRole("button", { name: "查看更新", exact: true })
        .click();
      await dialog
        .getByRole("button", { name: "安装并重启", exact: true })
        .waitFor();
      await page.evaluate(
        () => (window.updateFixture.update.canPrompt = false),
      );
      await dialog
        .getByRole("button", { name: "安装并重启", exact: true })
        .click();
      await dialog.getByText(/等待远程连接结束/).waitFor();
      await dialog.getByRole("button", { name: "稍后", exact: true }).click();
      await polls(page);
      await noDialog(page);
      await page.evaluate(() =>
        Object.assign(window.updateFixture.update, {
          phase: "installing",
          message: "正在安装更新，完成后将重新启动。",
        }),
      );
      await polls(page);
      await noDialog(page);
      await page
        .getByRole("button", { name: "共享与设置", exact: true })
        .click();
      await page
        .locator(".client-updates")
        .getByRole("button", { name: "查看更新", exact: true })
        .click();
      await dialog
        .getByRole("button", { name: "正在安装…", exact: true })
        .waitFor();
      assert(
        await dialog
          .getByRole("button", { name: "正在安装…", exact: true })
          .isDisabled(),
      );
      assert.equal(
        await dialog.getByRole("button", { name: "稍后", exact: true }).count(),
        0,
      );
      await dialog
        .getByRole("button", { name: "关闭窗口", exact: true })
        .click();
      assert.equal(
        await page.evaluate(() => window.updateFixture.update.phase),
        "installing",
        "Closing the progress window must not imply cancelling installation",
      );
      await untouchedPreferences(page);
    });
    await scenario("internal-error-is-not-user-copy", {}, async (page) => {
      await available(page);
      const dialog = updateDialog(page);
      await dialog.waitFor();
      await page.evaluate(
        () =>
          (window.updateFixture.nextDownloadError =
            "reqwest::Error SendRequest https://internal.invalid/private /tmp/updater.rs:123"),
      );
      await dialog
        .getByRole("button", { name: "立即更新", exact: true })
        .click();
      await dialog.getByText("更新未完成，请重试。", { exact: true }).waitFor();
      assert(
        !/reqwest|internal\.invalid|updater\.rs/.test(
          await page.locator("body").innerText(),
        ),
      );
      assert.equal(await dialog.locator(".hint").count(), 0);
      await dialog.getByRole("button", { name: "稍后", exact: true }).click();
      await page
        .getByRole("button", { name: "共享与设置", exact: true })
        .click();
      const settings = page.locator(".client-updates");
      await settings
        .getByText("更新未完成，请重试。", { exact: true })
        .waitFor();
      assert(
        !/reqwest|internal\.invalid|updater\.rs/.test(
          await settings.innerText(),
        ),
      );
    });
    await scenario("download-failure-retry", {}, async (page) => {
      await available(page);
      const dialog = updateDialog(page);
      await dialog.waitFor();
      await page.evaluate(
        () =>
          (window.updateFixture.nextDownloadError =
            "更新包校验失败，请重新下载。"),
      );
      await dialog
        .getByRole("button", { name: "立即更新", exact: true })
        .click();
      await dialog
        .getByText("更新包校验失败，请重新下载。", { exact: true })
        .waitFor();
      assert.equal(
        await dialog
          .getByRole("button", { name: "安装并重启", exact: true })
          .count(),
        0,
      );
      await dialog
        .getByRole("button", { name: "重试更新", exact: true })
        .click();
      await page.waitForFunction(() => !!window.updateFixture.pendingDownload);
      await page.evaluate(() => window.updateFixture.finishDownload());
      await dialog
        .getByRole("button", { name: "安装并重启", exact: true })
        .waitFor();
      assert.equal(
        await page.evaluate(
          () =>
            window.updateFixture.calls.filter(
              (c) => c.command === "update_download",
            ).length,
        ),
        2,
      );
      await untouchedPreferences(page);
    });
    await scenario(
      "initial-background-failure-reminder",
      {
        update: {
          availableVersion: "0.1.22",
          phase: "error",
          message: "更新包校验失败，请重新下载。",
        },
      },
      async (page) => {
        const dialog = updateDialog(page);
        await dialog.waitFor();
        assert.equal(await showCount(page), 1);
        await dialog
          .getByText("更新包校验失败，请重新下载。", { exact: true })
          .waitFor();
        await dialog
          .getByRole("button", { name: "重试更新", exact: true })
          .click();
        await page.waitForFunction(
          () => !!window.updateFixture.pendingDownload,
        );
        await page.evaluate(() => window.updateFixture.finishDownload());
        await dialog
          .getByRole("button", { name: "安装并重启", exact: true })
          .waitFor();
        await dialog.getByRole("button", { name: "稍后", exact: true }).click();
        await polls(page);
        await noDialog(page);
        await untouchedPreferences(page);
      },
    );
    await scenario(
      "existing-auto-install-preference",
      { autoInstall: true },
      async (page) => {
        await available(page);
        await updateDialog(page).waitFor();
        await updateDialog(page)
          .getByRole("button", { name: "后台更新", exact: true })
          .click();
        await polls(page);
        await untouchedPreferences(page, true);
        assert.equal(
          await page.evaluate(
            () =>
              window.updateFixture.calls.filter((c) =>
                ["update_download", "update_install"].includes(c.command),
              ).length,
          ),
          0,
          "Displaying a reminder must not itself download or install",
        );
      },
    );
    for (const activity of [
      "remote-session",
      "file-session",
      "remote-viewer",
    ]) {
      await scenario(
        `defer-${activity}`,
        { canPrompt: false },
        async (page) => {
          await available(page);
          await polls(page);
          await noDialog(page);
          assert.equal(await showCount(page), 0);
          await page.evaluate(
            () => (window.updateFixture.update.canPrompt = true),
          );
          await updateDialog(page).waitFor();
          assert.equal(await showCount(page), 1);
        },
      );
    }
    await scenario("defer-error-modal", {}, async (page) => {
      await page.evaluate(() => (window.updateFixture.failDevices = true));
      await page.getByRole("button", { name: "刷新状态", exact: true }).click();
      const failure = page.getByRole("alertdialog");
      await failure.waitFor();
      await available(page);
      await polls(page);
      await noDialog(page);
      assert.equal(await showCount(page), 0);
      await failure
        .getByRole("button", { name: "关闭错误", exact: true })
        .click();
      await updateDialog(page).waitFor();
      assert.equal(await showCount(page), 1);
    });
    await scenario("defer-login-modal", { signedIn: false }, async (page) => {
      await page
        .getByLabel("邮箱地址", { exact: true })
        .fill("updates@example.invalid");
      await page
        .getByLabel("密码", { exact: true })
        .fill("synthetic-password-123");
      await page.getByRole("button", { name: "登录", exact: true }).click();
      const login = page.getByRole("dialog", { name: "密码错误", exact: true });
      await login.waitFor();
      await available(page);
      await polls(page);
      await noDialog(page);
      assert.equal(await showCount(page), 0);
      await login
        .getByRole("button", { name: "返回登录", exact: true })
        .click();
      await updateDialog(page).waitFor();
      assert.equal(await showCount(page), 1);
    });
    fs.writeFileSync(
      path.join(directory, "verification.json"),
      JSON.stringify(
        {
          verification: "passed",
          scope:
            "Synthetic native IPC in real browser UI; no updater downloads or installer execution",
          engine: engineName,
          browser:
            engineName === "webkit"
              ? "Playwright WebKit"
              : process.env.UI_BROWSER || "chrome",
          cases: results,
          not_tested: [
            "Physical desktop modal behavior",
            "Native detection of active remote/viewer/file sessions (canPrompt is simulated here)",
            "Real cross-version native update download/install/restart",
          ],
        },
        null,
        2,
      ) + "\n",
    );
    console.log(
      `PASS ${results.length} update UI cases (${engineName}); synthetic IPC only.`,
    );
  } finally {
    await browser.close();
  }
})().catch((error) => {
  fs.writeFileSync(
    path.join(directory, "verification.json"),
    JSON.stringify(
      {
        verification: "failed",
        engine: engineName,
        cases: results,
        error: error.message,
      },
      null,
      2,
    ) + "\n",
  );
  console.error(error);
  process.exitCode = 1;
});
