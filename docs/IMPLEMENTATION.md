# FarSail 实施与接力记录

本文件是开发状态的权威入口。`PROJECT_DESIGN.md` 为产品目标，不能作为功能已实现的证据。用户于 2026-09-28 授权按设计实现，并已授权把完成的代码提交至公开 GitHub 仓库。

## 起点与协作边界

- 初始提交：`e74298567ce3e2fe819b0e1c05fdbba40d7cc15b`，仅设计、调研、品牌资产。
- 远端：`https://github.com/wanghao9103/farsail`，公开、MIT，主分支 `main`。
- 按用户全局长任务规则，每个可独立验收结果使用一个同项目任务；共享工作区写入串行。总控在写任务进行中只做只读检查，任务结束后再更新下一工作项。
- 每个任务开工前核对记录的基线 HEAD 和工作区。不得覆盖其他未提交改动；只在自己的 Write Set 中修改文件，必要扩展先记录原因和具体路径。
- 本地运行状态进入忽略的 `.local/`；Rust 缓存 `target/`、Node 缓存 `node_modules/` 不提交。Docker 默认仅操作 `farsail-dev` 及工作项明确新增的隔离测试项目，本机端口只绑定回环；不得停止用户其他服务。WI-008A 另行定义 `farsail-deploy-test`，用户服务器运行参数化的 `farsail-prod`。
- 每项交付包含实现、适当测试、准确文档和独立提交。推送前审查暂存范围、凭据与私人路径；使用现有 GitHub 登录及项目 noreply 身份。不得把测试账号、令牌、真实邮件、屏幕画面或运行数据库上传。
- 公网 Ubuntu 24.04.2/amd64 已由用户准备，Docker 29.8.1 与 Compose 5.5.1 已安装；约 1.6 GiB 内存、35 GB 可用磁盘。首次采用仅回环测试收件箱。服务器尚未部署项目，当前由用户在外部终端执行命令，总控没有可接管 SSH 或密钥。公网、跨 NAT 与手机验证缺口须如实记录。

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
| WI-004 | Windows 屏幕采集、用户确认、鼠标键盘输入与可用远控基线 | 本机及三条远端 CI 通过 |
| WI-008A | 公网 IP 部署包、预构建 Linux 制品与联调准备 | 完成：固定六镜像制品、匿名下载、跨Docker存储导入及两轮Linux CI通过 |
| WI-008B | Windows x64 预览安装包、正式构建与公开制品验收 | 完成：NSIS、原生安装生命周期、CI与完整匿名下载均通过 |
| WI-008C | 部署下载HTTP/1.1、可靠续传与缓存复用 | ready：优先修复实际部署阻断 |
| WI-005 | 分块双向文件传输、无损压缩、校验/续传、授权与限速 | 进行中：已暂让写权限给WI-008C，未验证WIP保留 |
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

实现提交 `78541d08d885b6827c9ed8c22a1aa7787e1f365d` 已推送并经 `git ls-remote` 核对；三条 GitHub Actions 在交接时仍运行中，链接与最终本机端口/进程状态见 [WI-004 验证记录](verification/WI-004.md)。本机写集已收口且工作区干净，允许总控串行开始部署包/服务器联调，CI 完成状态继续只读核对。

总控核对：最终提交 `a66e43f3b7a42f7c6ee1a565bc96c8bc66511770` 与远端 main 一致，工作区干净；WI-004 写任务已完成、无本机运行进程。用户要求开始公网部署，先串行执行 WI-008A，再返回 WI-005/006/007。CI 未完成不等于已通过，若失败由当前写任务在明确范围内修复，总控不并行改代码。

## WI-008A：公网 IP 部署包与服务器联调准备（提前执行）

### 背景与顺序
用户已准备 Ubuntu 24.04.2 x86_64 公网服务器，约 1.6 GiB 内存，无 Docker/Swap。总控正在指导用户安装 Docker。服务器地址和登录信息仅保存在当前会话/忽略的本地记录，公共仓库保持参数化。当前 Codex 无可接管 SSH、无密钥；不得声称已连接或远程部署。先完成能独立验收的 Linux 部署包，再由总控配合用户运行；后续 WI-005/006/007 继续，不因此标记全部产品完成。

后续准备更新：用户已安装 Docker 29.8.1 / Compose 5.5.1，已克隆代码；阿里云服务器DockerHub出站拉取超时，交付改为包含全部六个运行镜像的离线归档。服务器具体产品类型和GitHub附件下载仍待确认。用户提供标称200Mbps，不作为实测吞吐。未接管SSH，不修改外部服务器。

