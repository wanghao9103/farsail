[English](README.md) | **简体中文**

<p align="center">
  <img src="assets/branding/farsail-v2/128x128.png" width="96" height="96" alt="FarSail icon candidate" />
</p>

<a id="farsail"></a>

# FarSail · 遥舟

FarSail 是正在开发中的开源远程桌面项目，Windows 与 Ubuntu 客户端采用 Rust、Tauri 2 和 React，支持加密的 P2P 连接及自建中继。当前源码候选新增双向单文件传输，旧版已发布安装包不会自动获得该功能；手机端仍属计划能力。

账号、设备与授权协调服务使用 Rust。**WI-001 协调服务、WI-002 Windows 客户端、WI-003 认证加密传输及 WI-004 Windows 远程查看/输入基线已在本机验证；WI-008B 提供已验收的 [Windows x64 预览安装包](docs/WINDOWS_INSTALL.zh-CN.md)。** iroh 数据通道支持本地直连及自建 TLS relay。Windows 被控端可在本机开启共享，默认逐次批准后传送 JPEG 画面并接受鼠标键盘。安装包未签名；公网跨 NAT、物理 Windows 文件传输、HEVC 和手机端仍待验证或实现。当前候选使用方式与具体验证边界见[文件传输](docs/FILES.zh-CN.md)。

<a id="documented-preview-milestones"></a>

## 已记录的预览版本进展

- 0.1.2 增加独立原生查看窗口、断线清理和被控端本机显式开启的同账号“远程值守”。见 [验证记录](docs/verification/WI-REMOTE-012.zh-CN.md)。
- 0.1.3 增加固定桌面工作区、明确的连接操作及计算机名称显示。见 [交付记录](docs/verification/WI-UI-RELEASE-013.zh-CN.md)。
- 0.1.4 修复鼠标输入，并在 Windows 拒绝输入时保留画面连接。见 [输入修复](docs/verification/WI-INPUT-014.zh-CN.md)。
- 0.1.5 提供默认最大化、自动隐藏工具栏与 720p/1080p 高清切换。见 [窗口与画质记录](docs/verification/WI-VIEWER-015.zh-CN.md)。
- 0.1.6 将远程画面光标改为普通箭头。见 [光标更新](docs/verification/WI-INPUT-016.zh-CN.md)。
- 0.1.7 增加统一深色远程标题栏、按窗口自动适配、2K/4K JPEG 档位和静止画面保活，帧率显示真实接收更新；完整功能需要两端升级。下载入口见上方安装说明，验证见 [VIEWER-017](docs/verification/WI-VIEWER-017.zh-CN.md)。
- 0.1.8 默认将完整远程画面铺满可用区域，消除比例不同时的留边；“更多操作”可切换保持比例，选择会保留，输入坐标随显示方式同步。见 [VIEWER-018](docs/verification/WI-VIEWER-018.zh-CN.md)。
- 0.1.9 在新连接前主动刷新地址发现，中继会话定时、有界重试直连；刷新保留原端点、批准和画面，地址变化及时上报。建议两端升级，现有服务端兼容。下载与真实 QAD 失败→恢复、不断流及制品证据见 [P2P-019](docs/verification/WI-P2P-019.zh-CN.md)。实际跨 NAT 升级仍需双机验收，不保证所有网络都能直连。
- 0.1.10 记住本机共享与远程值守选择：普通重启后重新检查登录、设备、连接和交互桌面，通过后恢复；手动关闭后下次保持关闭，退出登录清除开启偏好。高级连接设置仍只在本次运行有效。见 [SHARE-020](docs/verification/WI-SHARE-020.zh-CN.md)。
- 0.1.11 修正旧布局下鼠标抬起丢失，增加指针捕获与失焦/拖出清理，并包含地址刷新和共享/值守偏好保存。连续点击与自建普通窗口焦点切换通过，交付证据与下载见 [INPUT-021](docs/verification/WI-INPUT-021.zh-CN.md)。

以上是已记录的版本进展。本地候选工作和验收边界保留在验证文档中，本地安装包构建成功不代表公开发布。

<a id="planned-capabilities"></a>

## 计划能力

- 账号注册与登录、设备绑定、同账号全部可远程设备列表。
- Windows 远程查看与鼠标键盘控制；Android/iPhone 控制 Windows。
- 跨网络加密通信、P2P 直连与中继回退。
- 多屏、分辨率切换、可协商的 4:4:4 画质与自适应带宽。
- H.265/H.264 视频链路、AV1 能力评估，以及适用数据的传输前无损压缩。
- 当前单文件基线之外的断点续传、选择性压缩和可配置文件/媒体共同带宽控制。
- 后续扩展手机被控能力，按平台公开接口分别验证。

