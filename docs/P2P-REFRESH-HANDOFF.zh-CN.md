[English](P2P-REFRESH-HANDOFF.md) | **简体中文**

<a id="p2p-019-refresh-discovery-without-restarting-clients"></a>

# P2P-019：不重启客户端刷新地址发现

<a id="authority-and-workspace"></a>

## 授权与工作区

- 用户确认服务端QAD=true、`[tls].quic_bind_addr=0.0.0.0:7842`、Docker UDP映射和真实中继socket。云规则此前误设TCP，修正UDP并完整重启两端后已直连（用户截图RTT16ms、4.9FPS）。用户问连接/远控为何不重新发现直连；应实现适当有界刷新，不能说没有P2P。
- 基线 `392b88d7390c76155057f5d9ab1a297200ca7652`（已验证0.1.8），托管目录 `C:/Users/wangy/.codex/worktrees/desktop-ui-release/远程`，新分支 `codex/p2p-refresh-019`。父任务派发前提交交接并停止写入；在此目录继续，不用项目默认目录。
- 主目录 `D:/working/远程` 有旧基线及暂停的Cargo/文件传输WIP，不修改、还原、提交或合并。此托管目录已关联父任务，不另建重复检出/工作树，不覆盖主目录。
- 用户全局AGENTS授权同项目长任务接力；除另行明确授权，不使用子代理。不SSH服务端、不改正式配置、不采真实桌面截图、不输出环境/token或消息到其它聊天。原有推送/新安装包授权持续；把0.1.9结果推进到精确CI制品及匿名公开摘要检查。

<a id="facts-vs-open-questions"></a>

## 已知事实与未决问题

- FarSail当前真实直连认证帧/续期/撤销测试再次通过。桌面forceRelay默认false、bind为0.0.0.0:0，iroh使用Minimal加自建中继，关闭公开查询和portmapper不等于关闭QAD。前端路径状态来自QUIC实时所选路径。
- 独立QAD探针位于忽略的 `.local/qad-probe`，固定Iroh1.2.0及仓库锁基线：UDP7842 QAD IPv4往返true、发现全局IPv4 true、HTTPS探测true，TLS验证 `/ping` 返回200。无服务回环反例全false。没有账号/凭据/持久身份，不打印地址/密钥。目标URL由用户提供，不公开。源码/证据在 `.local/qad-probe`，可执行 `target/debug/farsail-qad-diagnostic.exe` 返回时已关闭。
- 原生主心跳每25s重新公布endpoint.addr()，但不强制新发现。`NativeClient::connect_transport` 当前下载对端地址后连接，没有先刷新本地发现。
- 用户重启结果证明当前这对设备/网络可直连。精确陈旧缓存因果链仍是推断，未埋点；不保证所有NAT可穿透，也不说完全没有周期QAD。
- Windows ActiveStore防火墙查询被系统权限拒绝，不提权/改规则绕过。

<a id="version-specific-api-findings"></a>

## 版本特定API发现

读取此主机缓存的Iroh1.2.0源码与官方文档，不依赖最新版API记忆。

- Endpoint::network_change()只是通知netwatch。云防火墙规则变化不是本机接口/路由变化，单用此API不一定触发完整新QAD报告。
- socket.rs约每20–26s报告网络，`net_report.rs` 每5min完整报告，其它增量；`UpdateReason::RelayMapChange` 被视为重大变化。
- 公开Endpoint::insert_relay(url,Arc<RelayConfig>)替换已有映射并无条件发送RelayMapChange。重新插入完全相同配置可作为调度完整QAD报告的候选，不重建端点/身份，不断会话或移除中继；使用前仍需受控可执行回归。不得删后重加中继、把独立新探针UDP地址抄到活端点，或批准grant后重启传输（停止会使grant失效）。
- Endpoint::watch_addr()稳定，可及时公布地址变化。Endpoint::net_report()受unstable-net-report限制，只在必要开发测试启用，不随意把不稳定诊断加入正式包。
- 新报告到达后socket.handle_net_report_report通知传输并更新直连地址，现有QUIC路径可动态升级。
- iroh-relay::server::{Server,ServerConfig,RelayConfig,TlsConfig,QuicConfig}支持普通TLS认证中继和独立QAD，QuicConfig.server_config接受rustls::ServerConfig。测试CA须真正信任，不接受全部证书。

<a id="work-item-and-exact-write-set"></a>

## 工作项与精确写集

实现P2P-019，目标发布0.1.9：源端/入站新连接前有界刷新，适当的仅中继会话重试和/或显式重试命令，及时公布新本地地址。保留端点身份/凭据、会话/grant TTL、16会话/8握手限制、输入取消、强制中继覆盖及发现失败时继续媒体。优先共享刷新锁/冷却，不让状态轮询触发网络风暴；不反复重建传输或关闭良好直连。

