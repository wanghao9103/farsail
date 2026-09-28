# 公网 IP 联调部署：Ubuntu 24.04 / amd64

服务器只导入预构建镜像，不编译 Rust。发布包包括 coordinator、固定 iroh-relay 1.2.0、Nginx、PostgreSQL 17、Mailpit、Certbot 5.4.0 六个镜像。当前客户端是低帧率 JPEG，有 payload 带宽和帧期限；文件、HEVC和手机端未交付。公网签证、用户服务器与第二台电脑仍需独立验收，范围见 [WI-008A](verification/WI-008A.md)。

## 初始准备

用户自行安装官方Docker Engine/Compose。准备root、至少4GiB空闲磁盘。1.6GiB内存适合轻量联调，不承诺并发容量。常驻内存上限：协调384MiB、relay256MiB、数据库256MiB、Nginx96MiB、Mailpit128MiB；系统/Docker/临时Certbot还需余量。日志每容器3×10MB。

下面每个代码块分别执行，尖括号须替换。脚本使用Bash/jq/openssl/标准工具，没有项目自编Python依赖。

```sh
apt-get update
```

```sh
apt-get install -y git curl jq openssl ca-certificates util-linux iproute2
```

未克隆时执行（已有目录则跳过）：

```sh
git clone https://github.com/wanghao9103/farsail.git /home/data/farsail
```

```sh
cd /home/data/farsail
```

从验证记录取得完整固定提交，不能使用latest：

```sh
DEPLOY_SHA='51e131f0bf60a3a8fa15cddd869a8b97d3db79a0'
```

运行镜像使用已验证的原4252归档，加载器为仅修复跨Docker存储校验的新提交，两者明确分开：

```sh
RELEASE_SHA='4252e9d901e3022175772f563be45df967c3e9cc'
```

```sh
git fetch origin
```

```sh
git checkout --detach "$DEPLOY_SHA"
```

```sh
bash scripts/deploy/farsail.sh init '<公网IPv4>' '<ACME联系邮箱>'
```

随机数据库密码和准入bearer写入忽略的 `.local/production`，不打印、不覆盖已有状态。根目录700，env文件600；挂载私钥640/group10001供非root relay读取。API和relay运行用户10001。首次Mailpit测试模式仍需正常验证邮箱。不要公开此目录、Docker inspect完整环境或数据库备份。

已在4252版本完成init的用户必须跳过init，保留整个state。等旧load进程退出后再fetch/checkout上述加载器，重新load-release同一4252即可复用完整缓存文件；旧ID检查失败不会提交新的运行配置。新加载器只对该已审计的manifest/归档SHA组合允许跨源码ref兼容，不是忽略版本或内容检查。

```sh
bash scripts/deploy/farsail.sh preflight
```

安全组放行TCP80/443/8443，不开放5432/8787/8788/8025/1025。公网IP只用于URL/证书，本地监听0.0.0.0兼容EIP/NAT映射。协调服务仍只监听共享namespace内的127.0.0.1，不修改全局Docker网络/代理。

## 六镜像离线包

阿里云按实际产品选择ECS安全组或轻量应用服务器防火墙。初始化在创建state前检查依赖；若磁盘/权限错误中断，保留并重命名尚未投入使用的半初始化目录后重试，不覆盖已投用的秘密。Linux变更操作共用state旁的operation.lock，避免续期、导入与升级并发。除上述4252精确兼容例外外，load-release要求当前代码HEAD与制品revision一致。

固定发布页包含 `farsail-linux-amd64-images.tar.gz`、`manifest.json`、`SHA256SUMS`。脚本核验附件SHA256、revision、六个image ID和linux/amd64，再更新Compose。依赖源按registry digest锁定；归档使用完整image-ID派生tag，避免save/load不保留RepoDigest。所有服务pull_policy=never，启动显式--pull never。

镜像内容身份以manifest中的 `config_digest` 为准，配置SHA同时绑定rootfs diffIDs。classic/containerd存储的docker inspect .Id可能分别代表config或manifest，不能直接跨机器比较；加载后流式重新导出Docker元数据校验配置SHA，仍不访问镜像仓库，也不保存大型临时tar。manifest的id保留构建引擎观察值供追溯。

```sh
bash scripts/deploy/farsail.sh load-release "$RELEASE_SHA"
```

代码clone成功不代表GitHub附件可下载。附件失败检查DNS、出站443和curl返回；重试可复用已下载文件。校验不通过先移走对应缓存文件。服务器无法下载时，在自己电脑下载同一发布的三个文件，用自己的SSH/scp传入 `/home/data/farsail-release`，然后：

```sh
bash scripts/deploy/farsail.sh load-release "$RELEASE_SHA" /home/data/farsail-release
```

此路径无需访问DockerHub。不得替换来源不明镜像站。SHA256保证完整性，信任根仍是本项目GitHub发布身份。`.local/production/manifest.json`是已核对清单，下载目录可保留供回滚。

## 证书和首次启动

