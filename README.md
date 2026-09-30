<p align="center">
  <img src="assets/branding/farsail-v2/128x128.png" width="96" height="96" alt="FarSail icon candidate" />
</p>

# FarSail · 遥舟

正在开发中的开源远程桌面与文件传输工具。账号、设备与授权协调服务使用 Rust；Windows 客户端使用 Tauri 2、React 和 Rust 原生客户端库。

**当前状态：WI-001 协调服务、WI-002 Windows 客户端、WI-003 认证加密传输及 WI-004 Windows 远程查看/输入基线已在本机验证，WI-008B 提供已验收的 [Windows x64 预览安装包](docs/WINDOWS_INSTALL.md)。** iroh 数据通道支持本地直连及自建 TLS relay；Windows 端可在本机开启共享，默认逐次批准后传送 JPEG 画面并接受鼠标键盘；0.1.2 提供独立原生查看窗口、断线清理和本机显式开启的同账号“远程值守”，详见 [验证记录](docs/verification/WI-REMOTE-012.md)。0.1.3 增加固定桌面工作区、明确的连接操作及计算机名称显示，见 [新版交付记录](docs/verification/WI-UI-RELEASE-013.md)。0.1.4 修复鼠标输入与输入拒绝时保留画面的处理，见 [输入修复记录](docs/verification/WI-INPUT-014.md)。0.1.5 提供默认最大化、自动隐藏工具栏与 720p/1080p 高清切换，见 [窗口与画质记录](docs/verification/WI-VIEWER-015.md)。安装包未签名；文件内容、公网跨 NAT、HEVC与手机端仍待验证或实现。

## 计划能力

- 账号注册与登录、设备绑定、同账号全部可远程设备列表。
- Windows 远程查看与鼠标键盘控制；Android/iPhone 控制 Windows。
- 跨网络加密通信、P2P 直连与中继回退。
- 多屏、分辨率切换、可协商的 4:4:4 画质与自适应带宽。
- H.265/H.264 视频链路，AV1 能力评估，以及适用数据的传输前无损压缩。
- 双向文件传输、独立文件会话、分块校验、断点续传和限速。
- 后续扩展手机被控能力，按平台公开接口分别验证。

## 文档与资产

- [项目设计](PROJECT_DESIGN.md)：功能范围、架构、账号与授权、传输和实施验收。
- [编码全景调研](CODEC_SURVEY.md)：视频/图像格式和选型依据。
- [图标说明](assets/branding/README.md)：候选资源、生成提示词及视觉相似性初筛记录。
- [协调服务 API](docs/API.md)：已实现的账号、设备、邀请、授权状态机和管理员接口。
- [Windows 客户端](docs/CLIENT.md)：运行方式、凭据边界与已实现界面。
- [Windows 预览安装](docs/WINDOWS_INSTALL.md)：无需开发环境的x64安装、校验、WebView2与双机联调。
- [认证传输](docs/TRANSPORT.md)：iroh 握手、短租约、数据通道和路径状态。
- [Windows 远控](docs/REMOTE.md)：DXGI/JPEG、viewer、输入权限与安全停止。
- [自建 relay](deploy/relay/README.md)：HTTPS 证书与公网 IP 配置示例。
- [公网 IP 部署包](docs/DEPLOYMENT.md)：六镜像离线制品、短期 IP 证书、回环 Mailpit 与备份/升级。
- [实施与验收](docs/IMPLEMENTATION.md)：工作项进度及本地验证证据。

图标由内置图像生成工具生成，目前推荐候选为 v2；相似性初筛不代表唯一性或完成商标查重。

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

服务只监听 `127.0.0.1:8787`，Mailpit UI 为 `http://127.0.0.1:58025`。`GET http://127.0.0.1:8787/healthz` 可检查 PostgreSQL。`./scripts/test-coordinator.ps1 -Stop` 仅停止此项目容器；保留本地数据卷。生产实例需要 TLS 反向代理、有效证书、TLS SMTP、备份和限流，不能通过关闭证书校验接入。功能完成状态以 [WI-001](docs/verification/WI-001.md)、[WI-002](docs/verification/WI-002.md)、[WI-003](docs/verification/WI-003.md) 和 [WI-004](docs/verification/WI-004.md) 验证记录为准。

Windows 客户端本地运行（另开终端分别执行）：

```powershell
./scripts/start-local.ps1
npm ci
npm run tauri -w @farsail/desktop -- dev
```

`./scripts/test-remote.ps1 -RealCapture` 可跑前端、真实 PostgreSQL/HTTP/iroh/DXGI 集成测试和 Tauri 编译。日常双端流程是绑定设备→启动传输→被控端本机开启共享→发起授权请求→被控端明确批准→连接并查看。文件内容仍不可用；公网、跨 NAT、第二台 Windows 及手机端没有本项证据。

## License

[MIT](LICENSE)。引入的第三方依赖仍遵循其各自许可证。
