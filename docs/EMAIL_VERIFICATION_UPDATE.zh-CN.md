[English](EMAIL_VERIFICATION_UPDATE.md) | **简体中文**

# 邮箱验证码与正式邮件升级

此次更新包含正式中文验证及密码重置邮件、纯文本与 HTML 正文、6 位邮箱验证码和验证页直接重发。邮件明确说明用途、10 分钟有效期、单次使用及最新邮件优先；找回密码的重置码保持原有随机强度与 30 分钟有效期。

验证码绑定规范化邮箱，数据库仅保存摘要。每个邮箱 15 分钟最多尝试 5 次，重发使旧码失效；此前 43 字符长验证串仍在原有效期内兼容。登录会话、设备和中继凭据不改成数字短码。新增日志只记录发送阶段与跳过原因，不输出验证码、密码或收件人。

## 1. 先安装客户端

数字验证码要求客户端在验证请求中带邮箱。先安装 0.1.19 或以上客户端，再升级服务端。客户端兼容旧服务器的长验证串；重发使用本次注册在内存中保留的邮箱和密码，页面不展示邮箱密码表单，也不跳转。重新启动客户端导致注册上下文丢失时，可先在登录页填写原账号信息，再进入验证页。

本次本地生成的是 Ubuntu 26.04 amd64 安装包；其他系统的二进制未构建。将已提供的安装包放到当前目录后执行：

```bash
sudo apt install ./FarSail_0.1.19_ubuntu26.04_amd64.deb
```

客户端应用内在线升级留到后续。本次公开的服务端附件中不包含客户端安装包。

## 2. 服务器从 GitHub 下载升级

[服务端发布页](https://github.com/wanghao9103/farsail/releases/tag/coordinator-ubuntu-login-20261009)新增以下附件；原 Ubuntu 绑定与登录升级包保持原字节。

- `FarSail_coordinator_verification-code_20261009.tar.gz`
- `FarSail_coordinator_verification-code_20261009.tar.gz.sha256`

部署目录默认 /home/data/farsail，如不同请修改最后一个参数。保留既有 FARSAIL_STATE_DIR；已有数据库与 SMTP 配置无需重写。命令先验证外层和内层 SHA256，保存私有数据库备份，再仅重建协调服务。启动失败自动恢复旧镜像；备份路径会输出。已有远程连接可能因协调服务重建中断，升级后重新连接。服务器无需安装 Rust 或编译。不要重复执行已经选用该镜像的更新。

```bash
set -euo pipefail
mail_update_dir=$(mktemp -d /tmp/farsail-mail-update.XXXXXX)
curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
  --retry 3 --connect-timeout 20 --output "$mail_update_dir/package.tar.gz" \
  "https://github.com/wanghao9103/farsail/releases/download/coordinator-ubuntu-login-20261009/FarSail_coordinator_verification-code_20261009.tar.gz"
printf '%s  %s\n' '4500264bfb0c47437fe9dacb3f2af9f994f91a03a56c38121d7bd6394df53171' "$mail_update_dir/package.tar.gz" | sha256sum -c -
mkdir "$mail_update_dir/release"
tar -xzf "$mail_update_dir/package.tar.gz" -C "$mail_update_dir/release"
(cd "$mail_update_dir/release" && sha256sum -c SHA256SUMS)
bash "$mail_update_dir/release/upgrade-coordinator.sh" "$mail_update_dir/release" /home/data/farsail
```

## 3. 验收

在客户端对未验证账号直接重新发送，确认仍停留验证页。检查收件箱及垃圾邮件，邮件标题为“FarSail 遥舟｜邮箱验证”，输入最新 6 位验证码并登录。重发后旧码、过期和重复提交应失败；达到尝试限制后等待 15 分钟。旧 SMTP 配置无需修改。

```bash
cd /home/data/farsail
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml logs --tail 80 coordinator
```

若使用自定义状态目录，请对应替换环境文件路径。mail_delivery / smtp_accepted 表示 SMTP 服务已接受；不代表已进入收件箱。verification_resend 的 skipped 表示请求未满足发信条件。请勿分享密码、验证码和配置全文。

## 4. 验证范围

本机 Rust 1.93、PostgreSQL 17 的邮箱绑定、有效期、重放、并发验证与重发、限流和旧验证串兼容通过；原账号、设备和远控授权回归通过。WebKit 界面合成测试覆盖重发成功与失败均保留验证页和输入，未出现邮箱密码输入框。真实新服务镜像通过本地 SMTP 接收、中文 MIME、发送拒绝返回错误、重试及找回邮件检查。独立 Compose 从旧镜像升级、保留账号和数据库容器、私有备份及失败启动自动回退通过。Ubuntu 客户端构建通过。

未执行用户现网升级及新模板经真实提供商投递。旧版真实邮件已经由用户截图证明投递到垃圾邮件；不能据此证明新模板的收件位置。邮件排版不能保证进入收件箱。本机公网附件重新下载受网络限制，上传状态及摘要需在发布后核对。

镜像来自本地未提交工作区，基线与验证范围见包内 coordinator-release.json。GitHub 标签自动生成的 Source code 为旧基线；服务端部署使用上方命名附件。修复不新增验证码表迁移，仍包含已有 Linux 平台迁移；回退保留该兼容迁移。

- [真实邮箱 SMTP 配置](SMTP_SETUP.zh-CN.md)
- [API](API.zh-CN.md)
