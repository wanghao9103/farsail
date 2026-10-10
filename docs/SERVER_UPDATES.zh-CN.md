[English](SERVER_UPDATES.md) | **简体中文**

# 协调服务在线升级

安装升级入口后，在服务器执行下面一条命令即可。它自动查找最新的可用协调服务包、获取校验值、验证下载内容，再执行已有备份与升级流程，无需填写下载地址或 SHA256。

```bash
/home/data/farsail/farsail-update
```

只查看可用版本，不下载镜像、不修改部署：

```bash
/home/data/farsail/farsail-update --check
```

此命令只更新协调服务。[客户端更新](CLIENT_UPDATES.zh-CN.md)使用独立的安装与签名流程。实际升级会短暂中断协调服务，已有远程会话可能需要重新连接。

## 首次安装一次入口

旧服务端包中包含 `update-online.sh`，但旧升级流程不会安装常驻命令；临时解压目录删除后，脚本也可能不再保留。旧脚本仍要求地址和校验参数，安装新版客户端不会替换服务器上的这个脚本。

新版已校验包包含 `install-online-updater.sh`。传入并校验该包后，可安装升级入口，此步骤不重启服务：

```bash
bash /tmp/farsail-coordinator-release/install-online-updater.sh \
  /tmp/farsail-coordinator-release /home/data/farsail
```

若部署目录不同，用原目录替换 `/home/data/farsail`。原来使用自定义状态目录时，安装前保留原 `FARSAIL_STATE_DIR`。安装器记录绝对部署目录和状态目录，在状态目录内以私有权限保存工具，并在部署根目录创建 `farsail-update`。之后无需再填写路径或版本参数。使用能访问 Docker、配置及备份的部署用户执行；安装需要部署根目录和状态目录的写权限。

包含新版源码的检出目录也可直接运行：

```bash
bash scripts/deploy/update-online.sh --latest /home/data/farsail
bash scripts/deploy/update-online.sh --latest --check /home/data/farsail
```

**发布状态：**2026-10-10 核对时，公开的服务端包仍是历史 Ubuntu/登录修复包与邮件更新包。Files 包和本升级入口是待审查候选，尚未作为新服务端 Release 公开发布，也没有安装到现网。发布符合新规则的包前，新命令会明确提示没有可用的自动升级包。旧地址与校验参数模式仍可用于明确选择的历史版本，见 [Ubuntu/登录升级](UPGRADE_UBUNTU_LOGIN.zh-CN.md)和[邮件升级](EMAIL_VERIFICATION_UPDATE.zh-CN.md)。

## 如何选择最新服务端包

工具查询 `wanghao9103/farsail` 的公开发布记录，只选择已发布的 `coordinator-*` Release，当前服务端频道兼容普通发布和预览发布。只接受 `FarSail_coordinator_*_linux_amd64.tar.gz` 及其完全同名的 `.sha256` 附件。客户端安装器、客户端更新清单、完整部署包、自动生成的 Source code、草稿和不含新协议的历史包均不进入选择。

按升级包附件的创建时间及附件 ID 排序，因为同一个 Release 可能后来追加新服务包。最新包缺附件、格式错误或校验失败时直接停止，不退回旧包。请求次数和翻页范围均有上限。本仓库混合发布客户端与服务端，当前公开发布均为预览版，因此不适合使用 GitHub 通用 latest 接口，接口行为见 [GitHub Releases API](https://docs.github.com/en/rest/releases/releases)。

一次执行固定所选版本，不会下载途中切换。HTTPS 校验、校验附件格式、API 提供的附件摘要、归档实际摘要、安全解压路径、包内校验与协调服务元数据均须通过，才会运行升级脚本。下载公开升级包不要求 GitHub 登录、个人令牌、SMTP 密码或客户端签名密钥。

新包在 `coordinator-release.json` 中声明 `online_update_protocol: 1` 及镜像实际构建源码的完整 `source_revision`。仅刷新脚本的包可以记录 `package_source_revision`，不能声称未变的镜像已重新构建。发布需提供校验附件和新版升级、安装脚本；仅将旧包改名不能变成兼容的新包。

## 执行升级与重复运行

升级脚本使用与其他运维命令相同的部署操作锁。在锁内复核所选镜像、已安装源码及成功在线升级记录，拒绝会将 Compose 指向其他镜像或部署的环境变量覆盖，并校验导入的 Linux amd64 镜像配置摘要。

如果运行中的协调服务健康、已使用所选不可变镜像，且元数据一致，直接成功返回，不再次导入镜像、备份数据库或重启。相同标签但不健康或摘要不一致时报告问题，不误报已是最新。

真正升级时，先确认旧镜像仍可回退，生成私有 PostgreSQL 备份并检查可读取，保存原镜像选择和版本记录；然后只重建协调服务并等待健康。成功后原子写入版本元数据和在线升级记录，包中包含工具时刷新常驻入口。升级或版本记录发布失败时，恢复旧协调服务选择及对应版本记录，再尝试重建旧协调服务。新增数据库迁移保持，不自动用旧数据库覆盖当前用户数据。若仅后续工具安装失败，协调服务保留已健康提交的版本，安装器恢复原工具入口；诊断会明确区分这一情况。

成功升级记录拒绝已知更旧的发布附件，以及同一附件校验值被更换的情况。已有源码元数据时，固定仓库的源码比较须确认候选相同或更新，锁内基线也须保持一致。两个已知历史协调服务包可建立首次基线。未知旧部署或无法比较的未公开源码历史会停止并给出诊断，需要一次明确校验的迁移；附件时间不能单独证明源码兼容性。

## 验收与排障

成功后查看协调服务，再测试本次涉及的业务功能：

```bash
cd /home/data/farsail
cloud_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
docker compose --env-file "$cloud_state/compose.env" \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file "$cloud_state/compose.env" \
  -f deploy/production/compose.yaml logs --tail 80 coordinator
```

自定义状态目录需使用与常驻入口记录一致的原目录。容器健康证明服务启动，不能证明邮件送达、Files 兼容或公网远控已通过。测试文件传输前，两端客户端也须更新到匹配的 Files 版本。

| 输出                     | 处理                                                   |
| ------------------------ | ------------------------------------------------------ |
| 没有可用的自动升级包     | 发布使用新协议的已验证包，不用客户端或旧邮件包代替。   |
| API 限流、网络或证书失败 | 检查出站网络、DNS、时间或 CA 后重试，不关闭 TLS 验证。 |
| 校验附件缺失或错误       | 停止并修正发布附件；此次没有完成部署升级。             |
| 已有部署操作正在进行     | 等原操作结束，再重试。                                 |
| 已是最新且健康           | 继续业务验收，无需重启。                               |
| 同标签不健康或镜像不一致 | 先检查配置、运行镜像和协调服务日志，再修复。           |
| 源码基线变化或候选更旧   | 核对并发升级和版本选择，不绕过源码及成功记录检查。     |
| 启动或元数据发布失败     | 查看回退结果和脚本输出的准确私有备份路径。             |

本地 HTTPS/版本选择测试，以及模拟 Docker、备份、回退、安装的合同测试覆盖这些失败边界，不代表新包已经公开发布或现网已升级。命令在执行时检查更新，不会安装无人值守的定时任务。
