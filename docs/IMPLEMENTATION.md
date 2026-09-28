# FarSail 实施与接力记录

本文件是开发状态的权威入口。`PROJECT_DESIGN.md` 为产品目标，不能作为功能已实现的证据。用户于 2026-09-28 授权按设计实现，并已授权把完成的代码提交至公开 GitHub 仓库。

## 起点与协作边界

- 初始提交：`e74298567ce3e2fe819b0e1c05fdbba40d7cc15b`，仅设计、调研、品牌资产。
- 远端：`https://github.com/wanghao9103/farsail`，公开、MIT，主分支 `main`。
- 按用户全局长任务规则，每个可独立验收结果使用一个同项目任务；共享工作区写入串行。总控在写任务进行中只做只读检查，任务结束后再更新下一工作项。
- 每个任务开工前核对记录的基线 HEAD 和工作区。不得覆盖其他未提交改动；只在自己的 Write Set 中修改文件，必要扩展先记录原因和具体路径。
- 本地运行状态进入忽略的 `.local/`；Rust 缓存 `target/`、Node 缓存 `node_modules/` 不提交。Docker 仅操作明确命名的 `farsail-dev` 项目和自身服务，端口只绑定本机回环；不得停止用户其他服务。
- 每项交付包含实现、适当测试、准确文档和独立提交。推送前审查暂存范围、凭据与私人路径；使用现有 GitHub 登录及项目 noreply 身份。不得把测试账号、令牌、真实邮件、屏幕画面或运行数据库上传。
- 真实邮件发送、公网服务器部署及用户其他设备尚未配置。代码和本机隔离环境可继续实现；远端、跨 NAT 与手机验证缺口须如实记录。

## 环境基线

- 当前主机：Windows；Rust 1.93.0、Cargo 1.93.0；Node 23.11.1、npm 10.9.2；Docker Engine 29.1.3 可用。
- Android SDK 目录存在，具体平台/NDK/模拟器与真机仍需检查。
- 当前仅有本地 Windows 执行环境；iOS 需要 Mac/Xcode 或可用 CI 与真机，不能以网页预览代替验证。
- 依赖使用实际注册表可用且兼容工具链的稳定版本并提交锁文件；此前文档中的网络资料版本必须结合当前构建重新确认。

## 工作项与顺序

| Work Item | 可验收结果 | 状态 |
| --- | --- | --- |
| WI-001 | Rust workspace、PostgreSQL 账号/设备/授权服务、隔离本地运行与权限测试 | 完成：本地验收通过，代码已推送 |
| WI-002 | Tauri 客户端登录、设备注册/列表、凭据安全存储、用户/管理员界面 | 完成：本地与 Windows CI 通过，代码已推送 |
| WI-003 | iroh 端到端连接、应用授权、直连/中继配置与撤权 | 完成：本机专项验证和远端 CI 均通过 |
| WI-004 | Windows 屏幕采集、用户确认、鼠标键盘输入与可用远控基线 | 本机验收通过，远端 CI 待核对 |
| WI-005 | 分块双向文件传输、无损压缩、校验/续传、授权与限速 | planned |
| WI-006 | 实际硬编/解码、多屏与自适应速率、能力协商和 4:4:4 路径 | planned |
| WI-007 | Android/iOS 手机控制与文件接口、可执行平台构建和验证 | planned |
| WI-008 | 全链路回归、部署/打包、公开仓库与真实环境验证 | planned |

任何工作项可以因实测依赖拆成更小的连续项，但不得把尚未实现的范围标成完成。手机被控、无人值守和其他后续扩展仍按原设计作为后续路线。

## WI-001：Rust 账号与设备服务

### 基线与 Write Set

开工核对 HEAD 为 `1a48cdbd268335a29406724cc54677d5aa00a9f4`，工作区干净。代码路径均在下述 Write Set 内；未修改设计草案或其他工作项。实现提交 `0e6be111e03478053a9d10e252d46a9b392a4367` 已推送并由 `git ls-remote` 核对；Linux GitHub Actions 后端工作流通过。详情见 `docs/verification/WI-001.md`。

