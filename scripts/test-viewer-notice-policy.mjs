// No browser, DOM, application, network or OS input is started.
import assert from "node:assert/strict";
import test from "node:test";
import {
  ViewerNoticeLease,
  inspectViewerNotice,
  publishViewerNotice,
} from "../apps/desktop/src/viewer-notice-policy.ts";

test("closing before a success callback prevents publishing a viewer-open notice", async () => {
  const lease = new ViewerNoticeLease("1");
  let active = true;
  assert(await lease.check(async () => active));
  active = false;
  assert(!(await lease.check(async () => active)));
  assert(!lease.isPresent(), "both banner and device detail share this owner");
  let published = lease;
  await publishViewerNotice(
    lease,
    async () => false,
    () => true,
    (value) => {
      published = value;
    },
  );
  assert.equal(published, null);
});
test("an older viewer-open completion cannot erase a newer operation message", async () => {
  const lease = new ViewerNoticeLease("9");
  let version = 1;
  let deliver;
  let message = "opening";
  const pending = publishViewerNotice(
    lease,
    () =>
      new Promise((resolve) => {
        deliver = resolve;
      }),
    () => version === 1,
    (value) => {
      message = value ? "viewer opened" : "";
    },
  );
  version = 2;
  message = "settings saved";
  deliver(false);
  await pending;
  assert.equal(message, "settings saved");
});
test("a late earlier open-status response cannot revive an observed closed window", async () => {
  const lease = new ViewerNoticeLease("2");
  let deliver;
  const earlier = lease.check(
    () =>
      new Promise((resolve) => {
        deliver = resolve;
      }),
  );
  assert(!(await lease.check(async () => false)));
  deliver(true);
  assert(!(await earlier));
  assert(!lease.isPresent());
});
test("late close checks do not clear a replacement window or an unrelated operation notice", async () => {
  for (const replacement of [new ViewerNoticeLease("4"), "settings saved"]) {
    const old = new ViewerNoticeLease("3");
    let owner = old;
    let deliver;
    let closedCalls = 0;
    const pending = inspectViewerNotice(
      old,
      () =>
        new Promise((resolve) => {
          deliver = resolve;
        }),
      () => owner === old,
      () => {
        closedCalls++;
      },
    );
    owner = replacement;
    deliver(false);
    await pending;
    assert.equal(closedCalls, 0);
    assert.equal(owner, replacement);
  }
});
test("window owners are independent and repeated observation only closes the matching notice", async () => {
  const a = new ViewerNoticeLease("5");
  const b = new ViewerNoticeLease("6");
  let closed = null;
  await inspectViewerNotice(
    a,
    async (token) => token !== "5",
    () => true,
    (lease) => {
      closed = lease;
    },
  );
  assert.equal(closed, a);
  assert(!a.isPresent());
  assert(b.isPresent());
  let reads = 0;
  assert(
    !(await a.check(async () => {
      reads++;
      return true;
    })),
  );
  assert.equal(reads, 0, "closed leases stop reading native state");
});
test("failed status queries preserve the notice until a confirmed closure", async () => {
  const lease = new ViewerNoticeLease("7");
  assert(
    await lease.check(async () => {
      throw new Error("temporary IPC error");
    }),
  );
  assert(lease.isPresent());
  assert(!(await lease.check(async () => false)));
});
test("unmounted observers neither read nor deliver a delayed close callback", async () => {
  const lease = new ViewerNoticeLease("8");
  let active = true;
  let deliver;
  let reads = 0;
  const pending = inspectViewerNotice(
    lease,
    () =>
      new Promise((resolve) => {
        reads++;
        deliver = resolve;
      }),
    () => active,
    () => assert.fail("unmounted observer"),
  );
  active = false;
  deliver(false);
  await pending;
  await inspectViewerNotice(
    lease,
    async () => {
      reads++;
      return false;
    },
    () => active,
    () => assert.fail("inactive"),
  );
  assert.equal(reads, 1);
});