### 基线与 Write Set
代码基线 `a66e43f3b7a42f7c6ee1a565bc96c8bc66511770`（WI-004 本机验收通过，CI 由总控只读跟踪），本节计划提交后的 HEAD 为开工基线。
允许修改 deploy/production/**、deploy/relay/**、Dockerfile 或 deploy 下专用 Dockerfile、.dockerignore、scripts/deploy/**、scripts/test-deploy.ps1、.github/workflows/deploy.yml、Cargo.toml/Cargo.lock 必要兼容、services/coordinator/**（仅内部 relay 准入/部署配置/SMTP 端口等必要修复，新迁移不改历史迁移）、crates/client/** 或 apps/desktop/**（仅真实公网地址/relay 配置阻断修复；先记录精确改动范围）、README.md、docs/DEPLOYMENT.md、docs/IMPLEMENTATION.md、docs/API.md、docs/verification/WI-008A.md。不实现文件引擎或硬件编解码，不修改已完成 WI-004 媒体业务。

### 必须交付
- 可重复构建 Linux amd64 协调服务和固定 iroh-relay 1.2.0 的运行镜像/发布包；服务器只拉取预构建制品，不在 1.6 GiB 主机编译 Rust。提交锁文件构建，包含必要 CA 与迁移；通过 GitHub Actions 生成带提交标识和摘要的可下载制品或公开 GHCR 镜像，实际核对存在且匿名可用再写命令。不能发布未验证的 latest 冒充固定版本。
- Linux Docker Compose 生产/联调配置，独立 farsail-prod 项目与 volumes。协调服务当前强制 loopback，须实际解决容器网络可达：可用 Linux host network 或共享 network namespace，不能示例绑定 0.0.0.0 而二进制启动即退出。PostgreSQL、SMTP 测试收件箱、内部管理/准入接口仅内网/回环；公网默认 API 443/TCP、relay 8443/TCP、ACME 80/TCP（可配置，检查既有端口）。
- IP 证书正常 CA 验证，Certbot >=5.4 webroot+shortlived/IP 支持、HTTP-01 bootstrap、证书自动续期和服务 reload/restart hook、续期失败可见。既有模板 relay 与 API 都占443要拆开。云主机的公网IP可能是NAT映射、不属于网卡；公网URL用用户IP，监听地址用实际本地0.0.0.0/接口地址，不能把PUBLIC_IP直接绑定导致EADDRNOTAVAIL。若域名也可配置，但不能要求用户购买域名或关闭 TLS。未获得公网机器与交互账号授权信息时，只做本机测试 CA 演练，真实 CA 签发由用户运行明确命令。
- Relay 不能默认给任意设备无限使用。优先复用 upstream AccessConfig::Http：POST 带 X-Iroh-NodeId，经 relay 已验证公钥，私有 bearer token 保护协调服务内部准入端点；仅当前有效注册/启用设备可接入，未知/禁用/缺token/服务异常拒绝。API反代阻止内部接口公开。核实二进制 schema、超时与限速，限制 client_rx；accept_conn_limit/burst 在 1.2.0 源码明确未实施，不能当有效保护。无需另写整个relay服务器。保持应用授权和30秒租约撤销，与relay准入分别验证。
- 参数与秘密由服务器忽略文件/挂载提供，随机密码不打印、不进入 Git/镜像层/CI 输出；证书私钥留服务器。容器非 root 可用时采用，配置文件最小权限、持久化和备份/恢复、升级固定ref/回滚可操作。不得改全局Docker配置、清理其他容器或开放数据库公网端口。
- 用户已明确首次采用仅回环 Mailpit 联调。账号验证邮件要有真实可行流程：支持 TLS SMTP 的完整配置；尚无 SMTP 时可提供明确标记的测试 profile 与仅回环 Mailpit，通过用户自己的 SSH 转发访问，不能公开收件箱或跳过邮箱验证。管理员仍通过现有显式 bootstrap 命令，不能首个注册自动提权。
- 客户端设置示例准确区分 API URL 与 relay URL，并说明本机默认127.0.0.1绑定不能直接当跨网设置；实际选中 relay 路径才算中继测试通过。跨 NAT P2P 发现如尚未验证，明确缺口而不因服务器启动成功就声称穿透完成。
- 提供针对 Ubuntu24.04/root/amd64 的短步骤，每个复制块是单条可执行命令或已提交可审阅脚本，避免多行粘贴被用户终端拼接。阶段有检测和错误停点，公网不执行测试脚本的数据库清空/开发迁移重置。基础检查、安装依赖、clone固定提交、初始化配置、证书、启动、healthz、注册/管理员、日志、停止/升级可跟随。

### 验收与运行边界
- 在本地 Docker 实际构建并运行隔离部署，确认健康检查真实连库、数据持久化、内部端口仅回环/内网、配置和迁移存在、无凭据进入镜像/公开日志。使用独立项目 `farsail-deploy-test`，测试宿主发布端口限定回环（候选 HTTP 58080、API 58443、relay 58444、数据库 55433；实际使用前检查），不与 farsail-dev 或其他服务冲突；不能为了测试修改 Docker Desktop 全局 host-network 设置。
- 用测试 CA 的正确验证完成 HTTPS API 与 relay 端点、已注册设备准入和未知身份拒绝；反代内部接口不可达、relay服务停机/证书配置错误有可理解失败。可用 openssl/curl 检查 SAN/IP 与链，不能使用 -k 作通过证据。
- 单独记录 Linux镜像、本地Compose、公开制品下载、生产SMTP、真实CA、公网/家中电脑分别验证到哪一层；不冒称远端部署。
- backend适用 fmt/tests/clippy、部署脚本语法/Compose配置、GitHubCI/制品检查，必要的原有auth回归。停止本项测试进程/容器而保留需保留的卷；记录运行/缓存边界。
- 更新权威记录、知识沉淀，提交推送后核对远端，通知总控接用户服务器步骤。用户补充SSH访问时由总控分配后续真实部署动作，本任务不自猜密码或修改用户服务器。

### 当前实现记录

开工核对HEAD `58db323bdc523132377903e83b853a08c6a981a5`、工作区干净。变更保持本项Write Set；经总控补充授权，在 `docs/verification/WI-004.md` 追加三条CI最终成功结果。部署代码为Bash，测试探针为Rust，没有自编Python/Qt。

共享gateway网络命名空间保持协调服务loopback。内部relay准入独立路由、私有bearer、有效设备/账号/登录检查和2秒数据库期限；回环准入代理限制上游连接/读写期限。公网API屏蔽/internal，relay接收限速使用1.2.0实际有效字段。API443/relay8443，监听地址与公网URL分开。首次回环Mailpit仍做真实邮箱验证；显式bootstrap管理员。六镜像离线包使用固定tag+image ID验证，全部pull never；start一起重建共享namespace依赖。证书使用Certbot5.4.0短期IP/webroot、失败可见的systemd续期和Manual证书重载/relay重启。

运行边界：仅farsail-deploy-test、回环58080/58443/58444/58026/55433，私有测试状态和镜像缓存留.local及Docker本项目卷；不改farsail-dev或其他容器。Docker构建、真实测试CA/SMTP/身份/relay及制品发布结果持续记录在 [WI-008A](verification/WI-008A.md)。

运行镜像固定 `4252e9d901e3022175772f563be45df967c3e9cc`，[发布CI 36400484173](https://github.com/wanghao9103/farsail/actions/runs/36400484173) 成功；部署加载器固定 `51e131f0bf60a3a8fa15cddd869a8b97d3db79a0`，[兼容CI 36402545193](https://github.com/wanghao9103/farsail/actions/runs/36402545193) 成功。六镜像公开归档完整匿名下载、字节数/SHA256核验及Docker29跨store导入/实际TLS+relay启动均通过。前一实现9d910e6的后端/传输/Windows CI均成功。内部网络的宿主数据库测试问题仅在test.override修复，生产库保持私网。测试容器和端口已清理，卷/私有状态保留。用户服务器CA/部署/跨NAT交总控继续，不标记文件、HEVC或手机功能完成；制品链接、摘要、命令与边界见WI-008A验证记录。

跨存储结论：classic与containerd的inspect .Id语义不同，51e131f改为核对跨存储稳定的image config SHA（包含rootfs diffIDs），并精确白名单4252的manifest/归档SHA，允许已初始化用户复用旧包而不重置秘密或重下近300MB。必须使用51e131f加载器，不能直接使用4252内原加载脚本。已初始化用户等旧load退出后fetch→checkout51e131f→load-release4252，跳过init；之后才由总控带用户申请真实证书/启动。

总控验收：最终提交 `8b46e19d8f70bda8f1511d95963466724cd08b91` 与远端main一致、工作区干净。payload CI 36400484173与loader CI 36402545193均已独立核对success；完整匿名附件及Docker29严格导入/实际启动由WI-008A留有证据。该写任务已结束。用户已在服务器初始化并通过preflight，正在按修复加载器步骤继续；真实CA/公网服务尚未声称完成。串行开始WI-008B，为第二台Windows提供可运行安装包。

## WI-008B：Windows 预览安装包

### 基线与 Write Set
代码基线 `8b46e19d8f70bda8f1511d95963466724cd08b91`；本节计划提交后的 HEAD 为开工基线。WI-008A 写任务已结束、工作区干净、测试服务已停止。总控继续指导用户在独立公网服务器执行已验证部署命令，本项不访问或修改服务器。
允许修改：`apps/desktop/**`（仅正式构建、安装、首次运行等必要修复/配置，不扩展业务功能）、`scripts/package-windows.ps1`、`scripts/test-windows-package.ps1`、`.github/workflows/windows-release.yml`、现有client工作流必要兼容、`package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock`、`.gitignore`、`README.md`、`docs/IMPLEMENTATION.md`、`docs/CLIENT.md`、`docs/WINDOWS_INSTALL.md`、`docs/verification/WI-008B.md`。若原生crate存在阻断release构建/启动的实际问题，先记录精确路径和修复，再回归，禁止无关重构。不改WI-008A已交付部署脚本、运行包、用户配置或服务器。

### 必须交付
- 基于当前 Rust/Tauri/React Windows 客户端生成真正可安装的 x64 NSIS 预览包；提供可复现打包脚本与固定提交的 GitHub Actions。沿用 FarSail v2 图标、现有应用标识和当前账号/设备/远控基线。用户无需安装Rust/Node来运行。
- 正式前端嵌入release，不依赖Vite本机端口；正确处理WebView2依赖并准确说明联网/安装要求。仅使用Tauri官方机制与已核实依赖，不用Python/Qt。不要为了打包更换整个工具链或改用户全局开发配置。
- debug IPC探针与测试环境快捷入口不得进入生产行为；私钥、长期token、设备token和grant仍只留Rust/DPAPI。首次启动不自动注册、共享、注入输入或开启无人值守；升级/卸载操作不静默清空已有用户凭据。
- 安装行为尽量按当前用户，避免默认要求系统服务/开机后台共享。不要求用户购买签名证书；若没有现成签名，明确包未签名，不能伪称可信发布者或关闭系统防护。测试安装/卸载优先在一次性Windows CI中进行；本机只在明确隔离目录/无真实凭据条件下验证本项目进程，禁止覆盖已有安装或清理用户配置。
- 产物包含安装包、源码提交/版本元数据与SHA256，发布到已授权公开仓库的明确预发布。最终应实际匿名下载公开安装包并比对摘要；不可仅凭cargo build成功或Actions绿色声称安装包可用。必要时保留便携包，但不为凑品类增加未验证格式。
- 给出第二台Windows的短步骤：下载/校验/安装、配置自己的HTTPS API和relay地址、测试收件箱验证、登录绑定、目标端开启共享/批准、查看校验码与实际relay路径、结束共享。文档参数化，不硬编码用户公网IP、邮箱或凭据。
- 明确这是当前JPEG低帧率查看/控制预览，文件/HEVC/手机后续实现；不把此项标成整个产品完成。公网用户实例的实际CA与双机验收由总控接续，不冒称完成。

### 验收与运行边界
- 锁文件安装依赖、前端typecheck/build、Rust release/NSIS打包，必要的格式/Clippy与相关回归。优先验证真实风险，不重复全后端测试掩盖安装未测。
- 核对安装包内二进制、图标、资源与版本；在一次性Windows环境验证安装/启动/卸载和真实WebView初始页/原生命令基本可用。生产启动证据与先前debug IPC证据区分，无法验证的层级须清楚说明，不加入不安全的生产测试后门。
- 如做原生输入，仍仅自建受控窗口并先核对前台；本项通常无需再重复已通过采集/输入测试。不保存或上传真实桌面像素、用户凭据或私人路径。
- 运行/缓存只用本项目 `target/`、`node_modules/`、忽略的 `.local/windows-package/` 及Tauri必要缓存；不停止其他应用，不修改全局代理/证书/防火墙。清理自启进程并记录遗留状态。
- 提交推送、核对精确远端、CI、匿名公开文件和实际摘要，更新交接与可复用知识后结束本项。后续返回WI-005文件、WI-006编解码/自适应/多屏、WI-007手机。

### 回查结论
本项窄查Tauri凭据/WebView边界与独立公开制品验收笔记：适用的是原生凭据隔离、debug/正式构建区分、原生IPC与网页预览分层，以及完整匿名制品核验；旧笔记中“远控尚未实现”的状态已被WI-004更新，不再适用。官方构建入口与WebView2选项以当前Tauri文档和锁定版本核对：https://v2.tauri.app/distribute/windows-installer/ 。

### 执行记录
开工 HEAD `2d96e3e54f3515a48fceb065d3ad65e7e2a940f1`，工作区干净。窄查两篇指定机制笔记并与现有代码核对。沿用0.1.0版本和v2图标；补上Tauri `custom-protocol` feature和release Windows GUI subsystem，官方NSIS currentUser + embedBootstrapper。首次缺WebView2时仍需联网。测试不安装到本机：仅一次性GitHub Windows runner安装，UI Automation按本测试PID/顶层HWND子树驱动普通设置页验证原生IPC/持久化，重装/静默卸载检查数据保留；不添加生产测试后门，不截图桌面。

正式源码/发布tag固定 `6d79ef8e4c8336cee45840b5c9cd812fe08910dd`，预发布 `windows-preview-0.1.0-6d79ef8`。安装包7720107字节，SHA256 `361e15a22db30be3c3009cad2da813caf81e9fe9e01b054ce6bfca848ce5c01e`。完整 [Windows安装CI 36405852502](https://github.com/wanghao9103/farsail/actions/runs/36405852502) 成功，真实WebView设置保存/重启、三次原生启动、同版重装和卸载保留数据、debug探针缺席均通过；客户端8项/桌面1项单元测试与Clippy通过。原生代码的既有Windows client CI 36404665949也成功。首轮因Tauri临时bundle type补丁造成严格EXE摘要断言失败，改用锁定CLI官方 `--no-binary-patching`（当前无updater）后保持断言并通过。四个公开附件均完整匿名下载，逐个与该CI产物比对摘要和字节数一致。

固定链接、元数据、详细证据/边界见 [WI-008B](verification/WI-008B.md)，第二台Windows步骤见 [WINDOWS_INSTALL](WINDOWS_INSTALL.md)。未签名；未模拟无WebView2系统、未验跨版本数据迁移或用户公网双机。没有本机安装/真实配置变更、没有自启进程或测试端口残留；只保留指定构建/下载缓存，Docker服务和卷未改。总控接续真实CA/公网双机验收，开发顺序返回WI-005/006/007。

总控验收：最终提交 `246a83dba70129d3d5dc5dfd73ff6c6fc02cecf5` 与远端main一致、工作区干净。安装CI已独立核对success，公开安装包及安装生命周期证据完整；WI-008B已结束。用户已获得固定安装包链接，公网服务器仍在等待镜像导入反馈，未把证书/服务启动视作已完成。继续本地WI-005，同时由总控接续用户服务器步骤。

## WI-005：双向文件传输与选择性无损压缩

### 基线与 Write Set
代码基线 `246a83dba70129d3d5dc5dfd73ff6c6fc02cecf5`，WI-008B 写任务已结束、工作区干净；本节计划提交后的 HEAD 为开工基线。总控独立指导用户服务器部署，本项不登录或改变用户服务器。
允许修改 crates/file-transfer/**、crates/core/**、crates/media/**（仅共享预算接口，不实现新编码器）、crates/transport/**、crates/client/**、apps/desktop/**、packages/ui/**、Cargo.toml、Cargo.lock、package.json、package-lock.json、scripts/test-files.ps1、.github/workflows/files.yml 及现有 CI 必要兼容、README.md、docs/IMPLEMENTATION.md、docs/FILES.md、docs/CLIENT.md、docs/TRANSPORT.md、docs/verification/WI-005.md。仅当真实文件能力/审计对接需要时修改 services/coordinator/** 与 docs/API.md，新迁移而不改历史迁移。不得写 WI-006 编码器或手机平台实现。

### 必须交付
- 使用 Rust/Tauri；自编脚本仅PowerShell/Bash/Rust，不用Python/Qt。原生可复用文件引擎和真实 Tauri 文件面板：从设备卡片独立发起，亦可从远控页建立独立 Files 授权会话；上传/下载多个任务，进度、实际速率、暂停/续传、取消、失败重试。同账号也须目标明确确认。
- 会话路由须按授权类型分发：RemoteRuntime 不应将 Files 会话当作屏幕会话启动或关闭；文件引擎与画面引擎拥有各自会话接收器，避免争抢同一 receive。
- Files 不能推导屏幕或控制权限，view/control 不允许读写文件。被控方本地选定范围和读/写权限，默认无共享。远端只接触不透明资源 ID 与允许的相对名称，不能指定任意本地绝对路径。can_files 仅在原生支持且本地启用后声明。共享开关与文件开关独立，并复用 WI-004 的操作代次/取消约束；迟到启用响应不能在停止/退出后恢复能力，关闭屏幕不应暗中撤销仍被明确允许的独立文件会话。
- 使用被授权的目录/文件句柄实施范围；避免 canonicalize+starts_with 后重新按路径打开的替换竞态。cap-std 或 Windows handle-relative 等实现须核实 reparse/junction、父目录替换、硬链接/现有文件覆盖和 ADS 等边界。接收使用新建临时文件，无静默覆盖，无自动打开/执行。
- 有界分块协议、序号、版本、传输 ID、偏移、原始/编码长度、none/zstd 协商与每块/最终 SHA-256；流式读写，无全文件内存加载。文件元数据、块顺序和提交确认有界且可靠；实际通过现有认证 iroh 通道，不以本地复制伪装传输。
- 选择性 Zstd 无损压缩在加密前、接收解密后执行，压缩无收益直通；已压缩内容默认跳过，可压缩文本实测。每块独立，无跨消息字典；限制原始长度、压缩窗口、累计磁盘/内存和并发任务。受限工作线程执行压缩/哈希/文件 I/O，不能阻塞 Tokio 输入或 UI。
- 暂停/断线保存已验证进度，重新授权后才允许续传；进度绑定账号/双方设备/资源/源内容版本。用稳定源快照或合适持有句柄防止不同版本拼接，恢复须核实已存块与源一致。最终 hash 验证、同卷无覆盖提交和对端 commit ack 后才标记完成；磁盘满/损坏/取消不留下伪成品。恢复状态不含长期令牌。
- 文件限速可配置，输入/心跳和交互画面优先。多任务/文件与媒体受共享总预算约束，降低后台流量，不能只靠不同 QUIC streams 宣称优先级。慢消费者始终有界，撤权/退出/结束全部及时停止读写，不复活旧排队任务。
- 保持已发布账号/设备/查看控制路径及已存凭据兼容；新的能力更新不得重置另一种共享开关。对旧部署包不支持的新API给出明确升级提示，不伪造can_files。旧Windows预览包/服务器镜像仍是各自固定版本，不改写既有发布附件；源码功能与已发布版本需区分。
- 保存到 Windows 用户明确选取位置；为移动文档提供器设计句柄接口，但本项不声称移动端已完成。审计仅必要元数据，不记录正文、敏感完整路径或访问凭据。

### 验收与运行边界
- 独立 Files 会话和远控中独立 Files 授权，实际两端 iroh 上传/下载、多文件、空文件、中文名、大于内存窗口的文件；本地直连及强制 TLS relay 至少一条文件内容往返。
- 真实服务/客户端授权负向：未授权/只读/只写/过期/撤销/换账号拒绝；范围逃逸、重解析点/目录替换、同名无覆盖、错误 offset/size/hash/codec/window、压缩炸弹/截断和取消/空间不足/commit ack 丢失对应状态。
- 断线重新确认后续传、源变化拒绝拼接，停流和过期任务不能续写。限速文件+媒体同时传输测量输入延迟/队列边界，记录实测数字与条件，不承诺固定压缩比。
- 合成或项目公开资产作为文件内容，不读传任意私人文件；测试目录与任务存储忽略。仍只用 farsail-dev 本机数据库/邮件及本地 TLS relay，无公网部署。
- fmt/tests/clippy、前端 typecheck/build、Windows Tauri 构建及相关既有回归/CI；记录自启进程/端口并收尾。更新文档、知识沉淀、提交推送核对远端后交接。
### 回查与依赖线索
已回查本项目短租约传输和媒体取消边界笔记：每次业务操作需重查租约/代次，等待队列有界，退出先停本地IO，身份切换不能接受迟到结果；把该机制扩展到文件操作仍须专项验证。cap-std 4.0.3 Dir/from_std_file 文档要求Windows句柄无FILE_SHARE_DELETE以避竞态；需实际核对所选API的链接和无覆盖提交语义。Zstd Decompressor提供输出容量/参数限制，应同时限制原始长度和压缩窗口。参考 https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html 、https://docs.rs/zstd/latest/zstd/bulk/struct.Decompressor.html 。库版本与Windows行为以当前构建/测试为准，不能只靠API名称推定安全。

## WI-008C：部署下载断点续传修复

### 触发、基线与写入交接
用户在公网机器从GitHub下载约284MiB归档，运行40多分钟后遇到curl92/HTTP2 PROTOCOL_ERROR。现有下载器把数据写入.part，但重新运行会从头覆盖，缺少可靠续传。总控已提供使用本机校验过的同一公开包通过scp上传、load-release离线目录导入的即时方案。
代码基线 `69b2817ff39bb0b9220e45affce7a972561951f0`，本节计划提交后为开工HEAD。WI-005已在原子编辑后明确quiescent，无构建/测试进程，暂停所有写入与提交，保留未提交WIP：Cargo.toml/Cargo.lock、crates/file-transfer/**、crates/client/src/lib.rs、crates/transport/src/lib.rs、services/coordinator/src/device.rs、apps/desktop/src-tauri/Cargo.toml和src/files.rs/src/remote.rs。这些内容未验收，严禁暂存、提交、改写或发布。本项结束由总控明确交回WI-005写权限。
精确Write Set：scripts/deploy/farsail.sh、新增scripts/deploy/download相关Bash/Node测试与辅助文件、.github/workflows/download.yml、docs/DEPLOYMENT.md、docs/verification/WI-008C.md、docs/IMPLEMENTATION.md。不改Cargo/应用业务/镜像内容/旧发布附件，不重编镜像，不登录用户服务器。

### 必须修复与验收
- 生产下载显式使用HTTP/1.1、正常HTTPS和有限连接/无进展超时；每次重试从现存.part长度重新发起Range请求，保留失败和中断前的新增进度，避免curl内层重试把本轮进度回退。对永久HTTP/证书/Range错误给出清楚失败，不绕过TLS，不无限循环。
- 利用已知SHA256先识别已完整下载的.part；即使上次网络收尾报错，校验匹配也能直接采用而不请求网络。未知/损坏内容不得推广为最终文件；摘要不符保留证据并失败，不偷偷删除重下。
- 小型SHA256SUMS/manifest与大归档分别处理，先取得有效校验信息，再续传和验证大文件；支持原缓存和已初始化state，无需init。保留现有legacy425精确白名单、portable config SHA/rootfs/平台等导入检查；source_dir离线导入继续完全不访问网络。
- 有意义的本地网络故障测试：可协商HTTP2的测试服务确认客户端使用HTTP1.1；主动中断后下一请求Range偏移前进、最后字节与摘要一致；完全.part在断网时直接采用；损坏摘要不接受；服务器拒绝Range不破坏已下载部分；有限重试和TLS拒绝。用Node/标准工具和测试CA，不用Python/Qt，不接触用户文件。测试监听仅回环，结束清理自己的进程。
- 对公开原425附件做小型匿名HTTP1.1/Range检查即可；本机已有完整公开包，不重复下载/发布整个近300MB归档来验证一个下载器。必要集成使用现有缓存和严格校验，不能因File WIP运行Cargo或混入未完成代码。
- 更新操作说明：直接本机上传三个已验证文件的备选路径、旧.part复用、故障信息/进度。不得承诺200Mbps标称带宽等于GitHub实际下载速度。
- 只暂存上述精确文件，确认File WIP未进入提交；提交/推送固定修复版本、对应CI/证据及知识沉淀。提供保留现有私有配置与缓存的更新命令。完成后停止本项写入，通知总控恢复WI-005。

### 适用经验
已知GitHub TLS/EOF经验仅支持按实际连接路径诊断、使用命令级选项、保持证书验证；不能把所有失败归因于同一种网络原因。此次有实际curl92和下载器源码缺少-C/外层续传的证据。原六镜像包已完整匿名下载并通过SHA/运行验证，问题是交付可靠性，不能用关闭校验或要求用户再次从头下载来掩盖。

## UI-011 — 0.1.1 客户端交互修复（2026-09-29）

用户验收反馈已收拢：深色设备列表/详情面板、本机共享与入站批准、远端直接 control/view 请求及批准后自动连接、注册验证连续流程、中文登录失败弹窗、自定义窗口栏。源码 `9ad6175`，Windows installer CI `36511486164`、Windows client CI `36511468941` 均通过。已发布 `windows-preview-0.1.1-9ad6175`，四个公开附件匿名下载校验一致。详情见 `docs/verification/WI-UI-011.md`。

本次仅发布客户端 UI 和交互修复。WI-005 文件传输未提交 WIP 保留，仍不在发布包中；WI-008C 下载续传修复仍待实施。公网生产服务器无需因本次客户端更新重部署。下一步真实双机验收不能由合成 IPC 测试替代。

## REMOTE-012：0.1.2 远控可靠性与独立窗口

在独立工作区基于 `024eb05` / 计划 `01817f0` 实施。包含租约读取竞争、乱序输入、焦点和断线队列清理、有界媒体重试、原生独立查看窗口及会话范围 IPC、全新授权的可取消重连、本机默认关闭的“远程值守”。服务端 routes/migrations 与 4252 发布包兼容，无服务端升级要求。真实数据库/HTTP/QUIC 已通过多轮 30 秒租约的 70 秒保持测试，用户现场约 30 秒断开的唯一根因尚未证实；缺口和最终发布证据见 [REMOTE-012](verification/WI-REMOTE-012.md)。WI-005 WIP 和 WI-008C 状态保持。

交付完成：源码 `3519613`、`c65fb77` 已推送；最终 Windows 安装 CI `36517620603`、Windows client `36517618553`、transport `36517618586`、backend `36517618587` 全部 success，均固定 `c65fb775b50af8345d0f3716d566b233a683f060`。预发布 [windows-preview-0.1.2-c65fb77](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.2-c65fb77) 四个附件完整匿名下载核验一致。安装包 7,843,396 字节，SHA256 `016074d1f492913b138f92286fab6f97f48a307e6ce59a9d55eb96bc07c0ae33`。原生输入、窗口 IPC 隔离/生命周期、本地真实采集与跨多次租约连接均通过；物理双机和跨 NAT 缺口未冒称完成。运行时已收尾，知识笔记已更新，详细后续入口为 [最终交接](REMOTE-RELIABILITY-HANDOFF.md#completed-delivery--coordinator-entry-2026-09-29)。

## UI-012/013/014 — 0.1.3 桌面操作与计算机名称交付

在独立 desktop-ui-release 工作区，以远端 `5f8fb54` 为基线整合本轮布局、操作流程、名称读取，保留 0.1.2 独立原生窗口、输入取消、有限重连和显式远程值守。源码 `0987d42`、最终修复 `f2f5c6b` 已推送。最终 Windows client `36527793606`、安装 CI `36527795316` 均通过；首次安装验收发现的紧凑侧栏共享状态不可访问已修复，未跳过断言。

已发布 [0.1.3](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.3-f2f5c6b)，四个附件均完整匿名下载并与 CI 原件核验一致。安装包 7,853,825 字节，SHA256 `dc8f43cc79af6d1b24757d217bd502653aef9b6e27bd1df258c1a851443ae258`。无需服务端更新；两台客户端分别更新后上报各自计算机名称。完整证据与后续入口见 [UI-RELEASE-013](verification/WI-UI-RELEASE-013.md)。原始共享目录仍保留旧基线与文件传输 WIP，后续 UI/发布工作应复用当前发布工作区，不能将旧目录覆盖到远端新版。运行时已停止，未覆盖真实安装/配置。

## INPUT-014 — 普通鼠标操作后断开修复（2026-09-30）

用户确认普通应用内移动/点击鼠标会断开，旧截图显示 input_failed。以 0.1.3 为基线，在当前发布工作区实现虚拟桌面绝对 SendInput、普通系统按键支持，以及输入拒绝时暂停控制但保留媒体的失败边界。输入状态采用有界单调代次；明确重试与普通释放分离，保留授权、共享、会话取消和受保护桌面的既有边界。

源码 `40edacb2e17b1ac1d2dc04baa3457135fcb831e2` 已推送并发布 [0.1.4](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.4-40edacb)。本机自建前台窗口真实输入、GUI-less 工作线程定位、认证 loopback 暂停/媒体继续/恢复、两组浏览器回归及静态检查通过；最终 Windows client 36660400761、installer 36660402240、transport 36660400809、backend 36660400832 全部成功。四个附件完整匿名下载校验一致，安装包 7,849,691 字节，SHA256 `7cbec092f01f7ee6f05d15fea1fcf4c163f4c5b0e41aeebdfb9b592e8186322b`。

两台电脑都需要更新，尤其被控端；服务端无需更新。用户机器原始 Windows 拒绝的唯一根因仍未复现，不能把旧截图归因为 UAC。新版提供具体 Windows 返回码，并避免这类输入拒绝直接关闭视频。证据与后续入口见 [INPUT-014](verification/WI-INPUT-014.md)。本机运行时已关闭，生产安装/配置与原目录的文件传输 WIP 未改。

## VIEWER-015 — 大画面、隐藏工具栏和高清切换（2026-09-30）

在当前发布工作区以 0.1.4 为基线，远程窗口默认最大化，工具栏改为浮动自动隐藏，文字与连接详情按需展开；传输分辨率支持 720p/1080p，默认高清并改善色度压缩，像素/单帧/带宽预算不扩大。实际接收尺寸可查，原比例和鼠标留边定位保留。两端更新可用完整画质切换，无服务端更新。

源码 `cf5b72aadf4854fe536fa6e4155ccc3585357da2` 已推送并发布 [0.1.5](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.5-cf5b72a)。实际原生默认最大化/还原/全屏/关闭隔离、DXGI 内存采集、编码色彩边缘、浏览器全客户区/隐藏/定位/切换及既有回归通过。Windows installer 36666218872、client 36666217848、transport 36666217772、backend 36666217762 全部成功。四个公开附件匿名下载核验一致，安装包 7,871,208 字节，SHA256 `d6d2871952212339754c599336f20159d8f015c8861d86d3bddbaf5c579e0f9c`。详见 [VIEWER-015](verification/WI-VIEWER-015.md)。测试运行时已关闭；后续复用当前管理工作区，不将旧主目录覆盖到已发布源码。
