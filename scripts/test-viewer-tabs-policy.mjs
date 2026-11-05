// No DOM, browser, native commands, network or OS input is started.
import assert from "node:assert/strict";
import test from "node:test";
import { ViewerTabsController } from "../apps/desktop/src/viewer-tabs-policy.ts";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const row = (instance, id = instance) => ({ instance, id, selected: true });

function setup(read) {
  const snapshots = [],
    errors = [];
  const controller = new ViewerTabsController({
    read,
    publish: (tabs) => snapshots.push(tabs),
    failed: (error) => errors.push(error),
  });
  return { controller, snapshots, errors };
}

test("an old poll cannot undo a selection, even if the mutation finishes before the poll", async () => {
  const stale = deferred();
  let native = [row("A")],
    reads = 0,
    inFlight = 0,
    maximum = 0;
  const { controller, snapshots, errors } = setup(async () => {
    inFlight++;
    maximum = Math.max(maximum, inFlight);
    try {
      return ++reads === 1 ? await stale.promise : structuredClone(native);
    } finally {
      inFlight--;
    }
  });
  controller.start();
  await controller.mutate(async () => {
    native = [row("B")];
  });
  assert.equal(reads, 1, "post-mutation refresh waits for the existing read");
  stale.resolve([row("A")]);
  await tick();
  assert.deepEqual(
    snapshots,
    [[row("B")]],
    "the stale snapshot is never published",
  );
  assert.equal(
    reads,
    2,
    "the required fresh read was not lost behind the stale read",
  );
  assert.equal(maximum, 1);
  assert.deepEqual(errors, []);
  controller.stop();
});

test("rapid selections and close execute in user order and publish the final native selection", async () => {
  const held = deferred(),
    events = [];
  let native = [row("A")],
    inFlight = 0,
    maximum = 0;
  const { controller, snapshots } = setup(async () => structuredClone(native));
  controller.start();
  await tick();
  const action =
    (name, hold = false) =>
    async () => {
      inFlight++;
      maximum = Math.max(maximum, inFlight);
      events.push(name);
      try {
        if (hold) await held.promise;
        native = [row(name)];
      } finally {
        inFlight--;
      }
    };
  const b = controller.mutate(action("B", true));
  const close = controller.mutate(action("close-C"));
  const e = controller.mutate(action("E"));
  controller.refresh();
  controller.refresh();
  await tick();
  assert.deepEqual(events, ["B"]);
  assert.deepEqual(
    snapshots,
    [[row("A")]],
    "pending mutations suppress intermediate snapshots",
  );
  held.resolve();
  await Promise.all([b, close, e]);
  await tick();
  assert.deepEqual(events, ["B", "close-C", "E"]);
  assert.deepEqual(snapshots, [[row("A")], [row("E")]]);
  assert.equal(maximum, 1);
  controller.stop();
});

test("the latest failed selection reports an error and restores native truth", async () => {
  let displayed = [row("A")];
  const failure = new Error("tab closed before selection");
  const errors = [];
  const controller = new ViewerTabsController({
    read: async () => [row("A")],
    publish: (tabs) => {
      displayed = tabs;
    },
    failed: (error) => errors.push(error),
  });
  controller.start();
  await tick();
  displayed = [row("B")]; // The caller's immediate optimistic selection.
  await controller.mutate(async () => {
    throw failure;
  });
  await tick();
  assert.deepEqual(errors, [failure]);
  assert.deepEqual(displayed, [row("A")]);
  controller.stop();
});

test("an earlier failed selection does not replace a newer intent's result with an obsolete error", async () => {
  const first = deferred();
  let native = [row("A")];
  const { controller, snapshots, errors } = setup(async () =>
    structuredClone(native),
  );
  controller.start();
  await tick();
  const b = controller.mutate(async () => {
    await first.promise;
    throw new Error("old B failed");
  });
  const e = controller.mutate(async () => {
    native = [row("E")];
  });
  first.resolve();
  await Promise.all([b, e]);
  await tick();
  assert.deepEqual(errors, []);
  assert.deepEqual(snapshots.at(-1), [row("E")]);
  controller.stop();
});