Certbot5.4+ webroot支持 `--ip-address`，使用 `--preferred-profile shortlived`。IP证书约160小时，当前nginx/apache installer不支持IP，须自行部署/续期加载。[Let’s Encrypt官方说明](https://letsencrypt.org/2026/03/11/shorter-certs-certbot)。

先staging演练（隔离目录，不会替换正式证书，也不受系统信任）：

```sh
bash scripts/deploy/farsail.sh certificate staging
```

再正式签发、加载并启动：

```sh
bash scripts/deploy/farsail.sh certificate
```

首次HTTP仅提供ACME目录。正式证书先nginx原位reload，再启动业务；证书/路由错误须处理，不使用-k或跳过验证。

```sh
bash scripts/deploy/farsail.sh install-timer
```

```sh
systemctl start farsail-renew.service
```

```sh
systemctl status farsail-renew.timer farsail-renew.service --no-pager
```

每天两次检查，随机延迟最多30分钟、补跑错过的任务。deploy hook留待加载标记；复制证书、nginx检查/reload、relay重启并健康后才移除标记。失败保留标记和systemd失败状态。查看 `journalctl -u farsail-renew.service` 和私有 `renew-last-success.txt`；应接入自己的运维通知，本包没有外部告警接收账号。当前明确使用Manual+重启relay，续期会断开中继会话，客户端须重新批准。未声称热加载无中断。

```sh
curl --fail --show-error 'https://<公网IPv4>/healthz'
```

```sh
bash scripts/deploy/farsail.sh status
```

## 邮箱、管理员和双端配置

SMTP在共享namespace的127.0.0.1:1025，收件箱只发布到宿主127.0.0.1:8025。电脑另开终端，保持SSH转发：

```sh
ssh -N -L 18025:127.0.0.1:8025 root@<公网IPv4>
```

电脑打开 `http://127.0.0.1:18025`。客户端API为 `https://<公网IPv4>`，relay为 `https://<公网IPv4>:8443/`，UDP绑定改为 `0.0.0.0:0`，首次先勾选强制relay。注册→收件箱读取token→客户端验证→登录→绑定设备。Mailpit不向真实邮箱投递；测试信都由当前运维者可见，正式开放注册前应配置真实SMTP并清理测试账号。

```sh
bash scripts/deploy/farsail.sh bootstrap-admin '<已验证且启用的邮箱>'
```

不是首个注册自动提权。管理员页面可改为邀请注册/关闭注册。被控端还须本机开启共享、逐次批准；检查双端校验码、实际selected path=relay以及停止/撤销。配置relay URL不等于实际走relay。公共发现/自动端口映射默认关闭，跨NAT直连须另验。标称带宽不等于实测吞吐或画质。

真实SMTP：编辑私有coordinator.env，设置 `FARSAIL_MAIL_MODE=smtp-tls`、HOST/USER/PASSWORD/MAIL_FROM（完整名为FARSAIL_SMTP_HOST、FARSAIL_SMTP_USER、FARSAIL_SMTP_PASSWORD、FARSAIL_MAIL_FROM），`FARSAIL_SMTP_TLS=implicit`与PORT=465或`starttls`与PORT=587（PORT完整名FARSAIL_SMTP_PORT）。必需TLS，不降级。遵循Compose env-file转义语法。清空compose.env的COMPOSE_PROFILES，先stop后start，不再启动Mailpit。真实SMTP本次未验。

## 备份、升级、回滚

```sh
bash scripts/deploy/farsail.sh backup
```

私有backups目录生成pg_dump custom-format文件。另自行加密备份整个 `.local/production`（含证书、秘密、清单）和代码ref；备份含账号/邮件数据，不能公开上传。

升级前备份，再检出新固定SHA、load-release 新SHA，执行：

```sh
bash scripts/deploy/farsail.sh start
```

start总是一起force-recreate gateway/coordinator/relay/Mailpit，避免依赖服务仍留在旧network namespace；短暂中断，卷保留。禁止只重建gateway。回滚先检出旧代码和导入旧归档，再start。本项不改变数据库迁移；未来涉及不可逆迁移时必须评估备份兼容性。

恢复数据库：先stop，以同一state的Compose单独启动db，把选定dump经标准输入传给 `pg_restore -U farsail -d farsail --clean --if-exists --exit-on-error`，再start。--clean覆盖业务库，须由操作者核对备份/停机后执行。不得在公网运行开发测试脚本。完整灾备恢复演练尚未验证。

恢复时先停止定时器及正在执行的续期，避免恢复中重启业务：

```sh
systemctl stop farsail-renew.timer farsail-renew.service
```

```sh
bash scripts/deploy/farsail.sh stop
```

```sh
docker compose --env-file .local/production/compose.env -f deploy/production/compose.yaml up -d --pull never --wait db
```

确认选定备份后，下面单条命令持有实例操作锁并覆盖当前数据库内容：

```sh
flock .local/production.operation.lock docker compose --env-file .local/production/compose.env -f deploy/production/compose.yaml exec -T db pg_restore -U farsail -d farsail --clean --if-exists --exit-on-error < '<选定的.dump完整路径>'
```

```sh
bash scripts/deploy/farsail.sh start
```

```sh
systemctl start farsail-renew.timer
```

```sh
bash scripts/deploy/farsail.sh logs
```

```sh
bash scripts/deploy/farsail.sh stop
```

stop仅删除本项目容器/网络、保留数据卷。故障按Compose健康、nginx配置、数据库、准入bearer、IP SAN/完整链、时钟和ACME80路由检查。准入只验证新relay连接，现有应用流由30秒grant截止；限速及未实施的连接限额见 [relay说明](../deploy/relay/README.md)。

## 开发者测试

Windows `scripts/test-deploy.ps1 -Build` 需要Git Bash/openssl/jq。Linux构建两个本地镜像后运行 `bash scripts/deploy/test.sh`。仅操作farsail-deploy-test，回环58080/58443/58444/58026/55433；临时CA正常验证、真实注册/邮件/公钥证明、双向relay数据和selected path、未知/禁用身份/服务停机拒绝、namespace重建与持久化、后端tests/Clippy。测试结束停止容器，保留私有状态和卷。不登录用户服务器。
