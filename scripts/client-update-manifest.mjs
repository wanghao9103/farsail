import { execFileSync } from "node:child_process";
import { createHash, createPublicKey, verify } from "node:crypto";
import {
  readFileSync,
  readdirSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Verify the artifact and the signed version, including minisign's trusted-comment signature.
// This mirrors the updater's format; never trust the manifest version or checksum alone.
export function verifyArtifact(
  bytes,
  encodedSignature,
  encodedPublicKey,
  version,
) {
  const publicLines = Buffer.from(encodedPublicKey.trim(), "base64")
    .toString("utf8")
    .trim()
    .split(/\r?\n/);
  const key = Buffer.from(publicLines[1] || "", "base64");
  const lines = Buffer.from(encodedSignature.trim(), "base64")
    .toString("utf8")
    .trim()
    .split(/\r?\n/);
  const signature = Buffer.from(lines[1] || "", "base64");
  if (
    key.length !== 42 ||
    signature.length !== 74 ||
    !lines[2]?.startsWith("trusted comment: ")
  )
    throw new Error("Invalid updater signature format");
  if (!signature.subarray(2, 10).equals(key.subarray(2, 10)))
    throw new Error("Updater signing key mismatch");
  const algorithm = signature.subarray(0, 2).toString();
  if (!["ED", "Ed"].includes(algorithm))
    throw new Error("Invalid signing algorithm");
  const pubkey = createPublicKey({
    key: Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      key.subarray(10),
    ]),
    type: "spki",
    format: "der",
  });
  const data =
    algorithm === "ED"
      ? createHash("blake2b512").update(bytes).digest()
      : bytes;
  const comment = lines[2].slice("trusted comment: ".length);
  const rawSignature = signature.subarray(10);
  if (
    !verify(null, data, pubkey, rawSignature) ||
    !verify(
      null,
      Buffer.concat([rawSignature, Buffer.from(comment)]),
      pubkey,
      Buffer.from(lines[3] || "", "base64"),
    )
  )
    throw new Error("Updater signature verification failed");
  if (
    comment.split("\t").find((field) => field.startsWith("version:")) !==
    `version:${version}`
  )
    throw new Error("Signed artifact version mismatch");
}

export function buildManifest(
  input,
  output,
  config,
  sourceCommit,
  pubDate = new Date().toISOString(),
) {
  const version = config.version;
  if (!/^\d+\.\d+\.\d+$/.test(version))
    throw new Error("Client updates require a stable semver version");
  if (!/^[a-f0-9]{40}$/.test(sourceCommit))
    throw new Error("Missing exact source commit");
  const files = [];
  const platforms = {};
  for (const [platform, directory, extension] of [
    ["linux-x86_64-deb", "linux", ".deb"],
    ["windows-x86_64-nsis", "windows", "-setup.exe"],
  ]) {
    const root = join(input, directory);
    const matches = readdirSync(root).filter((file) =>
      file.endsWith(extension),
    );
    if (matches.length !== 1)
      throw new Error(`Expected exactly one ${platform} installer`);
    const name = matches[0];
    if (!name.startsWith(`FarSail_${version}_`))
      throw new Error("Installer filename version mismatch");
    const bytes = readFileSync(join(root, name));
    if (
      extension === ".deb"
        ? bytes.subarray(0, 8).toString() !== "!<arch>\n"
        : bytes.subarray(0, 2).toString() !== "MZ"
    )
      throw new Error("Wrong installer format");
    const signature = readFileSync(join(root, `${name}.sig`), "utf8").trim();
    verifyArtifact(bytes, signature, config.plugins.updater.pubkey, version);
    const metadata = JSON.parse(
      readFileSync(join(root, "release-metadata.json"), "utf8").replace(
        /^\uFEFF/,
        "",
      ),
    );
    const digest = createHash("sha256").update(bytes).digest("hex");
    if (
      metadata.version !== version ||
      metadata.source_commit !== sourceCommit ||
      metadata.installer !== name ||
      metadata.installer_sha256 !== digest ||
      metadata.installer_bytes !== bytes.length
    )
      throw new Error("Build metadata does not match the published artifact");
    files.push(
      [join(root, name), name],
      [join(root, `${name}.sig`), `${name}.sig`],
      [
        join(root, "release-metadata.json"),
        `${directory}-release-metadata.json`,
      ],
    );
    if (directory === "windows") {
      const reportPath = join(root, "installation-verification.json");
      const report = JSON.parse(
        readFileSync(reportPath, "utf8").replace(/^\uFEFF/, ""),
      );
      if (
        report.source_commit !== sourceCommit ||
        report.installer_sha256 !== digest ||
        [
          "install",
          "embedded_frontend",
          "native_settings_ipc_restart",
          "sharing_preferences_restart",
          "reinstall_retention",
          "uninstall_retention",
          "debug_probes_absent",
        ].some((field) => report[field] !== "passed")
      )
        throw new Error(
          "Windows installer verification has not passed for this artifact",
        );
      files.push([reportPath, "windows-installation-verification.json"]);
    }
    platforms[platform] = {
      signature,
      url: `https://github.com/wanghao9103/farsail/releases/download/client-v${version}/${encodeURIComponent(name)}`,
    };
  }
  mkdirSync(output, { recursive: true });
  if (readdirSync(output).length)
    throw new Error("Use an empty publication directory");
  for (const [path, name] of files) copyFileSync(path, join(output, name));
  const manifest = {
    version,
    notes: "FarSail 客户端更新。安装后会重新启动；远程连接进行中会等待。",
    pub_date: pubDate,
    platforms,
  };
  writeFileSync(
    join(output, "latest.json"),
    JSON.stringify(manifest, null, 2) + "\n",
  );
  writeFileSync(
    join(output, "SHA256SUMS.txt"),
    readdirSync(output)
      .sort()
      .map(
        (name) =>
          `${createHash("sha256")
            .update(readFileSync(join(output, name)))
            .digest("hex")}  ${name}`,
      )
      .join("\n") + "\n",
  );
  return manifest;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const [, , input, output, sourceCommit] = process.argv;
  if (!input || !output)
    throw new Error(
      "Usage: node scripts/client-update-manifest.mjs INPUT OUTPUT SOURCE_COMMIT",
    );
  const config = JSON.parse(
    readFileSync("apps/desktop/src-tauri/tauri.conf.json", "utf8"),
  );
  const pubDate = new Date(
    execFileSync("git", ["show", "-s", "--format=%cI", sourceCommit], {
      encoding: "utf8",
    }).trim(),
  ).toISOString();
  const manifest = buildManifest(input, output, config, sourceCommit, pubDate);
  console.log(
    `Verified ${manifest.version}: ${Object.keys(manifest.platforms).join(", ")}`,
  );
}