允许修改：`Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、`crates/core/**`、`services/coordinator/**`、`deploy/local/**`、`scripts/test-coordinator.ps1`、`.github/workflows/backend.yml`、`.gitignore`、`README.md`、`docs/IMPLEMENTATION.md`、`docs/API.md`、`docs/verification/WI-001.md`。

### 必须交付

- Rust/Axum 模块化服务与 PostgreSQL migrations，核心协议/DTO 与服务分离。配置来自环境或忽略的本地文件；默认仅本机监听，生产配置不得静默开启开发后门。
- 邮箱注册、验证、登录、密码修改/恢复；Argon2id 密码哈希；短访问会话、刷新轮换/撤销、退出/踢出。邮件通过可替换 mailer，测试使用内存 mailer，本地可使用 Mailpit；本项不发送真实邮件。
- 设备公钥持有证明、一次性挑战、唯一归属、同账号完整分页设备列表、能力/状态、改名和解绑；专用设备凭据与用户会话权限分离。心跳有代次/期限，禁止旧连接污染新连接状态。
- 临时邀请、远控/文件权限请求、目标设备批准/拒绝、短授权租约、续期与撤销 API；本项测试状态机和授权，真实传输在 WI-003 接入。
- 管理员用户启停、注册策略、设备撤销、会话/审计接口。管理员不能自动获取用户远控权限；bootstrap admin 通过显式本地配置/命令而非注册先到先得。
- 可复现本地 PostgreSQL/Mailpit 环境、运行文档和 API 契约；开发/测试数据隔离，外部依赖或未完成功能清楚说明。

### 验收

- `cargo fmt --all -- --check`
- `cargo test -p farsail-core`
- `cargo test -p farsail-coordinator`（包含真实 PostgreSQL 集成测试，使用独立测试库/隔离 schema）
- `cargo clippy -p farsail-core -p farsail-coordinator --all-targets -- -D warnings`
- 核心负向测试：跨账号枚举/访问、设备 challenge 重放与并发绑定、device token 越权、refresh 重用/撤销、旧心跳代次、批准前/过期后/撤权后的 grant、只读权限与管理员边界。
- 本地实际启动服务并验证健康检查；记录 HTTP smoke、迁移、端口、命令和退出方式。不得用仅 mocks 的测试声称数据库授权行为通过。
- 更新本项状态、证据、下一项契约及仍未验证环境；提交并推送，核对远端提交。尚有核心失败时继续修复，不能以“骨架完成”结项。

### 记录

Rust workspace、共享协议枚举、Axum 协调服务及 PostgreSQL 迁移已实现。账号支持邮箱验证、登录/恢复、Argon2id、访问/刷新轮换、登录会话撤销；设备支持 Ed25519 持有证明、唯一绑定、登录会话关联的专用凭据、完整分页列表、心跳代次与解绑；授权支持一次性邀请、目标设备批准/拒绝、30 秒 grant、续期和撤权；管理员支持用户/设备启停、注册策略和邮箱定向注册邀请、元数据审计。测试使用真实 PostgreSQL 隔离 schema，邮件使用内存 mailer 或本地 Mailpit。完整命令、结果、端口、运行状态和仍未验证项见 [WI-001 验证记录](verification/WI-001.md)，请求契约见 [API 文档](API.md)。

WI-002 对接从 `/v1/me` 开始；账号访问 token 与设备 token 分开保存，设备密钥留在原生安全存储。`GET /v1/devices` 返回已绑定的同账号全部设备，包含离线与管理员禁用项。`GET /v1/remote/{id}` 只报告 pending/approved/denied/revoked/expired 状态，不传 grant token。目标端 `decide`/`renew` 才收到 grant token；其安全交付给发起端及 iroh 握手校验属于 WI-003。当前未验证公网、域名/TLS 代理、真实邮件、第二台 Windows、P2P/中继撤权或移动端。

总控验收：实现及交接已经收口，远端 main 为 `9f222515c0482cf83e318c1788dcbf34f9d61be9`；本机真实数据库/HTTP 与 Linux CI 均通过，工作区干净，当前无本项运行服务。批准继续 WI-002。

## WI-002：Tauri 客户端与账号设备界面

### 基线与 Write Set

代码基线 `9f222515c0482cf83e318c1788dcbf34f9d61be9`；本节计划单独提交后，以该计划提交作为开工 HEAD。

允许修改：`apps/desktop/**`、`crates/client/**`、`packages/ui/**`、`package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock`、`.gitignore`、`scripts/start-local.ps1`、`scripts/test-desktop.ps1`、`.github/workflows/client.yml`、`README.md`、`docs/IMPLEMENTATION.md`、`docs/CLIENT.md`、`docs/verification/WI-002.md`。如发现已有后端契约阻断实际接入，先记录所需 `services/coordinator/**` / `docs/API.md` 的精确修复范围，再修复并跑后端专项回归；禁止随意改写历史迁移。

### 必须交付

- 可在 Windows 编译运行的 Tauri 2 + TypeScript 客户端（优先 React/Vite），使用已选 FarSail v2 图标。界面包含登录/注册/邮箱验证/恢复、我的设备、连接请求与会话、账号安全、设置和管理员页面；管理员页面可先集成在同一客户端。
- 所有业务页面调用真实协调 API。Rust 原生层负责 HTTP、登录/刷新串行化、设备签名和心跳；界面不能获得长期 token 或私钥，不使用 WebView localStorage 保存凭据。不构造假设备列表作为运行结果。
- 服务地址可配置：本机开发可用显式 loopback HTTP，公网强制验证 HTTPS；无域名的 HTTPS 公网 IP 是合法配置。禁用验证证书的选项不可引入。
- Windows 凭据采用系统安全存储或 DPAPI 保护；密钥、账号登录、设备凭据分开管理。退出/切账号清空 UI 状态并停止旧 heartbeat/poll，撤销登录后不能用旧设备凭据继续上线。后续移动端通过接口适配原生安全存储。
- 登录后确认设备绑定，完整分页读取同账号设备（含离线/禁用），搜索和按能力筛选；支持改名/解绑、其他登录会话撤销。设备能力必须对应实际已实现功能；尚未接入采集/文件时，不把本机宣称为可工作的被控端。
- 真实远控请求/临时邀请、目标设备的批准/拒绝、状态轮询与取消；本项明确只接通授权 UI，画面/输入与文件通道由后续项实现。grant token 留在 Rust 原生层，不通过页面或日志泄漏。
- 管理员页面实际接入用户启停、设备撤销/恢复、注册策略/邀请、会话与审计。服务端授权仍为权威，前端角色判断只控制展示。
- Rust 客户端逻辑放在可复用 crate；前端布局适配窄屏，为手机界面准备。不要在本项安装/运行 iOS 环境或并行启动新任务。

### 验收与运行边界

- 提交 npm/Cargo 锁文件；执行前端类型检查、生产构建、必要的行为测试，`cargo fmt --all -- --check`、客户端 crate 测试、Windows Tauri `cargo check/build` 和适用 Clippy。实际命令写入证据文档。
- 通过隔离的 farsail-dev PostgreSQL/Mailpit 实测注册/验证/登录、原生客户端绑定/签名、列设备、刷新/撤销、管理员路径；不使用假 HTTP 响应代替该集成验证。
- 浏览器或可用客户端测试真实界面布局、空状态/错误状态及交互；说明浏览器预览与 Tauri 原生能力验证的各自范围，不能把 preview 冒充原生端到端。
- 测试进程与本地账号/凭据不得进入 Git；本项结束记录所有启动进程、端口和保留状态，清理自己创建且不再需要的服务。测试只能使用本项目隔离实例。
- 完成后更新记录、独立提交并推送，核对 CI 与远端，再交给 WI-003；核心功能不以 TODO/stub 替代。

### 记录

Windows Tauri 2 + React/Vite 客户端、可复用 Rust 原生客户端 crate、DPAPI 用户作用域凭据、Ed25519 设备身份、登录/刷新/心跳、完整设备列举和筛选、授权申请/批准/拒绝/撤销、账号安全与管理员页面已实现。浏览器仅作响应式布局验证；真实 Windows WebView2 的 `state` 和登录命令已通过 IPC 烟测。客户端在隔离 PostgreSQL 上经真实 HTTP 完成注册、验证、绑定、授权、管理员与退出链路。实现细节、命令与风险见 [WI-002 验证记录](verification/WI-002.md) 和 [客户端文档](CLIENT.md)。

WI-003 的输入：原生层提供 `NativeClient::take_grant_for_transport` 和 `renew_grant_for_transport`，只有目标设备批准端取得 grant。下一项须加入认证信令将其安全交给发起端，并用两端公钥、nonce、session ID、权限及期限绑定 iroh 握手、租约续期与断线撤权。`GET /v1/remote/{id}` 仍只返回状态，不能用它领取 grant。当前没有公网服务器、有效 HTTPS IP 证书、第二台 Windows、P2P/中继或移动真机验证；客户端保持 `can_host=false` 和 `can_files=false`，不可把授权 UI 解释为实际远控。

总控验收：WI-002 实现 `e507412`、交接提交 `a1c9cf68a23724612243fb267c24d2c43e40fa2c` 已推送。真实 PostgreSQL/HTTP、Windows DPAPI、WebView2 登录 IPC、本机编译和 Windows/Linux CI 均通过；开发进程停止，工作区干净。批准继续 WI-003。

## WI-003：认证的加密传输与中继

### 基线与 Write Set

开工 HEAD `e0c610678cddc2d3371b0246a52969f19d290d32`，工作区干净；本地 `main` 比远端领先一条已授权的 WI-003 计划提交。所有代码改动均在本节 Write Set 内。

允许修改：`crates/transport/**`、`crates/core/**`、`crates/client/**`、`services/coordinator/**`（只添加新迁移，不改已提交的历史迁移）、`apps/desktop/src-tauri/**`、`apps/desktop/src/**`、`packages/ui/**`、`Cargo.toml`、`Cargo.lock`、`deploy/relay/**`、`scripts/test-transport.ps1`、`.github/workflows/transport.yml`、现有后端/客户端 CI 的必要兼容或缓存配置、`README.md`、`docs/IMPLEMENTATION.md`、`docs/API.md`、`docs/CLIENT.md`、`docs/TRANSPORT.md`、`docs/verification/WI-003.md`。

### 必须交付

- 使用已核实兼容 Rust 1.93 的 `iroh` / `iroh-relay` 1.2.x，提交锁文件。复用设备注册时的同一 Ed25519 身份；连接握手公钥须与授权中的双方身份一致，不能另生成匿名 endpoint 绕开设备注册。
- 增加经设备凭据与当前在线代次校验的连接地址登记、更新和授权查询。发现信息只对有权的账号/会话参与者返回，失效设备不可发布，跨账号邀请不能变成任意设备地址目录。
- 目标端批准后，grant 只通过认证的端到端连接交付给正确的发起端；发起端向协调服务验证 grant，双方校验会话、设备公钥、nonce、权限与期限后才开放对应数据通道。握手有超时、并发上限和重放处理；拒绝、未批准、错误身份和权限升级必须被拒绝。
- 实现被封装的会话数据通路（控制、媒体、文件的类型/版本和尺寸边界），为 WI-004/005 提供真正可用的有界读写接口。不能只提供 echo 示例，也不能提前宣称屏幕/文件内容处理完成。
- 续期、服务端撤销、退出、解绑、断线与租约失效能关闭已建立的 P2P/中继数据通路。用可信服务端剩余 TTL 和本地单调时钟执行有界租约，避免信任对端传来的任意过期时间；退出先取消本地会话，不能被缓慢 HTTP 请求长期阻塞。控制权限不能从 view 推导。
- 本地直连与自建中继均可配置；使用真实路径/选中路径信息显示 direct/relay/connecting/closed 及 RTT。配置了 relay URL 不等于实际走中继。
- 测试禁用公网地址发现，明确 loopback 绑定并关闭 UPnP/PCP/NAT-PMP。客户端默认也不自动修改路由器端口映射。中继只绑定本机测试端口；生产部署文件保留可配置域名或公网 IP 和正常 TLS 验证，用户公网机器尚未提供，不部署外部环境。
- 单独测试强制中继：用 1.2 的 `clear_ip_transports()` 等实际配置禁用 IP 直连并确认选中路径；不要修改系统全局防火墙来制造条件。中继 TLS 使用测试 CA/证书的正确验证，禁止 accept-all verifier。
- 将原生传输状态/请求与现有 Tauri 界面接通，但长期凭据及 grant 仍不流向 JS。保留未实现媒体/文件能力的真实标识，下一项才能启用主机对应能力。

### 验收

- 本地两个真实 endpoint：授权握手、双向数据、错误公钥/未批准/过期/replay/权限不匹配拒绝；验证已有流在撤权/失联租约到期后不能继续传送业务数据。
- 用真实协调 HTTP/PostgreSQL 与设备绑定完成至少一条完整传输集成；本地 TLS relay 强制中继也传送并校验数据，检查实际路径。只模拟授权回调的测试不足以作为完整集成证据。
- `cargo fmt --all -- --check`、传输/客户端/后端适用 tests 与 Clippy、前端 typecheck/build、Windows Tauri 编译。新增迁移在已有本项目数据库和隔离测试库中分别验证，不删除先前数据来掩盖迁移问题。
- 为公网 NAT 与第二台设备保留部署说明和待验收清单，不把 loopback 成功表述为已完成公网穿透率验证。
- 更新所有运行端口/进程状态和下一项接口，提交/推送并核对 CI/远端。可复用结论按技能沉淀。

### 记录

实现 `crates/transport` 设备密钥复用的 iroh 1.2 加密 QUIC、分类型有界数据帧、真实 selected path/RTT 和本地单调短租约。协调服务添加独立新迁移，设备地址按注册公钥、在线代次登记，批准会话的参与设备才能查询；relay URL 受 HTTPS/凭据格式及服务端名单限制。目标只经认证连接交付 grant，两端核对会话、公钥、nonce、精确权限和服务端有效 TTL，续期与撤权关闭已有通道；断开的旧批准不能在端点重启后复用。Tauri 页面可启动/连接/查看路径，但 `can_host=false`、`can_files=false`，媒体处理和文件内容仍未实现。详细接口、运行方式与验证证据见 [认证传输](TRANSPORT.md)、[API](API.md)、[客户端](CLIENT.md) 和 [WI-003 验证记录](verification/WI-003.md)。

本机两端直连、强制本地 TLS relay、真实 PostgreSQL/HTTP 授权集成与既有开发数据库原地迁移已通过。实现提交 `e9e119a17e485da97d288913b63e5ff809cc45d9` 已推送；后端、传输和 Windows 客户端 CI 均成功。用户尚未提供公网服务器和第二台设备；公网 IP 证书、跨 NAT 与移动端均未验证。进程清理、命令和运行链接见验证记录。WI-004 可直接使用原生 `Session::send/receive` 的 `Media`/`Control` 通道，在实际采集与输入确认实现后再启用主机能力。

总控验收：WI-003 最终远端 `e4992a86f35a1b0dd84f9ad211229e40abcb1edd`，工作区干净；直连、强制 TLS relay、真实授权集成、断线后的重新批准及三条 CI 均有证据，允许继续 WI-004。用户再次说明公网仍在准备，本机开发继续。

## WI-004：Windows 屏幕与输入

### 基线与 Write Set

代码基线 `e4992a86f35a1b0dd84f9ad211229e40abcb1edd`，本节计划提交后的 HEAD 为开工基线。

允许修改：`crates/windows/**`、`crates/media/**`、`crates/core/**`、`crates/transport/**`、`crates/client/**`、`apps/desktop/**`、`packages/ui/**`、`services/coordinator/**`（只有主机能力/参与者展示等实际接入修复；添加新迁移而不改历史迁移）、`Cargo.toml`、`Cargo.lock`、`package.json`、`package-lock.json`、`.gitignore`、`scripts/test-remote.ps1`、`.github/workflows/remote.yml`、现有工作流的必要兼容和 Rust 构建缓存、`README.md`、`docs/IMPLEMENTATION.md`、`docs/API.md`、`docs/CLIENT.md`、`docs/TRANSPORT.md`、`docs/REMOTE.md`、`docs/verification/WI-004.md`。

### 必须交付

- Windows 原生屏幕枚举、选择及 DXGI 采集。候选 `windows-capture` 2.0.1 的 Rust `dxgi_duplication_api`（不用 Python/Qt），或同等可验证实现。遵守显示器所属适配器、行间距、旋转、DPI 和负坐标；处理超时、ACCESS_LOST/显示器变更及资源释放。截图数据只在本机内存或忽略的测试目录，不进 Git。
- 首条实际媒体路径采用明确标注的 JPEG 低帧率基线，经过现有认证 iroh 会话，Tauri 收到二进制帧并展示真实画面；H.264/H.265 和 4:4:4 留在 WI-006 实测接入，不用选项或标签冒充已有编码能力。帧含显示器/布局版本、序号、尺寸、时间与编码标识，校验编码图像的实际尺寸和像素预算。
- 有界最新帧队列和基础带宽/FPS控制，画面不变时减少重复工作。现有 transport 通用 IO 超时为 10 秒，媒体需适当的帧期限与 reset/drop 机制，过期 JPEG 帧不能拖住新帧或可靠输入；不允许无界队列或假固定 FPS 指标。
- 真实鼠标移动、左右键、滚轮、按键/组合键和文本输入的 Windows 系统适配；坐标正确映射显示区域、缩放/留黑和远端显示器。view grant 不得注入输入；字段白名单与消息量受限。实现执行时重新检查活动会话/代次，取消后丢弃排队输入并释放所有由本会话按下的键，避免 release-all 后旧 keydown 又执行。
- 被控端本地开启共享、明确批准 view/control、持续显示状态并可立即停止，窗口关闭/退出/租约失效都会停止采集与输入。先停止本地资源再等待 HTTP 撤销。系统安全桌面/UAC/无人登录仍不支持，遇到此边界显示真实原因，不绕过系统权限。
- 注册/更新 `can_host` 必须与实际 Windows 后端和本地开关一致，能让同账号真实客户端列出并连接该主机；`can_files` 仍待 WI-005。修复必要 API/操作顺序，使普通双端客户端流程可用，不能只在数据库里手改 capability 来演示功能。
- 加入实际 viewer 页面、显示器选择、缩放/适应窗口、仅查看/控制状态、停止按钮和网络 RTT/实际帧率。批准窗口显示有意义的请求账号/设备信息。双方展示由连接 TLS exporter 与会话上下文导出的可比较校验码，不能展示 grant token。网络 RTT 不冒充端到端画面延迟。
- 增加桌面单实例或显式配置目录排他机制，防止双启动共享同一刷新令牌导致互相撤销；测试身份使用独立临时存储。

### 验收与资源边界

- 本机真实显示器至少采集帧、编码、经授权连接传输并解码验证；证明这是真实原生路径，同时保留合成图用于稳定协议、尺寸、坐标和背压测试。不上传实际桌面像素或私人窗口内容。
- 输入测试仅在自建受控测试窗口/接收器中使用无害数据，执行前确认前台属于该窗口，结束释放按键并清理。无法安全获得前台时记录未验证项，不能把按键注入用户终端或其他应用，也不能用模拟函数冒称系统注入成功。
- 覆盖拒绝/只读权限、过期/撤销时排队输入、失焦释放、非法坐标/帧尺寸、断线停流、慢消费者、显示器不可用/采集丢失以及单实例行为。测试与实现风险对应，原生截图/注入验收不能由浏览器预览替代。
- 完成真实服务+客户端+传输回归、前端 typecheck/build、Rust tests/fmt/Clippy 和 Windows Tauri 构建；CI 中无交互桌面时明确区分可运行的合成测试与本机原生烟测。已有三项冷缓存构建明显耗时，可添加不包含凭据的 Rust 依赖/target 缓存并验证。
- 更新运行方法、明确支持范围、进程/端口清理与下一项接口；提交推送、核对 CI 和远端，并做知识沉淀。公网或第二台电脑仍不可用时，继续本机实现并记录缺口。

### 记录

Windows 原生层按显示器所属 DXGI 适配器采集，处理 DPI、负坐标、旋转和 row pitch；首条媒体链路是明确标注的低帧率 JPEG，带显示器、布局、序号、尺寸及时间元数据。主机与 viewer 使用已有认证 iroh 会话、最新帧/ACK 和 payload 预算；Windows 原生系统输入在 control 权限与本机共享开关、会话代次、布局快照下执行，停止后释放已注入输入。普通客户端通过带心跳代次的能力 API 启用/关闭 `can_host`，不手改数据库；`can_files` 仍为 false。Tauri viewer 接收 raw binary IPC，显示双方 TLS exporter 校验码、路径 RTT、实际 FPS、权限和停止控件；配置目录使用排他锁。接口与运行方法见 [远控文档](REMOTE.md)，命令、真实采集/安全输入证据及剩余环境缺口见 [WI-004 验证记录](verification/WI-004.md)。

本机真实 PostgreSQL/HTTP/iroh 授权链路与 DXGI→JPEG→解码通过，受控测试窗口中的无害文字、组合键和点击通过，WebView2 二进制 IPC 与单实例烟测通过。H.264/H.265/4:4:4 待 WI-006；文件通道待 WI-005。公网部署目标刚由用户告知总控，本项未连接外部机器，后续需独立服务器联调及第二台 Windows/跨 NAT 验证。
