# FarSail Windows 客户端（WI-002/003）

Windows 客户端位于 `apps/desktop`，React/Vite 页面调用 Tauri 2 命令，账号、设备和授权 HTTP 请求由 `crates/client` 发出。服务协议见 [API.md](API.md)，传输见 [TRANSPORT.md](TRANSPORT.md)。当前客户端已有认证的 iroh 数据通道；画面、输入与文件内容处理仍属后续工作项。

## 本地运行

需要 Windows、Rust 1.93、Node/npm 和 Docker。服务端示例只绑定回环，测试邮件只进入本地 Mailpit。

```powershell
./scripts/start-local.ps1
# 另开终端
npm ci
npm run tauri -w @farsail/desktop -- dev
```

或者先运行 `./scripts/test-transport.ps1`，它启动项目专属的 `farsail-dev` PostgreSQL/Mailpit，执行传输、后端与客户端测试、前端构建和 Tauri 编译；不会发送真实邮件。服务地址默认为 `http://127.0.0.1:8787`，可在退出登录后的设置页修改。Mailpit UI 在 `http://127.0.0.1:58025`。结束后以 `./scripts/test-coordinator.ps1 -Stop` 停止本项目容器；开发数据库卷仍保留。运行中的 `start-local.ps1` 窗口用 Ctrl+C 结束协调服务。

## 使用流程

1. 注册账号，去 Mailpit 读取本地验证令牌，在验证邮箱页提交；真实部署须配置 TLS SMTP。
2. 登录后在总览中确认绑定本机。Rust 生成 Ed25519 密钥，签署服务挑战，声明 `can_host=false`、`can_files=false`。绑定后启动心跳；退出登录或切换账号会清掉旧的登录与设备凭据、停止旧身份心跳。
3. “我的设备”逐页读取完整同账号设备，包含离线和管理员停用项，可搜索、按能力筛选、改名和解绑。“账号安全”可改密、退出及撤销其他登录会话。
4. “设置”可启动安全传输并填写自建 HTTPS relay URL、UDP 绑定地址或强制 relay；本机地址经设备凭据与心跳代次发布。“连接请求”显示真实授权请求和状态，支持跨账号一次性邀请码、目标端批准/拒绝及主动撤销。获批发起端可以建立 iroh 认证通道，页面显示被选中的 direct/relay 路径和 RTT。批准或连接都不会启动屏幕共享或文件内容处理。
5. 管理员账号可在同一客户端查看用户、登录会话和审计元数据，启停账号/设备、调整注册策略、创建和撤销注册邀请。管理员无法代替目标设备批准远控。

## 凭据边界

- `crates/client` 的串行状态锁负责访问 token 刷新轮换、HTTP 和设备签名；WebView 不获得访问/刷新 token、设备 token、设备私钥或 grant token。前端仅暂时持有用户输入的密码、邮箱验证令牌及一次性邀请码，不使用 `localStorage`。
- 登录会话、设备凭据、每个服务地址与账号对应的设备私钥分别存放在 Windows 用户作用域的 DPAPI 加密文件中，目录由 Tauri 的 `app_local_data_dir` 提供。iroh 使用这同一把设备私钥。退出先取消本地会话，再删除登录和设备凭据；保留私钥以便同一账号下次重新证明设备身份。移动端需另实现平台安全存储适配。
- 公网服务地址只能使用证书验证的 HTTPS，包含 HTTPS IP；显式回环地址才允许 HTTP。请求不跟随重定向，响应单次上限 1 MiB。不存在跳过证书验证的开关。
- 登录撤销和设备 token 失效由服务端决定。网络离线时退出仍删除本机凭据，同时报告远端撤销未确认。下次登录须重新签名绑定设备。授权 grant 留在原生内存，通过设备认证的 iroh 握手交给正确的发起端，页面不接触该凭据。原生层最多处理 8 个同时握手、16 个活跃会话；退出或重启传输后的旧启动/旧连接结果不会写回。

## 已知边界与 WI-004/005 接口

客户端不会宣称本机能作为被控端。普通双端 UI 中没有实际可被控目标，须由 WI-004 完成采集与能力声明后才能从应用实际提供被控能力。集成测试只在隔离 schema 内为目标设备临时启用未来能力，以检验当前授权和传输路径。

原生接口 `NativeClient::start_transport`、`connect_transport`、`transport_session` 和 `farsail_transport::Session::send/receive` 已可供下项使用。`Media`、`Control`、`File` 有独立权限和尺寸边界。WI-004 才能为被控设备启用画面与输入能力，WI-005 才能启用文件能力；注册时继续保持 `can_host=false`、`can_files=false`。本项已在本地双端验证直连及强制 TLS relay 路径，但没有公网 TLS 代理、第二台电脑、跨 NAT 穿透率或手机真机证据。
