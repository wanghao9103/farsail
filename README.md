<p align="center">
  <img src="assets/branding/farsail-v2/128x128.png" width="96" height="96" alt="FarSail icon candidate" />
</p>

# FarSail · 遥舟

正在开发中的开源远程桌面与文件传输工具。账号、设备与授权协调服务使用 Rust；客户端计划使用 Tauri。

**当前状态：WI-001 协调服务已实现并可在本机运行。** 尚无可运行的远程控制客户端、传输链路或安装包。画面、输入、文件传输、跨网络与手机功能仍是待实现目标。

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

服务只监听 `127.0.0.1:8787`，Mailpit UI 为 `http://127.0.0.1:58025`。`GET http://127.0.0.1:8787/healthz` 可检查 PostgreSQL。`./scripts/test-coordinator.ps1 -Stop` 仅停止此项目容器；保留本地数据卷。生产实例需要 TLS 反向代理、有效证书、TLS SMTP、备份和限流，不能通过关闭证书校验接入。功能完成状态以实际实现和 [WI-001 验证记录](docs/verification/WI-001.md) 为准。

## License

[MIT](LICENSE)。引入的第三方依赖仍遵循其各自许可证。
