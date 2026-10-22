import assert from "node:assert/strict";
import { test } from "node:test";
import { verifyLinuxArchitecture } from "./verify-linux-architecture.mjs";

function elf(machine) {
  const bytes = Buffer.alloc(64);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(machine, 18);
  return bytes;
}

test("ARM64 and x86_64 require their real ELF machine, not a filename label", () => {
  assert.equal(
    verifyLinuxArchitecture(elf(183), "aarch64-unknown-linux-gnu").deb,
    "arm64",
  );
  assert.equal(
    verifyLinuxArchitecture(elf(62), "x86_64-unknown-linux-gnu").deb,
    "amd64",
  );
  assert.throws(() =>
    verifyLinuxArchitecture(elf(62), "aarch64-unknown-linux-gnu"),
  );
  assert.throws(() =>
    verifyLinuxArchitecture(elf(183), "x86_64-unknown-linux-gnu"),
  );
});

test("32-bit, non-ELF, big-endian, truncated and unsupported targets are refused", () => {
  for (const index of [0, 4, 5, 6]) {
    const bytes = elf(183);
    bytes[index] = 0;
    assert.throws(() =>
      verifyLinuxArchitecture(bytes, "aarch64-unknown-linux-gnu"),
    );
  }
  assert.throws(() =>
    verifyLinuxArchitecture(
      elf(183).subarray(0, 63),
      "aarch64-unknown-linux-gnu",
    ),
  );
  assert.throws(() =>
    verifyLinuxArchitecture(elf(183), "armv7-unknown-linux-gnueabihf"),
  );
});
