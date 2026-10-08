// Pure Node regressions. No browser, DOM, WebView or OS input is started.
import assert from "node:assert/strict";
import test from "node:test";
import {
  RefreshFailureGate,
  DialogKeyboardGate,
} from "../apps/desktop/src/feedback-policy.ts";

test("a dismissed sustained background failure does not repeat; explicit retry may report it", () => {
  const gate = new RefreshFailureGate();
  assert(gate.shouldReport("network unavailable", true));
  for (let n = 0; n < 30; n++)
    assert(!gate.shouldReport("network unavailable", false));
  assert(gate.shouldReport("network unavailable", true));
  assert(!gate.shouldReport("network unavailable", false));
  gate.recovered();
  assert(gate.shouldReport("network unavailable", false));
  assert(gate.shouldReport("authorization expired", false));
  assert(!gate.shouldReport("authorization expired", false));
});
test("dialog dismissal cannot forward held Enter repeats or its release to the remote screen", () => {
  const gate = new DialogKeyboardGate();
  gate.press("Enter");
  assert(!gate.suppress("Enter", "down", true));
  for (let n = 0; n < 10; n++) assert(gate.suppress("Enter", "down", false));
  assert(gate.suppress("Enter", "up", false));
  assert(!gate.suppress("Enter", "down", false));
  assert(!gate.suppress("Enter", "up", false));
});
test("Space activation within the dialog and unrelated fresh remote keys remain available", () => {
  const gate = new DialogKeyboardGate();
  gate.press("Space");
  assert(!gate.suppress("Space", "up", true));
  assert(!gate.suppress("KeyA", "down", false));
  gate.press("Escape");
  assert(gate.suppress("Escape", "up", false));
  assert(!gate.suppress("Escape", "down", false));
  gate.press("ControlLeft");
  gate.reset();
  assert(!gate.suppress("ControlLeft", "down", false));
});

test("an inherited Enter repeat cannot activate a newly opened dialog", () => {
  const gate = new DialogKeyboardGate();
  for (let n = 0; n < 10; n++) {
    gate.press("Enter", true);
    assert(gate.suppress("Enter", "down", true));
  }
  assert(gate.suppress("Enter", "up", true));
  gate.press("Enter");
  assert(!gate.suppress("Enter", "down", true));
  gate.press("Enter", true);
  assert(!gate.suppress("Enter", "down", true));
  assert(!gate.suppress("Enter", "up", true));
});
