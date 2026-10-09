import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { verifyArtifact } from "./client-update-manifest.mjs";

const repository = "wanghao9103/farsail";
const secretName = "TAURI_SIGNING_PRIVATE_KEY";

// Maintainer setup only. Neither this helper nor any private key is bundled in the client.
export function configure({ root, keyPath, apply = false, run = spawnSync }) {
  const config = JSON.parse(
    readFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"),
  );
  if (!statSync(keyPath).isFile())
    throw new Error("签名私钥文件不存在，请从安全备份恢复。");
  const temporary = mkdtempSync(join(tmpdir(), "farsail-signing-check-"));
  try {
    const probe = join(temporary, "key-check.txt");
    const bytes = Buffer.from("FarSail updater signing key verification");
    writeFileSync(probe, bytes);
    const signed = run(
      process.execPath,
      [
        join(root, "node_modules/@tauri-apps/cli/tauri.js"),
        "signer",
        "sign",
        "--private-key-path",
        keyPath,
        "--password",
        "",
        "--app-version",
        config.version,
        probe,
      ],
      { cwd: root, encoding: "utf8", stdio: "pipe", timeout: 30000 },
    );
    if (signed.error || signed.status !== 0)
      throw new Error(
        "签名验证未完成。请先运行 npm ci，并使用对应的签名私钥。",
      );
    verifyArtifact(
      bytes,
      readFileSync(probe + ".sig", "utf8"),
      config.plugins.updater.pubkey,
      config.version,
    );
    if (!apply)
      return "签名私钥与应用公钥匹配。仅完成本地检查，未登录或修改 GitHub。";
    const authenticated = run(
      "gh",
      ["auth", "status", "--hostname", "github.com"],
      { cwd: root, stdio: "pipe", timeout: 30000 },
    );
    if (authenticated.error || authenticated.status !== 0)
      throw new Error(
        "请安装 GitHub CLI 并运行 gh auth login --hostname github.com，使用有仓库 Secrets 写入权限的账号。",
      );
    const privateBytes = readFileSync(keyPath);
    try {
      // gh encrypts the secret locally for the repository public key. It is never placed in argv or logs.
      const saved = run(
        "gh",
        ["secret", "set", secretName, "--repo", repository],
        { cwd: root, input: privateBytes, stdio: "pipe", timeout: 60000 },
      );
      if (saved.error || saved.status !== 0)
        throw new Error(
          "签名 Secret 保存失败。请核对仓库 Secrets 写入权限；私钥未输出到日志。",
        );
    } finally {
      privateBytes.fill(0);
    }
    return `已配置 ${repository} 的 ${secretName}。后续发布自动读取，无需用户填写。请保留安全备份。`;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const args = process.argv.slice(2);
  let keyPath = join(root, ".local/client-updater/private/farsail-updater.key");
  let apply = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--apply") apply = true;
    else if (
      args[i] === "--private-key" &&
      args[i + 1] &&
      !args[i + 1].startsWith("--")
    )
      keyPath = resolve(args[++i]);
    else
      throw new Error(
        "Usage: node scripts/configure-client-updates.mjs [--private-key FILE] [--apply]",
      );
  }
  try {
    console.log(configure({ root, keyPath, apply }));
  } catch (error) {
    console.error(
      error.code === "ENOENT"
        ? "签名私钥或依赖不存在，请从安全备份恢复密钥并运行 npm ci。"
        : error.message,
    );
    process.exitCode = 1;
  }
}
