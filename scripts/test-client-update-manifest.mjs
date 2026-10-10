import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  renameSync,
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
const commit = "a".repeat(40);
const config = { version: "0.1.20", plugins: { updater: { pubkey } } };
const fixtures = [
  ["linux", "amd64.deb", "x86_64-unknown-linux-gnu", "amd64"],
  ["linux-arm64", "arm64.deb", "aarch64-unknown-linux-gnu", "arm64"],
  ["windows", "x64-setup.exe", "x86_64-pc-windows-msvc", null],
];
function withArtifacts(run) {
  const root = mkdtempSync(join(tmpdir(), "farsail-update-"));
  try {
    for (const [platform, suffix, target, debArchitecture] of fixtures) {
      const dir = join(root, platform);
      mkdirSync(dir);
      const name = `FarSail_0.1.20_${suffix}`;
      const bytes = Buffer.from(
        `${debArchitecture ? "!<arch>\n" : "MZ"}fixture-${target}`,
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
          target,
          ...(debArchitecture
            ? {
                deb_architecture: debArchitecture,
                elf_machine: debArchitecture === "arm64" ? 183 : 62,
                native_debug_ipc: "passed",
                linux_store: "passed",
                package_dependencies: "passed",
              }
            : {}),
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
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}
function changeMetadata(root, platform, changes) {
  const path = join(root, platform, "release-metadata.json");
  writeFileSync(
    path,
    JSON.stringify({ ...JSON.parse(readFileSync(path, "utf8")), ...changes }),
  );
}
test("publication requires three signed architectures from one exact source", () => {
  withArtifacts((root) => {
    const output = join(root, "output");
    const manifest = buildManifest(root, output, config, commit);
    assert.deepEqual(Object.keys(manifest.platforms), [
      "linux-x86_64-deb",
      "linux-aarch64-deb",
      "windows-x86_64-nsis",
    ]);
    for (const [platform, name] of [
      ["linux-x86_64-deb", "FarSail_0.1.20_amd64.deb"],
      ["linux-aarch64-deb", "FarSail_0.1.20_arm64.deb"],
      ["windows-x86_64-nsis", "FarSail_0.1.20_x64-setup.exe"],
    ]) {
      assert.equal(
        manifest.platforms[platform].url,
        `https://github.com/wanghao9103/farsail/releases/download/client-v0.1.20/${name}`,
      );
      verifyArtifact(
        readFileSync(join(output, name)),
        manifest.platforms[platform].signature,
        pubkey,
        manifest.version,
      );
    }
    const checksums = readFileSync(join(output, "SHA256SUMS.txt"), "utf8")
      .trim()
      .split("\n");
    assert.equal(checksums.length, 11);
    for (const entry of checksums) {
      const [digest, name] = entry.split("  ");
      assert.equal(
        createHash("sha256")
          .update(readFileSync(join(output, name)))
          .digest("hex"),
        digest,
      );
    }
    assert.equal(
      JSON.parse(readFileSync(join(output, "latest.json"))).version,
      "0.1.20",
    );
  });
});
test("publication rejects mixed source and altered artifact metadata", () => {
  withArtifacts((root) => {
    assert.throws(
      () =>
        buildManifest(root, join(root, "wrong-source"), config, "b".repeat(40)),
      /metadata/,
    );
    changeMetadata(root, "linux-arm64", { installer_sha256: "0".repeat(64) });
    assert.throws(
      () => buildManifest(root, join(root, "wrong-hash"), config, commit),
      /metadata/,
    );
  });
});
test("publication requires each architecture and rejects duplicate installers", () => {
  for (const [platform, suffix] of fixtures) {
    withArtifacts((root) => {
      rmSync(join(root, platform), { recursive: true });
      assert.throws(() =>
        buildManifest(root, join(root, "missing"), config, commit),
      );
    });
    withArtifacts((root) => {
      writeFileSync(join(root, platform, `duplicate_${suffix}`), "duplicate");
      assert.throws(
        () => buildManifest(root, join(root, "duplicate"), config, commit),
        /exactly one/,
      );
    });
  }
});
test("publication rejects wrong-architecture filenames even with valid signatures", () => {
  withArtifacts((root) => {
    const dir = join(root, "linux-arm64");
    const original = "FarSail_0.1.20_arm64.deb";
    const wrong = "FarSail_0.1.20_amd64.deb";
    renameSync(join(dir, original), join(dir, wrong));
    renameSync(join(dir, original + ".sig"), join(dir, wrong + ".sig"));
    changeMetadata(root, "linux-arm64", { installer: wrong });
    assert.throws(
      () =>
        buildManifest(root, join(root, "wrong-architecture"), config, commit),
      /filename version or architecture mismatch/,
    );
  });
});
test("renaming an amd64 build to arm64 cannot satisfy the build target contract", () => {
  withArtifacts((root) => {
    const source = join(root, "linux", "FarSail_0.1.20_amd64.deb");
    const destination = join(root, "linux-arm64", "FarSail_0.1.20_arm64.deb");
    writeFileSync(destination, readFileSync(source));
    writeFileSync(destination + ".sig", readFileSync(source + ".sig"));
    changeMetadata(root, "linux-arm64", {
      ...JSON.parse(readFileSync(join(root, "linux", "release-metadata.json"))),
      installer: "FarSail_0.1.20_arm64.deb",
    });
    assert.throws(
      () => buildManifest(root, join(root, "renamed"), config, commit),
      /metadata architecture mismatch/,
    );
  });
});
test("publication rejects target triples and Debian architecture metadata mismatches", () => {
  for (const [platform] of fixtures) {
    withArtifacts((root) => {
      changeMetadata(root, platform, { target: "aarch64-pc-windows-msvc" });
      assert.throws(
        () => buildManifest(root, join(root, "wrong-target"), config, commit),
        /metadata architecture mismatch/,
      );
    });
  }
  withArtifacts((root) => {
    changeMetadata(root, "linux-arm64", { deb_architecture: "amd64" });
    assert.throws(
      () => buildManifest(root, join(root, "wrong-deb"), config, commit),
      /metadata architecture mismatch/,
    );
  });
});
test("ARM64 publication rejects missing signatures and signed-byte tampering", () => {
  withArtifacts((root) => {
    rmSync(join(root, "linux-arm64/FarSail_0.1.20_arm64.deb.sig"));
    assert.throws(() =>
      buildManifest(root, join(root, "unsigned"), config, commit),
    );
  });
  withArtifacts((root) => {
    const path = join(root, "linux-arm64/FarSail_0.1.20_arm64.deb");
    writeFileSync(
      path,
      Buffer.concat([readFileSync(path), Buffer.from("altered")]),
    );
    assert.throws(
      () => buildManifest(root, join(root, "tampered"), config, commit),
      /signature verification failed/,
    );
  });
});
test("Linux publication requires correct ELF architecture and successful native checks", () => {
  for (const platform of ["linux", "linux-arm64"]) {
    for (const changes of [
      { elf_machine: platform === "linux" ? 183 : 62 },
      { elf_machine: undefined },
      { native_debug_ipc: undefined },
      { native_debug_ipc: "failed" },
      { linux_store: undefined },
      { linux_store: "failed" },
      { package_dependencies: undefined },
      { package_dependencies: "failed" },
    ]) {
      withArtifacts((root) => {
        changeMetadata(root, platform, changes);
        assert.throws(
          () => buildManifest(root, join(root, "unverified"), config, commit),
          /Linux installer verification has not passed/,
        );
      });
    }
  }
});
test("Windows installation verification remains required for all-platform publication", () => {
  withArtifacts((root) => {
    rmSync(join(root, "windows/installation-verification.json"));
    assert.throws(() =>
      buildManifest(root, join(root, "missing-report"), config, commit),
    );
  });
});
