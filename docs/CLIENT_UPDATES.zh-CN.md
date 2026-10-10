[English](CLIENT_UPDATES.md) | **简体中文**

# 客户端构建与自动更新

## 用户安装什么

GitHub Actions 构建 Windows x64 NSIS 安装包和 Ubuntu x64 Debian 包。发布成功后，两端安装包都会放到对应版本的客户端 Release；Actions 附件只是临时构建产物，不是更新通道。旧客户端没有更新器，需要手动安装一次首个支持更新的版本。服务端升级包不会升级桌面应用。

“共享与设置”的“软件更新”显示当前版本、检查结果和下载进度。默认开启自动检查：正式安装版在启动后稍等片刻检查，之后每小时检查一次。自动下载与安装可以选择开启；有远程会话或查看窗口时，会等待结束后再安装并重启。Ubuntu 安装可能需要系统授权，Windows 沿用当前用户安装方式。设置会保留到下次启动。开发版不自动检查公网更新通道。

更新通道为 [client-updates](https://github.com/wanghao9103/farsail/releases/tag/client-updates)，清单附件是 `latest.json`。这些地址只有首次签名发布成功后才可用；写好功能和工作流不等于已经公开发布安装包。Ubuntu 24.04 是 CI 构建基线，本地 Ubuntu 26.04 构建的包不能代替这一兼容性验证。

## 维护者配置

应用内只保存更新公钥。将对应私钥保存为 GitHub Actions 仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`；工作流将 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 设置为空字符串，以适配当前无密码签名密钥。请在仓库外安全备份私钥，不要提交、作为发布附件、写入日志或文档。丢失该密钥后，现有安装将无法接受新签名；不能直接用另一把密钥替换已安装客户端信任的密钥。

新生成的密钥目前保存在本地被忽略的私有目录中，尚未配置为仓库 Secret 或上传。请先配置再创建发布标签。更新签名用于验证包内容和版本，与 Windows Authenticode 发布者证书是两件事，不会因此消除 SmartScreen 提示。

维护者不需要手动粘贴私钥：初始化脚本先检查本地私钥是否与应用内公钥匹配，再通过已经登录的 GitHub CLI 保存 Secret。值通过标准输入传递，不出现在命令参数或日志中；GitHub CLI 会在本地加密后上传。普通客户端用户始终无需配置签名 Secret。不带应用参数执行脚本时只做本地检查；下面的应用命令需要具有该仓库 Secrets 写入权限的账号。更换维护电脑时应恢复备份中匹配的私钥，不要重新生成替代密钥。初始化已经完成本地检查，但尚未实际写入仓库 Secret。

```bash
gh auth login --hostname github.com
node scripts/configure-client-updates.mjs --apply
```

## 自动发布流程

修改 `apps/desktop/src-tauri/tauri.conf.json` 中的客户端版本，合入评审后的源码，再推送对应版本标签。下面是首个候选版本的示例，不表示它已经发布。

```bash
git tag client-v0.1.20
git push origin client-v0.1.20
```

也可以在 GitHub 网页上发布对应版本的客户端 Release，或在 Run workflow 中选择源码分支并填写对应版本标签。工作流会自动构建并发布，维护者无需在本地安装构建工具链或从本机终端推送标签。

`Client installers and signed updates` 工作流会自动构建两端。Windows 验证安装后的程序摘要、原生设置、重启、共享偏好、重装和卸载。Ubuntu 在构建基线上检查原生 WebKitGTK IPC、系统密钥环、包元数据和运行依赖。上传前验证两份安装包的签名、签名中的版本、源码提交、摘要及 Windows 安装报告。PR 构建不读取生产签名私钥，只提供预览附件。

版本 Release 保留不可覆盖的安装包、签名、校验文件和验证元数据。每个附件匿名公开下载后必须与已经测试的字节完全一致，才推进更新通道。通道不能倒退到更旧或相同版本，两端缺一不可：构建失败、缺少签名、安装验证失败都会保留旧通道。更新通道先上传待替换清单，再替换已有附件；替换期间短暂检查失败时，客户端保留当前安装。

## 验证与边界

```bash
node --test scripts/test-client-update-manifest.mjs
cargo test --locked -p farsail-desktop --lib
```

清单回归使用一次性测试签名密钥，拒绝包内容被修改、签名注释被篡改、清单版本错误、混用源码提交、缺少签名和缺少安装报告。界面夹具验证设置修改、下载失败后重试以及等待远程连接结束。这些检查与真实已安装客户端的下载、系统安装授权和更新后重启不同。配置仓库 Secret 后仍需验证公开签名发布及一次真实跨版本升级。本文不声明新版更新客户端已经公开发布。
