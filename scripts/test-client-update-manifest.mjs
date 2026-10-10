import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { verifyArtifact, buildManifest } from "./client-update-manifest.mjs";

// Disposable test signing key, unrelated to the production key.
const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const keyId = Buffer.from("0011223344556677", "hex");
const pubkey = Buffer.from(
  `untrusted comment: test only\n${Buffer.concat([Buffer.from("Ed"), keyId, publicKey.export({ format: "der", type: "spki" }).subarray(-32)]).toString("base64")}\n`,
).toString("base64");
function signature(bytes, version = "0.1.20") {
  const sig = sign(
    null,
    createHash("blake2b512").update(bytes).digest(),
    privateKey,
  );
  const comment = `timestamp:1\tfile:fixture\tversion:${version}`;
  const global = sign(
    null,
    Buffer.concat([sig, Buffer.from(comment)]),
    privateKey,
  );
  return Buffer.from(
    `untrusted comment: test only\n${Buffer.concat([Buffer.from("ED"), keyId, sig]).toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`,
  ).toString("base64");
}
test("signed update rejects altered bytes, advertised versions and trusted comments", () => {
  const bytes = Buffer.from("fixture");
  const encoded = signature(bytes);
  verifyArtifact(bytes, encoded, pubkey, "0.1.20");
  assert.throws(
    () => verifyArtifact(Buffer.from("tampered"), encoded, pubkey, "0.1.20"),
    /verification failed/,
  );
  assert.throws(
    () => verifyArtifact(bytes, encoded, pubkey, "0.1.21"),
    /version mismatch/,
  );
  const altered = Buffer.from(
    Buffer.from(encoded, "base64")
      .toString()
      .replace("version:0.1.20", "version:0.1.21"),
  ).toString("base64");
  assert.throws(
    () => verifyArtifact(bytes, altered, pubkey, "0.1.21"),
    /verification failed/,
  );
});
test("publication requires both tested platform artifacts from one source", () => {
  const root = mkdtempSync(join(tmpdir(), "farsail-update-"));
  const commit = "a".repeat(40);
  const config = { version: "0.1.20", plugins: { updater: { pubkey } } };
  try {
    for (const platform of ["linux", "windows"]) {
      const dir = join(root, platform);
      mkdirSync(dir);
      const name =
        platform === "linux"
          ? "FarSail_0.1.20_amd64.deb"
          : "FarSail_0.1.20_x64-setup.exe";
      const bytes = Buffer.from(
        platform === "linux" ? "!<arch>\nfixture" : "MZfixture",
      );
      const digest = createHash("sha256").update(bytes).digest("hex");
      writeFileSync(join(dir, name), bytes);
      writeFileSync(join(dir, name + ".sig"), signature(bytes));
      writeFileSync(
        join(dir, "release-metadata.json"),
        JSON.stringify({
          version: "0.1.20",
          source_commit: commit,
          installer: name,
          installer_bytes: bytes.length,
          installer_sha256: digest,
        }),
      );
      if (platform === "windows")
        writeFileSync(
          join(dir, "installation-verification.json"),
          JSON.stringify({
            source_commit: commit,
            installer_sha256: digest,
            ...Object.fromEntries(
              [
                "install",
                "embedded_frontend",
                "native_settings_ipc_restart",
                "sharing_preferences_restart",
                "reinstall_retention",
                "uninstall_retention",
                "debug_probes_absent",
              ].map((field) => [field, "passed"]),
            ),
          }),
        );
    }
    const manifest = buildManifest(root, join(root, "output"), config, commit);
    assert.equal(Object.keys(manifest.platforms).length, 2);
    assert.equal(
      JSON.parse(readFileSync(join(root, "output/latest.json"))).version,
      "0.1.20",
    );
    assert.throws(
      () =>
        buildManifest(root, join(root, "wrong-source"), config, "b".repeat(40)),
      /metadata/,
    );
    rmSync(join(root, "windows/installation-verification.json"));
    assert.throws(() =>
      buildManifest(root, join(root, "missing-report"), config, commit),
    );
    rmSync(join(root, "linux/FarSail_0.1.20_amd64.deb.sig"));
    assert.throws(() =>
      buildManifest(root, join(root, "unsigned"), config, commit),
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
