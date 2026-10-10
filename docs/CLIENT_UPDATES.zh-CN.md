[English](CLIENT_UPDATES.md) | **简体中文**

# 客户端构建与自动更新

## 用户安装什么

GitHub Actions 构建 Windows x64 NSIS 安装包，以及 Ubuntu x64、ARM64 Debian 包。发布成功后，三种安装包都会放到对应版本的客户端 Release；Actions 附件只是临时构建产物，不是更新通道。旧客户端没有更新器，需要手动安装一次首个支持更新的版本。服务端升级包不会升级桌面应用。

“共享与设置”的“软件更新”显示当前版本、检查结果和下载进度。默认开启自动检查：正式安装版在启动后稍等片刻检查，之后每小时检查一次。自动下载与安装可以选择开启；有远程会话或查看窗口时，会等待结束后再安装并重启。Ubuntu 安装可能需要系统授权，Windows 沿用当前用户安装方式。设置会保留到下次启动。开发版不自动检查公网更新通道。

更新通道为 [client-updates](https://github.com/wanghao9103/farsail/releases/tag/client-updates)，清单附件是 `latest.json`。2026-10-10 已公开签名版本 0.1.20 和更新清单，包含 Windows x64 与 Ubuntu x64；ARM64 是 0.1.21 源码候选新增目标，尚未公开签名发布。写好功能和工作流不等于已经公开安装包。Ubuntu 24.04 是 x64/ARM64 的 CI 构建基线，本地 x64 Ubuntu 26.04 构建不能代替 ARM64 验证。

ARM64 系统名为 `aarch64`，Debian 包名后缀为 `arm64.deb`，清单目标是 `linux-aarch64-deb`。x64 保留 `linux-x86_64-deb`，Windows 保留 `windows-x86_64-nsis`。客户端按本机构建架构精确选择条目，并校验下载包名；ARM64 遇到缺少 ARM64 条目的旧清单时，会显示没有兼容更新，不退到 amd64。安装和架构查询见[Ubuntu 客户端](UBUNTU_INSTALL.zh-CN.md)。

原生 ARM64 CI 使用 GitHub 官方[托管 runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)，不把 x64 构建或 QEMU 可用当作 ARM 原生运行证据。

## 维护者配置

应用内只保存更新公钥。将对应私钥保存为 GitHub Actions 仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`；工作流将 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 设置为空字符串，以适配当前无密码签名密钥。请在仓库外安全备份私钥，不要提交、作为发布附件、写入日志或文档。丢失该密钥后，现有安装将无法接受新签名；不能直接用另一把密钥替换已安装客户端信任的密钥。

2026-10-10 已通过名称列表核对仓库存在该 Secret，并完成 0.1.20 云端签名发布；新维护环境应恢复匹配密钥，不要重新生成替代公钥。更新签名用于验证包内容和版本，与 Windows Authenticode 发布者证书是两件事，不会因此消除 SmartScreen 提示。

维护者不需要手动粘贴私钥：初始化脚本先检查本地私钥是否与应用内公钥匹配，再通过已经登录的 GitHub CLI 保存 Secret。值通过标准输入传递，不出现在命令参数或日志中；GitHub CLI 会在本地加密后上传。普通客户端用户始终无需配置签名 Secret。不带应用参数执行脚本时只做本地检查；下面的应用命令需要具有该仓库 Secrets 写入权限的账号。更换维护电脑时应恢复备份中匹配的私钥，不要重新生成替代密钥。本仓库已完成初始化；普通客户端用户不需要执行以下维护者命令。

```bash
gh auth login --hostname github.com
node scripts/configure-client-updates.mjs --apply
```

## 自动发布流程

修改 `apps/desktop/src-tauri/tauri.conf.json` 中的客户端版本，合入评审后的源码，再推送对应版本标签。下面是当前候选版本的示例，需在评审及验证完成后执行，不表示该版本已经发布。

```bash
git tag client-v0.1.21
git push origin client-v0.1.21
```

也可以在 GitHub 网页上发布对应版本的客户端 Release，或在 Run workflow 中选择源码分支并填写对应版本标签。Run workflow 默认只构建签名候选；只有显式选择 `publish_release` 才公开版本并推进更新通道。标签及客户端 Release 事件仍按发布流程执行，维护者无需本地构建工具链。

`Client installers and signed updates` 工作流会自动构建三种架构目标。Windows 验证安装后的程序摘要、原生设置、重启、共享偏好、重装和卸载。Ubuntu 分别使用 `ubuntu-24.04` 和 `ubuntu-24.04-arm` 原生 runner，检查 WebKitGTK IPC、系统密钥环、包元数据、实际 ELF 架构和运行依赖；保持通用 ARM64 指令目标。上传前验证三份安装包的签名、签名中的版本、源码提交、摘要、Linux 架构及 Windows 安装报告。PR 构建不读取生产签名私钥，只提供预览附件。

版本 Release 保留不可覆盖的安装包、签名、校验文件和验证元数据。每个附件匿名公开下载后必须与已经测试的字节完全一致，才推进更新通道。通道不能倒退到更旧或相同版本，三种目标缺一不可：构建失败、缺少签名、安装验证失败都会保留旧通道。更新通道先上传待替换清单，再替换已有附件；替换期间短暂检查失败时，客户端保留当前安装。

## 验证与边界

```bash
node --test scripts/test-client-update-manifest.mjs
cargo test --locked -p farsail-desktop --lib
```

清单回归使用一次性测试签名密钥，拒绝包内容被修改、签名注释被篡改、清单版本错误、混用源码提交、缺少签名和缺少安装报告。界面夹具验证设置修改、下载失败后重试以及等待远程连接结束。这些检查与真实已安装客户端的下载、系统安装授权和更新后重启不同。公开 0.1.20 的两端签名与匿名下载已验证，真实客户端跨版本安装/重启仍需验收；新增 ARM64 候选也需原生 CI 和后续公开签名发布分别留证据。本文不声明 ARM64 安装包已经公开发布。
