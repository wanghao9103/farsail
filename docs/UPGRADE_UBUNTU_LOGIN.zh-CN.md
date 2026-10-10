[English](UPGRADE_UBUNTU_LOGIN.md) | **简体中文**

# Ubuntu 绑定与登录提示：现网升级

2026-10-09 对现网 HTTPS 接口的无凭据兼容性检查返回：`platform: unknown variant linux, expected windows/android/ios`（HTTP 422）。这确认服务端仍不支持 Linux 设备，必须升级协调服务；仅重新安装 Ubuntu 客户端无法解决绑定失败。

此次协调服务更新加入 Linux 控制端平台与数据库约束迁移，并为登录返回 `account_not_found`、`invalid_password`、`email_not_verified`、`account_disabled`。客户端 0.1.18 将其显示为明确的中文弹窗。Ubuntu 仍为控制端；被控 Windows 电脑须开启共享。

## 服务器在线升级

在线升级通过 HTTPS 下载固定版本协调服务包，核对完整 SHA256 和包内校验，再调用备份、升级与回退流程。[服务端预览包已发布](https://github.com/wanghao9103/farsail/releases/tag/coordinator-ubuntu-login-20261009)。客户端应用内自动更新留到后续。

在服务器以有 Docker 和部署文件访问权限的用户执行以下命令：

```bash
set -euo pipefail
update_dir=$(mktemp -d /tmp/farsail-update.XXXXXX)
curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
  --retry 3 --connect-timeout 20 --output "$update_dir/package.tar.gz" \
  "https://github.com/wanghao9103/farsail/releases/download/coordinator-ubuntu-login-20261009/FarSail_coordinator_ubuntu-login_20261009.tar.gz"
printf '%s  %s\n' 'bab23db531d2c79acba9666fb699e3465f03cca2718a1d8d66717b5a128aa12a' "$update_dir/package.tar.gz" | sha256sum -c -
mkdir "$update_dir/release"
tar -xzf "$update_dir/package.tar.gz" -C "$update_dir/release"
(cd "$update_dir/release" && sha256sum -c SHA256SUMS)
bash "$update_dir/release/upgrade-coordinator.sh" "$update_dir/release" /home/data/farsail
```

默认部署目录是 `/home/data/farsail`，不同目录请替换最后一个参数。保留既有 `FARSAIL_STATE_DIR` 配置。下载或校验失败不会修改部署。实际升级仍需已有 Docker、Compose、jq 和数据库访问权限。发布包内的“尚未发布”文字及元数据是构建时记录，当前可用下载地址以此文档和发布页为准。

已包含新脚本的检出目录也可以执行：

```bash
bash scripts/deploy/update-online.sh HTTPS_PACKAGE_URL RELEASE_PACKAGE_SHA256 /home/data/farsail
```

两个占位参数分别使用上方命令中的下载地址和 SHA256，不直接执行未校验的远程脚本。

## 1. 上传离线升级包

交付的 `FarSail_coordinator_ubuntu-login_20261009.tar.gz` 内含单服务镜像、校验文件、升级脚本和本说明。先从当前开发电脑传至服务器（SSH 登录名按实际配置填写）：

```bash
scp /本机路径/FarSail_coordinator_ubuntu-login_20261009.tar.gz SSH登录名@39.105.83.33:/tmp/
```

镜像在开发电脑构建，服务器无需安装 Rust 或重新编译。不要使用以前的六镜像 `load-release` 命令加载这个单服务包。

## 2. 在服务器执行升级

以下沿用原部署目录 `/home/data/farsail`。若实际目录不同，替换脚本最后的目录参数；若原部署设置了 `FARSAIL_STATE_DIR`，此次也必须设置为相同目录。使用能访问 Docker 和私有部署文件的用户执行（通常为 root）。

```bash
mkdir -p /tmp/farsail-ubuntu-login-20261009
tar -xzf /tmp/FarSail_coordinator_ubuntu-login_20261009.tar.gz -C /tmp/farsail-ubuntu-login-20261009
cd /tmp/farsail-ubuntu-login-20261009
sha256sum -c SHA256SUMS
bash upgrade-coordinator.sh "$PWD" /home/data/farsail
```

脚本验证镜像平台和配置摘要，先备份数据库及 `compose.env`，然后只重建 `coordinator`。新服务启动时自动运行新增 Linux 迁移，不需要手动执行 SQL。健康检查失败时自动恢复旧镜像配置。会短暂中断协调服务，请在没有正在进行的远程会话时执行。

备份位于原状态目录的 `backups/coordinator-TIMESTAMP-PID/`，权限限制为私有文件。无需再次初始化、申请证书或修改中继配置。不要运行 `docker compose down -v`。

## 3. 验收

```bash
cd /home/data/farsail
bash scripts/deploy/farsail.sh status
curl --fail --silent --show-error https://39.105.83.33/healthz
curl --silent --show-error -i https://39.105.83.33/v1/devices/bind \
  -H 'Content-Type: application/json' \
  --data '{"challenge_id":"00000000-0000-0000-0000-000000000000","signature":"00","name":"compatibility-check","platform":"linux","can_host":false,"can_files":false}'
```

最后一条没有登录凭据，预期返回 **401**，说明 Linux 参数解析通过并进入认证；旧版返回 **422 unknown variant linux**。这个探测不会添加设备。健康接口返回 200 本身不能证明服务版本正确。

随后安装新版 Ubuntu 客户端 0.1.18：

```bash
sudo apt install ./FarSail_0.1.18_ubuntu26.04_amd64.deb
```

打开客户端：不存在的邮箱应提示“账号不存在”；已有账号的错误密码应提示“密码错误”；正确登录后在总览“添加这台电脑”。绑定成功后从“我的设备”连接已开启共享的 Windows 电脑。真实账号、绑定及跨机器远控须在现网实际验收。

## 4. 手动回退协调服务

若健康检查通过但业务验收失败，保留升级时输出的备份路径，将其中 `compose.env` 恢复到原状态目录，再只重建协调服务：

```bash
cd /home/data/farsail
# Replace with the backup directory printed by the upgrade
backup_dir=/home/data/farsail/.local/production/backups/coordinator-ACTUAL-TIMESTAMP-PID
cp "$backup_dir/compose.env" .local/production/compose.env
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml up -d --pull never --no-deps \
  --force-recreate --wait --wait-timeout 180 coordinator
```

非默认状态目录需同步替换上述路径。回退会失去新登录原因和 Linux 绑定支持；新增迁移保持，不能删迁移记录或把平台约束缩回去。已绑定 Linux 的设备属于新功能数据，旧版本的完整业务兼容性需单独核对。数据库转储用于另行审慎恢复，普通镜像回退不自动覆盖当前数据。

本地验证结果与未覆盖部分见交付包的 `coordinator-release.json`。此文档和本地升级测试不代表现网已经升级。