<a id="documentation-and-assets"></a>

## 文档与资产

文档默认使用英文，可通过每页顶部的语言链接切换简体中文。文件约定和检查方法见 [文档语言](docs/LANGUAGES.zh-CN.md)。

- [项目设计](PROJECT_DESIGN.zh-CN.md)：功能范围、架构、账号与授权、传输和实施验收。
- [编码全景调研](CODEC_SURVEY.zh-CN.md)：视频/图像格式和选型依据。
- [图标说明](assets/branding/README.zh-CN.md)：候选资源、生成提示词及视觉相似性初筛记录。
- [协调服务 API](docs/API.zh-CN.md)：已实现的账号、设备、邀请、授权状态机和管理员接口。
- [Ubuntu 桌面客户端](docs/UBUNTU_INSTALL.zh-CN.md)：源码启动、Debian 打包、桌面密钥环和控制端范围。
- [Windows 客户端](docs/CLIENT.zh-CN.md)：运行方式、凭据边界与已实现界面。
- [Windows 预览安装](docs/WINDOWS_INSTALL.zh-CN.md)：无需开发环境的 x64 安装、校验、WebView2 与双机联调。
- [认证传输](docs/TRANSPORT.zh-CN.md)：iroh 握手、短租约、数据通道和路径状态。
- [Windows 远控](docs/REMOTE.zh-CN.md)：DXGI/JPEG、viewer、输入权限与安全停止。
- [桌面文件传输](docs/FILES.zh-CN.md)：独立批准、原生选文件、安全保存、进度与限制。
- [自建 relay](deploy/relay/README.zh-CN.md)：HTTPS 证书与公网 IP 配置示例。
- [公网 IP 部署包](docs/DEPLOYMENT.zh-CN.md)：六镜像离线制品、短期 IP 证书、回环 Mailpit 与备份/升级。
- [协调服务在线升级](docs/SERVER_UPDATES.zh-CN.md)：常驻命令、自动选择服务端包、校验、备份与回退。
- [实施与验收](docs/IMPLEMENTATION.zh-CN.md)：工作项进度与本地验证证据。

图标由内置图像生成工具生成，目前推荐候选为 v2；相似性初筛不代表唯一性或完成商标查重。

<a id="development-and-contributions"></a>

## 开发与提交

本机需要 Rust 1.93 和 Docker。`scripts/test-coordinator.ps1` 会生成忽略的 `.local/dev.env`，仅启动 `farsail-dev` PostgreSQL 与 Mailpit，然后运行 Rust 测试。服务启动示例（PowerShell）：

```powershell
./scripts/test-coordinator.ps1
$password = ((Get-Content .local/dev.env) -split '=',2)[1]
$env:FARSAIL_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
$env:FARSAIL_MAIL_MODE = 'smtp-local'
$env:FARSAIL_SMTP_PORT = '51025'
cargo run -p farsail-coordinator
```

服务只监听 `127.0.0.1:8787`，Mailpit UI 为 `http://127.0.0.1:58025`。`GET http://127.0.0.1:8787/healthz` 可检查 PostgreSQL。`./scripts/test-coordinator.ps1 -Stop` 仅停止此项目容器，保留本地数据卷。生产实例需要 TLS 反向代理、有效证书、TLS SMTP、备份和限流，不能通过关闭证书校验接入。功能完成状态以 [WI-001](docs/verification/WI-001.zh-CN.md)、[WI-002](docs/verification/WI-002.zh-CN.md)、[WI-003](docs/verification/WI-003.zh-CN.md) 和 [WI-004](docs/verification/WI-004.zh-CN.md) 验证记录为准。

Windows 客户端本地运行，另开终端分别执行：

```powershell
./scripts/start-local.ps1
npm ci
npm run tauri -w @farsail/desktop -- dev
```

`./scripts/test-remote.ps1 -RealCapture` 可跑前端、真实 PostgreSQL/HTTP/iroh/DXGI 集成测试和 Tauri 编译。日常双端流程是绑定设备→启动传输→被控端本机开启共享→发起授权请求→被控端明确批准→连接并查看。独立文件流程见[文件传输](docs/FILES.zh-CN.md)，需要两端和协调服务使用新源码候选，旧版已发布安装包不能直接使用；公网、跨 NAT、第二台 Windows 及手机端没有本项证据。

<a id="license"></a>

## 许可证

[MIT](LICENSE)。引入的第三方依赖仍遵循各自许可证。
