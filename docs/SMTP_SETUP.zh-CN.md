[English](SMTP_SETUP.md) | **简体中文**

# 真实邮箱验证邮件配置

FarSail 已支持通过 SMTP 发送邮箱验证和密码恢复邮件。默认部署使用 Mailpit 测试收件箱，不会把邮件投递到真实邮箱。以下以已有 Linux Docker Compose 生产部署和 126 发件邮箱为例。只需配置发件邮箱，收件人无需启用 SMTP。

## 1. 准备发件邮箱

登录发件邮箱网页版，在“设置 → POP3/SMTP/IMAP”开启 SMTP，并生成客户端授权码。授权码用于服务器发信，不能用邮箱网页登录密码代替。只在服务器私有配置中填写授权码，不放入 Git、发布附件或聊天截图。

- [网易官方：新增客户端授权码](https://help.mail.126.com/faqDetail.do?code=d7a5dc8471cd0c0e8b4b8f4f8e49998b374173cfe9171305fa1ce630d7f67ac286624f309a1a7089)
- [网易官方：客户端协议服务器配置](https://help.mail.126.com/faqDetail.do?code=d7a5dc8471cd0c0e8b4b8f4f8e49998b374173cfe9171305fa1ce630d7f67ac25c12dcb3d46222b6)

示例使用 `smtp.126.com`，端口 465，隐式 TLS。其他邮箱服务按其官方参数设置；项目也支持 STARTTLS，常用端口为 587。服务器需要能解析 SMTP 主机并访问对应出站端口，无需开放入站邮件端口。

## 2. 自动更新已有配置（推荐）

新部署模板已补齐 SMTP 用户、授权码和 TLS 字段，默认仍使用测试收件箱。已有服务器不会被模板更新自动修改，可以使用独立配置脚本，不必逐行添加参数。脚本依赖 Python 3 和已有 Docker Compose，复用部署锁，先备份、再原子更新私有配置。发件邮箱和授权码在服务器交互输入，授权码不回显或传入命令行参数。

先按服务端发布页的下载地址和 SHA256 获取 configure-smtp.py，校验通过后执行。源码位于项目 scripts/deploy/configure-smtp.py；它是独立配置工具，不需要重建或更新协调服务镜像。

```bash
python3 /tmp/configure-smtp.py --deploy-root /home/data/farsail --apply
```

脚本询问主机、TLS 模式、端口、发件邮箱、SMTP 用户及授权码，保留已有数据库和中继配置。若这些基础参数缺失，会从已有私有部署配置恢复：数据库密码来自数据库容器的环境配置，中继令牌来自中继容器的环境配置，中继地址来自原设置和网关端口。不会重新生成密码、重置数据库或覆盖已有基础参数。

如果日志显示 `FARSAIL_DATABASE_URL is required`，说明当前容器未读取到数据库连接参数。可能是配置片段覆盖了整个文件，也可能是旧容器未重新加载已修正的配置。已有邮件参数无需重输，使用仅恢复基础参数的模式：

```bash
python3 /tmp/configure-smtp.py --deploy-root /home/data/farsail --repair-only --apply
```

保留既有 `FARSAIL_STATE_DIR`。脚本只重新创建协调服务；失败会恢复原文件并尝试用原配置重建。原配置本身已损坏时，恢复原文件并不保证服务能启动，需要查看日志继续排查。仅希望更新文件时省略 --apply，再按下一节命令使配置生效。备份目录由脚本打印，配置和备份权限为 600；命令不输出凭据，也不会发送测试邮件。数据库或中继的源配置缺失时拒绝修改，不猜测密码。

以下保留手动修改方法，可任选一种方式。

## 3. 手动备份并编辑配置

默认项目目录为 `/home/data/farsail`，状态目录为 `.local/production`。如果原部署使用 `FARSAIL_STATE_DIR`，保留同一个环境变量值；项目目录不同时替换下面的路径。

```bash
cd /home/data/farsail
umask 077
smtp_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
smtp_backup="$smtp_state/coordinator.env.smtp-backup-$(date +%Y%m%d-%H%M%S)-$$"
cp "$smtp_state/coordinator.env" "$smtp_backup"
chmod 600 "$smtp_backup"
nano "$smtp_state/coordinator.env"
```

只编辑邮件相关配置，不用以下片段覆盖整个文件。保留原文件中的 `FARSAIL_DATABASE_URL`、`FARSAIL_BIND`、`FARSAIL_RELAY_ACCESS_TOKEN` 和 `FARSAIL_RELAY_URLS`。

| 参数 | 原测试配置 | 真实邮箱处理 |
| --- | --- | --- |
| `FARSAIL_MAIL_MODE` | `smtp-local` | 修改为 `smtp-tls` |
| `FARSAIL_SMTP_HOST` | `127.0.0.1` | 修改为发件邮箱的 SMTP 主机 |
| `FARSAIL_SMTP_PORT` | 1025 | 修改为 465，隐式 TLS |
| `FARSAIL_MAIL_FROM` | 本地测试发件地址 | 修改为与 SMTP 用户一致的发件邮箱 |
| `FARSAIL_SMTP_USER` | 旧模板未生成，新模板为空 | 新增或填写完整发件邮箱 |
| `FARSAIL_SMTP_PASSWORD` | 旧模板未生成，新模板为空 | 新增或填写 SMTP 授权码 |
| `FARSAIL_SMTP_TLS` | 旧模板未生成 | 建议显式设置为 `implicit`，新模板已有该值 |

邮件配置完整示例：把示例邮箱和授权码替换为自己的值，确保每个参数只出现一次。

```dotenv
FARSAIL_MAIL_MODE=smtp-tls
FARSAIL_SMTP_HOST=smtp.126.com
FARSAIL_SMTP_PORT=465
FARSAIL_SMTP_TLS=implicit
FARSAIL_SMTP_USER=sender@126.com
FARSAIL_SMTP_PASSWORD='REPLACE_WITH_SMTP_AUTHORIZATION_CODE'
FARSAIL_MAIL_FROM=FarSail <sender@126.com>
```

以上是 Compose 环境文件，不要使用 shell 的 source 命令加载。[Docker 官方环境文件规则](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/)说明了引号及变量替换语法。授权码使用单引号可防止 Compose 对美元符号进行变量替换；如提供商的凭据包含引号，按 Compose 环境文件规则转义。使用 STARTTLS 时设置 `FARSAIL_SMTP_TLS=starttls` 和对应端口。TLS 证书保持正常校验。

## 4. 使配置生效

保存配置后，以能访问 Docker 和部署文件的用户执行：

```bash
cd /home/data/farsail
smtp_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
chmod 600 "$smtp_state/coordinator.env"
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml config --quiet
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml up -d --pull never \
  --no-deps --force-recreate --wait --wait-timeout 180 coordinator
```

[Docker 官方说明](https://docs.docker.com/reference/cli/docker/compose/restart/)确认仅执行 restart 不会应用配置变更，需要重新创建协调服务。上述命令仅更新协调服务，会短暂中断 API；数据库、代理及中继不重建。无需再次初始化或重新申请证书。

改用真实 SMTP 后可以停用测试邮箱：在私有 `compose.env` 的 `COMPOSE_PROFILES` 中移除 `test-mail`，保留其他需要的 profile；只有这一项时可改为空值。然后执行下面命令停止已运行的 Mailpit，保留其数据卷：

```bash
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml --profile test-mail stop mailpit
```

## 5. 验证收信

1. 在客户端注册一个自己能收信的邮箱；已有未验证账号使用原账号密码重新发送验证邮件。
2. 查看收件箱和垃圾邮件。升级后的服务发送正式中文邮件，突出显示 6 位数字验证码，并提供一致的纯文本与 HTML 正文和操作说明。
3. 在客户端验证邮箱页面输入 6 位数字验证码，然后登录。验证码有效期为 10 分钟且仅可使用一次。在当前页直接重发；重发使旧验证码失效，请使用最新邮件。客户端仅在内存中保留本次注册凭据，用于重发请求。

6 位验证码需要升级协调服务和 0.1.19 或以上客户端，由客户端在验证请求中带上邮箱。此前已发送的 43 字符旧验证串在原有效期内仍兼容。SMTP 配置无需修改；安装新版服务镜像与客户端后才会采用新格式。找回密码重置码保持原有随机强度和 30 分钟有效期，同时使用正式中文邮件模板。

注册可能已经写入账号后才发生邮件发送失败。如果再次注册提示账号已存在，使用“重新发送验证邮件”，不要反复注册同一个邮箱。重发请求成功不保证实际发送：账号必须处于启用且未验证状态，并提供正确密码。

## 6. 收不到邮件时检查

```bash
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml logs --tail 100 coordinator
```

| 现象 | 检查方向 |
| --- | --- |
| 缺少环境变量或服务启动失败 | 是否新增 SMTP 用户和授权码，是否保存并重新创建容器 |
| SMTP 认证失败 | 服务是否开启，授权码是否正确，用户是否为完整邮箱 |
| 连接超时或主机解析失败 | 服务器 DNS、云出站规则、主机防火墙和提供商网络限制 |
| TLS 握手失败 | 主机名、端口与 TLS 模式是否匹配，系统时间和 CA 是否正常 |
| 发件人被拒绝 | 发件地址是否与认证邮箱一致，账号是否有发送权限 |
| 请求成功但未收到 | 垃圾邮件、提供商投递记录及发送额度；重发条件是否满足 |

查看或提供排障信息时遮住密码、令牌和数据库连接凭据，不输出完整 Compose 配置。SMTP 服务接受邮件不等于收件箱已经投递，最终应以真实收信和邮箱验证成功为准。

## 验证范围与相关文档

本说明于 2026-10-09 核对协调服务 SMTP 参数、邮件正文、账号验证和 Compose 配置生成逻辑。真实发件邮箱、生产 SMTP 连通性及收件箱投递仍需在服务器验收；投递和重发日志只记录阶段及跳过原因，不记录凭据或验证码；SMTP 接受不等于已进入收件箱。写好配置说明不代表已配置或已发送邮件。

- [部署说明](DEPLOYMENT.zh-CN.md)
- [Ubuntu 绑定与登录提示的服务端升级](UPGRADE_UBUNTU_LOGIN.zh-CN.md)
