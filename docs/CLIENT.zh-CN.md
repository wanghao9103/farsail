[English](CLIENT.md) | **简体中文**

<a id="farsail-windows-client-wi-002003004"></a>

# FarSail Windows 客户端（WI-002/003/004）

自动构建与客户端更新的设置、签名及发布流程见[客户端自动更新](CLIENT_UPDATES.zh-CN.md)。

Ubuntu 控制端适配、源码运行、Debian 打包与 Secret Service 边界见 [Ubuntu 桌面客户端](UBUNTU_INSTALL.zh-CN.md)。Linux 本机屏幕共享暂不可用。

0.1.10 的本机共享与远程值守偏好见 [SHARE-020](verification/WI-SHARE-020.zh-CN.md)。DPAPI 只保存这两项已成功开启的选择及服务/账号/设备/登录会话范围，不保存高级连接参数。原生启动仅尝试恢复一次，先重验登录、心跳、传输和桌面，再发布共享能力；界面分别显示记住的意图和有效活动。手动关闭、退出登录、身份变化会取消迟到恢复；普通关窗或故障停止活动而保留有效偏好。首次默认关闭，仍需 Windows 已登录且应用运行，不提供服务、自启动或安全桌面支持。

Windows 客户端位于 `apps/desktop`，React/Vite 页面调用 Tauri 2 命令，账号、设备和授权 HTTP 请求由 `crates/client` 发出。服务协议见 [API.md](API.zh-CN.md)，传输见 [TRANSPORT.md](TRANSPORT.zh-CN.md)，画面与输入细节见 [REMOTE.md](REMOTE.zh-CN.md)。Windows 远程查看与鼠标键盘基线已接入；文件内容仍属 WI-005。

无需开发环境的 Windows x64 预览安装、WebView2、未签名说明及第二台电脑联调步骤见 [WINDOWS_INSTALL.md](WINDOWS_INSTALL.zh-CN.md)；固定下载与实际验证结果见 [WI-008B](verification/WI-008B.zh-CN.md)。

<a id="running-locally"></a>

## 本地运行

需要 Windows、Rust 1.93、Node/npm 和 Docker。服务端示例只绑定回环，测试邮件只进入本地 Mailpit。

```powershell
./scripts/start-local.ps1
# 另开终端
npm ci
npm run tauri -w @farsail/desktop -- dev
```

或者运行 `./scripts/test-remote.ps1 -RealCapture`：启动项目专属的 `farsail-dev` PostgreSQL/Mailpit，运行后端、传输、客户端和 Windows 合成测试，再以本机真实显示器在内存中完成采集→JPEG→认证连接→解码；最后执行 Clippy、前端构建和 Tauri 原生构建。真实系统输入是独立的忽略测试，只有确认程序自己创建的窗口位于前台才会注入无害内容。不会发送真实邮件。服务地址默认为 `http://127.0.0.1:8787`，可在退出登录后的设置页修改。Mailpit UI 在 `http://127.0.0.1:58025`。结束后以 `./scripts/test-coordinator.ps1 -Stop` 停止本项目容器；开发数据库卷仍保留。运行中的 `start-local.ps1` 窗口用 Ctrl+C 结束协调服务。

<a id="usage"></a>

## 使用流程

1. 注册账号，去 Mailpit 读取本地验证令牌，在验证邮箱页提交；真实部署须配置 TLS SMTP。
2. 登录后在总览中确认绑定本机。Rust 生成 Ed25519 密钥，签署服务挑战，初始声明 `can_host=false`、`can_files=false`。绑定后启动心跳；退出登录或切换账号会清掉旧的登录与设备凭据、停止旧身份心跳。
3. “我的设备”逐页读取完整同账号设备，包含离线和管理员停用项，可搜索、按能力筛选、改名和解绑。“账号安全”可改密、退出及撤销其他登录会话。
4. “设置”启动安全传输并开启本机共享后，Windows DXGI 探测成功才声明 `can_host=true`。同账号另一客户端刷新设备列表即可选择在线主机；目标端仍须逐次明确批准查看或控制，批准窗口显示申请邮箱及设备名。发起端点“连接并查看”，viewer 收到二进制 JPEG，显示显示器选择、实际接收 FPS、路径 RTT 和双方可比较校验码。`control` 允许鼠标键盘，`view` 仅看画面；双方都能本地立即停止。文件功能尚不可用。
5. 管理员账号可在同一客户端查看用户、登录会话和审计元数据，启停账号/设备、调整注册策略、创建和撤销注册邀请。管理员无法代替目标设备批准远控。

<a id="credential-boundaries"></a>

## 凭据边界

- `crates/client` 的串行状态锁负责访问 token 刷新轮换、HTTP 和设备签名；WebView 不获得访问/刷新 token、设备 token、设备私钥或 grant token。前端仅暂时持有用户输入的密码、邮箱验证令牌及一次性邀请码，不使用 `localStorage`。
- 登录会话、设备凭据、每个服务地址与账号对应的设备私钥分别存放在 Windows 用户作用域的 DPAPI 加密文件中，目录由 Tauri 的 `app_local_data_dir` 提供。iroh 使用这同一把设备私钥。退出先取消本地会话，再删除登录和设备凭据；保留私钥以便同一账号下次重新证明设备身份。移动端需另实现平台安全存储适配。
- 公网服务地址只能使用证书验证的 HTTPS，包含 HTTPS IP；显式回环地址才允许 HTTP。请求不跟随重定向，响应单次上限 1 MiB。不存在跳过证书验证的开关。
- 登录撤销和设备 token 失效由服务端决定。网络离线时退出仍删除本机凭据，同时报告远端撤销未确认。下次登录须重新签名绑定设备。授权 grant 留在原生内存，通过设备认证的 iroh 握手交给正确的发起端，页面不接触该凭据。原生层最多处理 8 个同时握手、16 个活跃会话；退出或重启传输后的旧启动/旧连接结果不会写回。
- 同一配置目录有排他锁，第二个桌面进程不能并发读取并轮换同一刷新令牌。debug IPC 测试可以指定独立的 `FARSAIL_TEST_PROFILE_DIR`。本机共享关闭、退出或窗口关闭先停采集和输入，服务端更新/撤销随后尽力执行；租约仍是兜底边界。

<a id="known-boundaries-and-wi-005006-interfaces"></a>

## 已知边界与 WI-005/006 接口

JPEG 低帧率链路是当前可用基线。系统安全桌面、UAC、无人登录、跨 NAT/公网及第二台 Windows 尚未验证。视频编码、4:4:4 和带宽自适应属于 WI-006。网页预览只能检查布局，无法代表 Windows 原生采集、输入或 IPC。

原生接口 `NativeClient::start_transport`、`connect_transport`、`transport_session` 和 `farsail_transport::Session::send/receive` 仍供文件工作项复用。`Media`、`Control`、`File` 有独立权限和尺寸边界。WI-005 实现分块、校验、续传及限速前继续保持 `can_files=false`。本机双端验证直连及强制 TLS relay，但没有公网 TLS 代理、第二台电脑、跨 NAT 穿透率或手机真机证据。
