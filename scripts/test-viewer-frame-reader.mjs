import assert from "node:assert/strict";
import test from "node:test";
import { ViewerFrameReader } from "../apps/desktop/src/viewer-frame-reader.ts";
globalThis.window = globalThis;

function fixture(call) {
  let handler;
  const released = [];
  const bridge = {
    addEventListener(name, callback) {
      assert.equal(name, "sharedbufferreceived");
      handler = callback;
    },
    releaseBuffer(buffer) {
      released.push(buffer);
    },
  };
  return {
    reader: () => new ViewerFrameReader(call, bridge),
    released,
    emit(id, request, bytes = [1, 2, 3], kind = "farsail-frame") {
      const buffer = Uint8Array.from(bytes).buffer;
      handler({
        additionalData: { kind, id, request },
        getBuffer: () => buffer,
      });
      return buffer;
    },
  };
}

test("shared frames stay scoped to the session and request across two viewers", async () => {
  const requests = [];
  const f = fixture(async (command, args) => {
    assert.equal(command, "media_next_shared");
    requests.push(args);
    return "shared";
  });
  const a = f.reader(),
    b = f.reader();
  const pa = a.next("phone-a", 0),
    pb = b.next("phone-b", 0);
  f.emit("phone-b", requests[0].request, [9]); // Wrong session: release, never deliver.
  f.emit("phone-b", requests[1].request, [2]);
  f.emit("phone-a", requests[0].request, [1]);
  const [fa, fb] = await Promise.all([pa, pb]);
  assert.deepEqual([...fa.bytes], [1]);
  assert.deepEqual([...fb.bytes], [2]);
  assert.equal(f.released.length, 1);
  fa.release();
  fa.release();
  b.cancel();
  assert.equal(
    f.released.length,
    3,
    "each owned buffer is released exactly once",
  );
});

test("late delivery after cancellation is released without reviving a viewer", async () => {
  let args, ack;
  const f = fixture(async (_, request) => {
    args = request;
    return new Promise((resolve) => {
      ack = resolve;
    });
  });
  const reader = f.reader(),
    result = reader.next("phone", 0);
  reader.cancel();
  f.emit("phone", args.request);
  ack("shared");
  assert.equal((await result).bytes.length, 0);
  assert.equal(f.released.length, 1);
});

test("unsupported runtimes switch once to binary IPC without losing the cursor", async () => {
  const calls = [];
  const f = fixture(async (command, args) => {
    calls.push({ command, ...args });
    return command === "media_next_shared"
      ? "unsupported"
      : new Uint8Array([7]).buffer;
  });
  const reader = f.reader();
  assert.deepEqual([...(await reader.next("phone", 42)).bytes], [7]);
  await reader.next("phone", 43);
  assert.deepEqual(
    calls.map((x) => x.command),
    ["media_next_shared", "media_next", "media_next"],
  );
  assert.equal(calls[1].after, 42);
});

test("an empty shared read does not start a second request", async () => {
  let calls = 0;
  const f = fixture(async () => {
    calls++;
    return "empty";
  });
  assert.equal((await f.reader().next("phone", 10)).bytes.length, 0);
  assert.equal(calls, 1);
});

test("authorization errors propagate and late posted data is discarded", async () => {
  let request;
  const f = fixture(async (_, args) => {
    request = args.request;
    throw new Error("denied");
  });
  await assert.rejects(f.reader().next("phone", 0), /denied/);
  f.emit("phone", request);
  assert.equal(f.released.length, 1);
});

test("a missing native reply releases received memory and falls back within a bound", async () => {
  let request;
  const f = fixture(async (command, args) => {
    if (command === "media_next") {
      return new Uint8Array([8]).buffer;
    }
    request = args.request;
    return new Promise(() => {});
  });
  const reader = f.reader(),
    result = reader.next("phone", 5);
  f.emit("phone", request);
  assert.deepEqual([...(await result).bytes], [8]);
  assert.equal(f.released.length, 1);
});