写集：`crates/transport/src/lib.rs`、必要时仅开发测试net-report特性的 `crates/transport/Cargo.toml`、`crates/client/src/lib.rs`，以及实现显式状态/操作所需的最窄原生IPC/viewer/前端/CSS/浏览器测试；`apps/desktop/src-tauri/tauri.conf.json`版本、专项脚本、docs/verification/WI-P2P-019.md、docs/TRANSPORT.md和README/WINDOWS_INSTALL/IMPLEMENTATION发布文档。扩展前在此记录具体原因；不改服务端/数据模型/编码器/无关文件传输。

<a id="required-verification"></a>

## 必须验证

1. 受控真实QAD失败→服务响应→同端点刷新→QAD成功，不重启端点/客户端。用随机回环端口（仅测试relay map QUIC覆盖）与临时可信CA；保持认证活会话持续传数据证明不拆连接，不只断言API被调用。服务可先占用/丢弃预留UDP端口，再在同端口启动QAD。
2. 强制中继与无中继跳过刷新，冷却/单飞，关闭/生命周期取消，旧地址代次和批准保留；现有撤销/TTL与输入拒绝继续验证。
3. 构建/fmt/合适Rust测试+Clippy -Dwarnings；触及UI时验证真实原生viewer隔离及浏览器套件。测试/辅助运行时只拥有回环/隔离进程，不打断真实应用。
4. 用户进度区分尝试直连、当前中继回退、发现/刷新失败与强制中继。不从单个QAD服务伪造NAT诊断，不公开端点/IP/token。
5. 工作源码在此任务前无改动，只有本交接新增。测试缓存放target/node_modules/.local，结束自有进程。

<a id="existing-buildrelease-workflow"></a>

## 已有构建/发布流程

- 内置Playwright NODE_PATH：`C:/Users/wangy/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules`。
- apps/desktop下显式Vite命令：`node ../../node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1420 --strictPort`，先前PowerShell经npm转参丢了--host。
- 浏览器 `node scripts/test-desktop-ui.cjs`、`node scripts/test-remote-viewer.cjs`；原生 `cargo build --locked -p farsail-desktop`、`./scripts/test-native-viewer.ps1`。原生烟测用独立配置并拥有进程。
- 常规Rust专项可在一个统一特性命令中包括client/transport/desktop/media/windows，避免多次全重编；输入/DXGI ignored测试仅在确认自有HWND/PID/内存时运行，不作用用户桌面。
- `git push origin HEAD:main`（刷新远端、不force、限定暂存），再 `gh workflow run windows-release.yml --ref main`；等待全部相关run固定源码SHA。不本地正式安装；临时Windows CI验证真实安装/设置重启/重装/卸载/调试探针缺席。
- 旧 `.local/publish-018.ps1` 校验精确源码run/版本/摘要并发布4个CI原件。019辅助使用真实commit/runID/tag/version，不发布本地debug构建。`.local/release-notes-018.md` 是旧说明模板。
- 匿名curl下载4附件，正常验证TLS、HTTP1.1、有限45s期限；与CI原件比较尺寸和SHA256，仅重试失败且可续已知部分文件。成功公开后记录证据表、更新README/WINDOWS_INSTALL/IMPLEMENTATION，文档另提交。
- 最近已发布0.1.8标签windows-preview-0.1.8-0f43f10，源码0f43f106f31e6065e6c2c45569d27f64930ab695，main文档基线392b88d。
- 实现前窄查知识（父任务已使用技能），只把已验证复用结果写Obsidian inbox，优先更新同机制旧笔记，无秘密/私有IP。相关：设备/传输短租约，离线镜像/共享网络/QAD运维；其中物理跨NAT缺口是历史，用户现在确认当前直连，不抹其它边界。

<a id="handoff-status"></a>

## 交接状态

P2P-019尚未改产品，候选刷新API还没验证。父任务在本交接提交后停止写入；先前诊断进程均结束，只剩探针源码/结果和缓存。接手此写集推进到验证交付，不重复询问已授权的常规操作。

<a id="completed-successor-delivery-2026-09-30"></a>

## 后继交付完成（2026-09-30）

P2P-019源码 `5c261c5578635bbe6174b209ff6bfbb0b0d588f6` 实现原端点有界刷新、中继会话维护、及时地址公布和公开调度状态。本地回归、精确源码CI/安装及4个匿名制品摘要通过；[0.1.9发布](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.9-5c261c5)。最终证据、写集和物理跨NAT迁移缺口以 [WI-P2P-019](verification/WI-P2P-019.zh-CN.md) 为准。全部自有测试进程已停，未改主WIP/服务/正式配置/凭据，未发其它聊天消息。在此已有托管目录继续，不回旧主目录。
