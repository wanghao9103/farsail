# Windows x64 预览安装与双机联调

FarSail 0.1.7 是支持自动适配与 720p/1080p/2K/4K 档位的 JPEG 查看/鼠标键盘控制预览。远程窗口默认最大化，使用与客户端一致的深色标题栏，全屏时隐藏；工具栏自动隐藏，移到顶部或点击显示入口可展开，并可固定显示。画面保持比例、不放大源显示器、不修改远端系统分辨率。需要 Windows 10/11 x64 的普通交互桌面；不需要安装 Rust、Node 或开发工具。文件传输、HEVC、自适应视频码率和手机端尚未交付。安全桌面、UAC 和无人登录桌面不支持。服务器真实 CA、跨 NAT 和两台家用电脑的效果须另行验收。

## 下载与安装

当前固定预发布：[下载安装包](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.7-d7753f2/FarSail_0.1.7_x64-setup.exe)、[SHA256SUMS.txt](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.7-d7753f2/SHA256SUMS.txt)，其余元数据见 [发布页](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.7-d7753f2)。源码 SHA、安装包 SHA256 和安装验证证据见 [VIEWER-017](verification/WI-VIEWER-017.md)。同时下载 `FarSail_0.1.7_x64-setup.exe`、`SHA256SUMS.txt`、`release-metadata.json`。PowerShell 在下载目录执行：

```powershell
$expected = ((Get-Content ./SHA256SUMS.txt -Raw).Trim() -split '\s+')[0]
$actual = (Get-FileHash ./FarSail_0.1.7_x64-setup.exe -Algorithm SHA256).Hash
if ($actual -ine $expected) { throw 'SHA256 mismatch: do not install' }
```

双击安装，选择简体中文或英语。安装包**未进行 Authenticode 签名**，Windows 可能显示未知发布者或 SmartScreen 提示；摘要证明下载一致性，不等于受信任签名。不要关闭系统防护。组织策略阻止未签名程序时，应按组织流程处理。

安装采用当前用户模式，默认用户本地应用目录，不安装系统服务、不设置开机共享。包内包含 Microsoft WebView2 Evergreen 引导器；若电脑没有 WebView2，需要联网下载并安装运行时，完成后再启动。已有兼容运行时时复用系统运行时。此包不是离线 WebView2 全量安装包。官方机制见 [Tauri Windows 安装文档](https://v2.tauri.app/distribute/windows-installer/)。

## 两台 Windows 首次连接

1. 两台电脑安装同一预览。首次启动处于未登录、未共享状态。在“设置”填入自己的有效 HTTPS API 地址，例如 `https://<公网IPv4>`，点“保存地址”。不要把 relay 的8443端口填进 API 字段；不提供跳过证书校验选项。
2. 服务器先按 [部署说明](DEPLOYMENT.md) 成功启动。若使用首次联调 Mailpit，在有 SSH 权限的电脑保留以下转发终端，然后浏览器打开 `http://127.0.0.1:18025`：

   ```sh
   ssh -N -L 18025:127.0.0.1:8025 <SSH用户>@<公网IPv4>
   ```

   在客户端注册，读取对应测试邮件的验证令牌，在“验证邮箱”提交，然后正常登录。测试邮箱不会发到真实邮箱，不要把收件箱端口暴露公网。已配置正式 TLS SMTP 时直接到自己的邮箱收信。第二台电脑可使用同一已验证账号登录，无需重复注册。
3. 两台电脑登录后均添加本机，默认读取 Windows 计算机名称，也可自行修改。旧的“这台 Windows 电脑”由每台新版客户端各自更新，手动名称保留。连接服务会在开启共享或连接时自动准备；管理员要求修改网络参数时，展开高级连接设置。本机 UDP 监听保持 `0.0.0.0:0`，中继填 `https://<公网IPv4>:8443/`，默认不勾选“始终通过中继连接”，优先尝试 P2P，直连不可用时回退中继；仅排查直连问题时勾选强制中继。传输配置为当前运行期间设置。
4. 被控电脑点“开启本机共享”，等待显示已共享。控制电脑在“我的设备”刷新，向在线目标发起查看或控制请求。默认由目标端核对申请账号、设备和权限后逐次批准；管理员身份不能代替批准。若希望同账号直接连接，在被控端“我的设备”的本机面板显式开启“远程值守”。此开关默认关闭，停止共享、退出或重启后关闭；关闭值守会结束入站连接并恢复手动批准。
5. 设备面板的请求获批后自动打开独立远程窗口；“远程连接”页也可根据批准权限点“连接并查看”或“连接并控制”。核对窗口实际链路为 P2P 或中继，在折叠的连接详情比较双方校验码；单纯填写 relay URL 不证明实际走中继。窗口显示实际接收 FPS、网络 RTT 和显示器选择。点击远端画面后获得键盘焦点，可使用最大化、全屏；关闭查看窗口会结束该会话并保留主设备管理窗口。先查看，再在获批 control 会话中对自己准备的无敏感内容窗口测试鼠标键盘。
6. 任一端停止会话；被控端关闭本机共享，确认画面与控制停止。关闭窗口或退出也会先停止本地采集和输入。不要把真实桌面截图、验证令牌或账号密码作为公开问题附件。

画质默认“自动适配”，按窗口物理像素和 Windows 缩放选择档位；也可手动切换。2K/4K 需要两端升级且源显示器有相应像素，当前实际接收尺寸见“更多操作”。帧率按近四秒接收间隔统计，静止画面定期保活，没有更新时明确提示。采集循环约 15 FPS 上限不包含编码与网络开销，不保证实际达到该值。两端无画面时先检查账号/设备在线、目标已共享与已批准，再检查有效证书和两端 relay 配置。网络 RTT 不等于画面延迟；JPEG 基线不代表后续视频性能。

## 升级、卸载与凭据

两端均升级到 0.1.7，尤其被控端，以使用 2K/4K 档位和静止画面保活。旧端仅有 720p/1080p 档位。已部署的 4252 服务端兼容此客户端，无需重新部署、初始化或重置密钥。先关闭 FarSail 再安装新版本。应用标识维持 `app.farsail.desktop`；原生配置和 DPAPI 加密凭据保存在当前 Windows 用户的 `%LOCALAPPDATA%\app.farsail.desktop`。重装和默认卸载保留该目录。卸载界面若显示删除应用数据选项，请保持未勾选以保留凭据。若希望撤销登录，先在应用内退出；卸载本身不等于服务端撤销会话。不要把该目录复制给其他人或上传。

## 开发者复现

干净的固定提交上使用 Windows x64 MSVC Rust 1.93 和 Node 22+：

```powershell
pwsh -File scripts/package-windows.ps1
```

脚本用锁文件安装、类型检查、生产前端构建、Tauri release/NSIS 打包，检查 AMD64/GUI PE、debug探针缺席、v2图标和摘要。输出在忽略的 `.local/windows-package/artifacts`。安装测试 `scripts/test-windows-package.ps1` 限制为一次性 GitHub-hosted Windows runner，避免本机用户配置/注册表被覆盖。工作流手动在固定 ref 执行，验证通过后才发布该 run 的附件；不把本机另一份非同摘要构建替换进去。