test("repeated timer refreshes coalesce into single-flight snapshots", async () => {
  const held = deferred();
  let reads = 0,
    inFlight = 0,
    maximum = 0;
  const { controller, snapshots } = setup(async () => {
    inFlight++;
    maximum = Math.max(maximum, inFlight);
    try {
      return ++reads === 1 ? await held.promise : [row("B")];
    } finally {
      inFlight--;
    }
  });
  controller.start();
  for (let i = 0; i < 10; i++) controller.refresh();
  assert.equal(reads, 1);
  held.resolve([row("A")]);
  await tick();
  assert.equal(reads, 2);
  assert.equal(maximum, 1);
  assert.deepEqual(snapshots, [[row("A")], [row("B")]]);
  controller.stop();
});

test("effect cleanup and restart discard the old lifetime without overlapping reads", async () => {
  const old = deferred();
  let reads = 0,
    inFlight = 0,
    maximum = 0;
  const { controller, snapshots, errors } = setup(async () => {
    inFlight++;
    maximum = Math.max(maximum, inFlight);
    try {
      return ++reads === 1 ? await old.promise : [row("fresh")];
    } finally {
      inFlight--;
    }
  });
  controller.start();
  controller.stop();
  controller.start();
  assert.equal(reads, 1);
  old.reject(new Error("disposed read failed"));
  await tick();
  assert.equal(reads, 2);
  assert.equal(maximum, 1);
  assert.deepEqual(snapshots, [[row("fresh")]]);
  assert.deepEqual(errors, []);
  controller.stop();
});

test("unmount skips queued commands and suppresses late results while preserving later recovery reads", async () => {
  const held = deferred(),
    commands = [];
  let native = [row("A")];
  const { controller, snapshots, errors } = setup(async () =>
    structuredClone(native),
  );
  controller.start();
  await tick();
  const first = controller.mutate(async () => {
    commands.push("B");
    await held.promise;
    native = [row("B")];
  });
  const second = controller.mutate(async () => {
    commands.push("close-C");
  });
  await tick();
  controller.stop();
  held.resolve();
  await Promise.all([first, second]);
  await tick();
  assert.deepEqual(commands, ["B"]);
  assert.deepEqual(snapshots, [[row("A")]]);
  assert.deepEqual(errors, []);
  controller.start();
  await tick();
  assert.deepEqual(snapshots.at(-1), [row("B")]);
  controller.stop();
});

test("after mutations settle, native new tabs and recovered session IDs are accepted", async () => {
  let native = [row("A")];
  const { controller, snapshots, errors } = setup(async () =>
    structuredClone(native),
  );
  controller.start();
  await tick();
  await controller.mutate(async () => {
    native = [row("B")];
  });
  await tick();
  native = [
    { instance: "B", id: "B-recovered", selected: false },
    row("new-C"),
  ];
  controller.refresh();
  await tick();
  assert.deepEqual(
    snapshots.at(-1),
    native,
    "the last user target is not pinned forever",
  );
  assert.deepEqual(errors, []);
  controller.stop();
});

test("a failed close remains actionable even when followed by a newer selection", async () => {
  const closeFailure = new Error("could not close C");
  let native = [row("A")];
  const { controller, snapshots, errors } = setup(async () =>
    structuredClone(native),
  );
  controller.start();
  await tick();
  const close = controller.mutate(async () => {
    throw closeFailure;
  }, "always");
  const select = controller.mutate(async () => {
    native = [row("E")];
  });
  await Promise.all([close, select]);
  await tick();
  assert.deepEqual(errors, [closeFailure]);
  assert.deepEqual(snapshots.at(-1), [row("E")]);
  controller.stop();
});
