// Pure state tests; no browser, DOM, application or OS input is started.
import assert from "node:assert/strict";
import test from "node:test";
import {
  ViewerDisplayGate,
  matchRetainedDisplay,
  recoveryStageText,
  dispatchViewerInput,
} from "../apps/desktop/src/viewer-display-policy.ts";
const primary = {
  id: 1,
  name: "Panel",
  x: 0,
  y: 0,
  width: 1920,
  height: 1080,
  dpi: 96,
  rotation: 0,
};
const secondary = { ...primary, id: 2, x: 1920 };

test("recovery suppresses the default screen and input through list, request and target frame", () => {
  const gate = new ViewerDisplayGate(secondary, true);
  assert(!gate.canControl());
  assert(!gate.acceptFrame(1));
  assert(!gate.acceptFrame(2));
  assert.equal(gate.resolve([]), null);
  const ticket = gate.resolve([primary, secondary]);
  assert.equal(ticket.display, 2);
  assert(
    !gate.acceptFrame(2),
    "selection must be sent before accepting a frame",
  );
  assert(gate.requested(ticket));
  assert(!gate.acceptFrame(1));
  assert(!gate.canControl());
  assert(gate.acceptFrame(2));
  assert(gate.canControl());
  assert.equal(gate.intent().id, 2);
});
test("a unique descriptor is rebound after enumeration changes without treating DPI as identity", () => {
  const movedId = { ...secondary, id: 7, dpi: 144 };
  const gate = new ViewerDisplayGate(secondary, true);
  const ticket = gate.resolve([primary, movedId]);
  assert.equal(ticket.display, 7);
  gate.requested(ticket);
  assert(!gate.acceptFrame(2));
  assert(gate.acceptFrame(7));
});
test("same name or same ordinal cannot silently restore a changed or missing screen", () => {
  for (const list of [
    [primary],
    [{ ...secondary, x: -1920 }],
    [{ ...primary, id: 2 }],
    [secondary, { ...secondary, id: 3 }],
  ]) {
    const gate = new ViewerDisplayGate(secondary, true);
    assert.equal(gate.resolve(list), null);
    assert.equal(gate.snapshot().stage, "unavailable");
    assert(!gate.canControl());
    for (const screen of list) assert(!gate.acceptFrame(screen.id));
    assert.equal(gate.intent().x, 1920);
  }
  assert.equal(
    matchRetainedDisplay(secondary, [primary, secondary]).kind,
    "matched",
  );
});
test("explicit selection recovers an unavailable screen without replaying stale selection callbacks", () => {
  const gate = new ViewerDisplayGate(secondary, true);
  gate.resolve([primary]);
  const old = gate.choose(secondary);
  assert(gate.fail(old, "request"));
  const current = gate.choose(primary);
  assert(!gate.requested(old));
  assert(!gate.fail(old, "request"));
  assert(gate.requested(current));
  assert(!gate.acceptFrame(2));
  assert(gate.acceptFrame(1));
  assert(
    !gate.fail(current, "timeout"),
    "late timeout cannot hide a recovered frame",
  );
});
test("screen choices cannot overlap a request or an unconfirmed target frame", () => {
  const gate = new ViewerDisplayGate(secondary, true);
  const pending = gate.resolve([primary, secondary]);
  assert.equal(gate.choose(primary), null);
  assert(gate.requested(pending));
  assert.equal(gate.choose(primary), null);
  assert(!gate.acceptFrame(1));
  assert(gate.fail(pending, "timeout"));
  assert(
    gate.choose(secondary),
    "a timed-out target can be explicitly retried",
  );
});
test("late input rejection cannot cancel recovery or a subsequent display selection", async () => {
  let epoch = 1;
  let closeCalls = 0;
  let reject;
  const queued = dispatchViewerInput(
    () =>
      new Promise((_, failed) => {
        reject = failed;
      }),
    () => epoch === 1,
    () => closeCalls++,
  );
  epoch = 2;
  reject(new Error("old transport closed"));
  await queued;
  assert.equal(closeCalls, 0);
  await dispatchViewerInput(
    () => Promise.reject(new Error("current error")),
    () => true,
    () => closeCalls++,
  );
  assert.equal(
    closeCalls,
    1,
    "current failures still stop the affected session",
  );
});
test("unmounted input leases suppress queued dispatch and delayed failure delivery", async () => {
  let active = true;
  let calls = 0;
  let reject;
  const pending = dispatchViewerInput(
    () =>
      new Promise((_, failed) => {
        calls++;
        reject = failed;
      }),
    () => active,
    () => assert.fail("stopped component cannot close the successor"),
  );
  active = false;
  reject(new Error("late IPC error"));
  await pending;
  await dispatchViewerInput(
    () => {
      calls++;
      return Promise.resolve();
    },
    () => active,
    () => assert.fail("inactive"),
  );
  assert.equal(calls, 1);
});
test("failure is not reissued by polling and stop rejects every late frame or callback", () => {
  const gate = new ViewerDisplayGate(secondary, true);
  const ticket = gate.resolve([primary, secondary]);
  assert(gate.requested(ticket));
  assert(gate.fail(ticket, "timeout"));
  assert.equal(gate.resolve([primary, secondary]), null);
  assert(!gate.acceptFrame(2));
  const retry = gate.choose(secondary);
  gate.stop();
  assert(!gate.requested(retry));
  assert(!gate.fail(retry, "request"));
  assert(!gate.acceptFrame(2));
  assert.equal(gate.choose(primary), null);
});
test("window intent survives session remount and wrong default frames cannot overwrite it", () => {
  const old = new ViewerDisplayGate(null, false);
  assert(old.acceptFrame(1));
  assert(old.observeInitial(1, [primary, secondary]));
  const ticket = old.choose(secondary);
  old.requested(ticket);
  old.acceptFrame(2);
  const saved = old.intent();
  old.stop();
  const next = new ViewerDisplayGate(saved, true);
  assert(!next.acceptFrame(1));
  assert.deepEqual(next.intent(), secondary);
});
test("real reconnect phases retain attempt counts and approval is not reported as connected", () => {
  const status = {
    cycle: 1,
    phase: "backoff",
    attempt: 2,
    maxAttempts: 3,
    retryInMs: 1600,
    manualRetryAllowed: false,
  };
  assert.match(recoveryStageText(status), /第 2\/3 次.*2 秒/);
  assert.match(
    recoveryStageText({
      ...status,
      phase: "awaiting_approval",
      retryInMs: null,
    }),
    /等待对方批准/,
  );
  assert.match(
    recoveryStageText({ ...status, phase: "connecting", retryInMs: null }),
    /正在建立连接/,
  );
  assert.match(recoveryStageText({ ...status, phase: "cancelled" }), /已取消/);
});
