import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync, createHash, sign } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { configure } from "./configure-client-updates.mjs";

test("maintainer setup checks key identity before sending, keeps secrets out of arguments and output", () => {
  const root = mkdtempSync(join(tmpdir(), "farsail-secret-fixture-"));
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const id = Buffer.alloc(8, 1);
  const pubkey = Buffer.from(
    `untrusted comment: disposable\n${Buffer.concat([Buffer.from("Ed"), id, publicKey.export({ type: "spki", format: "der" }).subarray(-32)]).toString("base64")}\n`,
  ).toString("base64");
  const keyPath = join(root, "test-only.key");
  const secret = "disposable-fixture-not-a-real-key";
  const calls = [];
  let wrongVersion = false;
  const run = (command, args, options) => {
    calls.push({ command, args });
    if (command !== "gh") {
      const probe = args.at(-1),
        bytes = readFileSync(probe);
      const raw = sign(
        null,
        createHash("blake2b512").update(bytes).digest(),
        privateKey,
      );
      const comment = `timestamp:1\tversion:${wrongVersion ? "0.1.19" : "0.1.20"}`;
      const global = sign(
        null,
        Buffer.concat([raw, Buffer.from(comment)]),
        privateKey,
      );
      writeFileSync(
        probe + ".sig",
        Buffer.from(
          `untrusted comment: disposable\n${Buffer.concat([Buffer.from("ED"), id, raw]).toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`,
        ).toString("base64"),
      );
    } else if (args[0] === "secret") {
      assert.deepEqual(args, [
        "secret",
        "set",
        "TAURI_SIGNING_PRIVATE_KEY",
        "--repo",
        "wanghao9103/farsail",
      ]);
      assert.equal(options.input.toString(), secret);
    }
    assert(!args.some((arg) => arg.includes(secret)));
    assert.equal(options.stdio, "pipe");
    return { status: 0 };
  };
  try {
    mkdirSync(join(root, "apps/desktop/src-tauri"), { recursive: true });
    writeFileSync(
      join(root, "apps/desktop/src-tauri/tauri.conf.json"),
      JSON.stringify({ version: "0.1.20", plugins: { updater: { pubkey } } }),
    );
    writeFileSync(keyPath, secret);
    assert(!configure({ root, keyPath, run }).includes(secret));
    assert.equal(calls.filter((call) => call.command === "gh").length, 0);
    assert(!configure({ root, keyPath, apply: true, run }).includes(secret));
    assert.equal(
      calls.filter((call) => call.command === "gh" && call.args[0] === "secret")
        .length,
      1,
    );
    wrongVersion = true;
    calls.length = 0;
    assert.throws(
      () => configure({ root, keyPath, apply: true, run }),
      /version mismatch/,
    );
    assert.equal(calls.filter((call) => call.command === "gh").length, 0);
    assert.equal(readFileSync(keyPath, "utf8"), secret);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
