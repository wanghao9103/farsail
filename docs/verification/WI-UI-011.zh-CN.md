[English](WI-UI-011.md) | **简体中文**

<a id="windows-preview-011--device-workspace-and-onboarding"></a>

# Windows 预览版 0.1.1 — 设备工作区与初始设置

<a id="scope"></a>

## 范围

- 深色设备工作区采用列表/详情双窗格，本地与远程操作明确区分。
- 设备详情面板提供本地共享和授权请求；远程查看/控制可以请求批准并连接，无需请求表单。
- 注册后进入邮箱验证，再进入登录。登录失败时显示中文模态框，不声称知道哪项凭据有误。
- 紧凑的自定义标题栏；原生最小化/最大化/关闭权限明确限定于主窗口。
- 默认建议的中继地址使用所配置 HTTPS 服务器的主机名和 8443 端口；自定义部署可在设置中修改。
- 排除文件传输的进行中工作。保留现有 JPEG 采集和授权要求；没有无人值守模式。

<a id="local-verification"></a>

## 本地验证

- `npm run build` 通过（TypeScript 和 Vite）。
- `scripts/test-desktop-ui.cjs` 使用 Playwright/Chrome 针对本地 Vite 服务器通过测试，仅使用模拟 IPC：注册/验证/登录、无效验证仍停留在表单、401 模态框、仅本地可用的共享控件、离线保护、取消请求、查看批准恰好触发一次连接、窄视口没有横向溢出。
- 已检查 `.local/ui-verification/` 中的截图：本地设备、远程设备、窄布局。仅使用模拟账号。
- 浏览器测试夹具不能证明真实对等连接或原生标题栏行为。发布前，发布 CI 必须通过实际 Windows 安装和 UI Automation 检查。

<a id="release-gate"></a>

## 发布门禁

使用 `.github/workflows/windows-release.yml`；`scripts/test-windows-package.ps1` 现在检查自定义最大化/最小化和关闭、原生设置持久化、重装和卸载后的保留情况。只发布哈希值出现在通过的安装报告中的那个产物。完成后记录源码和公开资产证据。

<a id="published-evidence"></a>

## 已发布证据

- 源码：`9ad61751506d44837e3e5d8df271914ef9ce7222`。
- Windows 安装程序 CI `36511486164`：成功，包括原生自定义最大化/最小化/关闭、安装、设置 IPC/重启、重装/卸载保留、Rust 测试和 Clippy。
- Windows 客户端 CI `36511468941`：成功。
- 发布版本：https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.1-9ad6175
- 安装程序：`FarSail_0.1.1_x64-setup.exe`，7,744,845 字节，SHA-256 `a79e7dbde64d3f4aec0dfad7861f1562413d26a0448f5df1512d0584f73ab3da`。
- 全部四个公开附件均已匿名下载，其 SHA-256 与 CI 产物逐字节一致。软件包仍未签名。本次发布不声称完成了新的真实对等采集/输入验证。
- 现有文件传输的进行中工作仍未暂存于共享工作区，未包含在本次发布中。此次客户端发布无需更新现有生产服务器镜像。
