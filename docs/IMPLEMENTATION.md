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
| WI-001 | Rust workspace、PostgreSQL 账号/设备/授权服务、隔离本地运行与权限测试 | ready |
| WI-002 | Tauri 客户端登录、设备注册/列表、凭据安全存储、用户/管理员界面 | planned |
| WI-003 | iroh 端到端连接、应用授权、直连/中继配置与撤权 | planned |
| WI-004 | Windows 屏幕采集、用户确认、鼠标键盘输入与可用远控基线 | planned |
| WI-005 | 分块双向文件传输、无损压缩、校验/续传、授权与限速 | planned |
| WI-006 | 实际硬编/解码、多屏与自适应速率、能力协商和 4:4:4 路径 | planned |
| WI-007 | Android/iOS 手机控制与文件接口、可执行平台构建和验证 | planned |
| WI-008 | 全链路回归、部署/打包、公开仓库与真实环境验证 | planned |

任何工作项可以因实测依赖拆成更小的连续项，但不得把尚未实现的范围标成完成。手机被控、无人值守和其他后续扩展仍按原设计作为后续路线。

## WI-001：Rust 账号与设备服务

### 基线与 Write Set

代码基线为初始提交；工作启动提交将额外包含本实施文档。开工时在本节记录实际 HEAD。

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

尚未开始代码实现。
