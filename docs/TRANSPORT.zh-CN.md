[English](TRANSPORT.md) | **简体中文**

<a id="farsail-authenticated-transport-wi-003004"></a>

# FarSail 认证传输（WI-003/004）

`crates/transport` 使用 iroh / iroh-relay 1.2.0 的加密 QUIC。端点密钥直接读取客户端已用来注册设备的 Ed25519 私钥；连接的公钥必须与授权记录中的 source/target 公钥一致。协调服务只负责受限地址发现及 grant 校验，不转发业务帧。Tauri WebView 只收到会话状态、所选路径和 RTT，不能读取设备密钥、设备 token 或 grant。

<a id="usage"></a>

## 使用流程

1. 客户端登录、绑定设备并进行带代次心跳。`NativeClient::start_transport` 从安全存储取同一设备密钥，绑定端点，向协调服务发布 `EndpointAddr`；心跳后更新地址。默认关闭端口映射与公网地址查询服务。`TransportConfig` 默认明确绑定 `127.0.0.1:0` 供本地测试；桌面设置可选择 UDP 绑定地址、自建 HTTPS relay 及强制 relay。
2. 发起端申请 `view`、`control` 或 `files`，目标端明确批准。目标原生内存保留唯一可获得的 grant。发起端在批准后以本机设备凭据及心跳代次查询对端地址。地址只返回会话另一参与设备，且双方设备、登录与租约仍有效。
3. 发起端对目标端注册公钥建立 iroh 连接。目标通过连接的认证公钥校验发起端，才在加密双向流中交付 grant；不会从状态轮询泄漏。发起端以自身设备凭据、代次和 grant 向协调服务检验会话 ID、精确权限、双方公钥、nonce 与剩余 TTL，随后回复 nonce 证明。目标验证回复后才开放数据流。握手每步限时 10 秒、消息限 4 KiB；目标同时处理至多 8 个握手，并缓存至多 4096 个会话 ID 拒绝重放。
4. 已认证的 `Session` 提供 `send(Frame)` / `receive()`、`permission()`、`path()`、`is_open()` 与 `close()`。协议版本字节为 1；帧类型独立为 `Control`、`Media`、`File`，上限分别为 64 KiB、1 MiB、256 KiB。`control` 只允许 `control` grant，`media` 允许 `view`/`control`，`file` 只允许 `files`。WI-004 在前两者上接入 JPEG 画面与 Windows 系统输入；文件内容/续传仍由 WI-005 实现。
5. 目标每 15 秒从服务续期并在已认证连接中交付新 grant；双方约每 5 秒重验。服务返回的剩余 TTL 与 HTTP 开始时的本地 `Instant` 构成保守截止时刻，独立到期任务会关闭连接。撤权、禁用、解绑、登录退出、心跳代次变化、服务不可达或 grant 失效不再允许超出本地截止时刻的业务帧。目标续期旋转 token 与发起端收到更新之间有短暂窗口；发起端最多等待更新消息，但不会延长旧租约。
6. 登出和本地断开先关闭会话与端点，已建立会话的本地 grant 立即作废，并异步尽力通知服务撤销。端点重启后不能用原会话的旧批准重连，须重新申请、批准；尚未使用的有效批准在首次启动端点时保留。并发启动、连接的结果须匹配原生生命周期代次才会写入当前会话；旧结果会关闭。客户端最多保留 16 个活跃会话，关闭后自动移除。若断线时协调服务不可达，远端撤销会等待短租约到期，本地不会继续放行旧会话。

媒体流读写使用 400 ms 期限。超时或 reset 的不完整媒体帧被丢弃，下一条流仍可处理；控制消息持续接收，避免卡在过期大图上。Windows viewer 每收到并验证一张 JPEG 就返回应用层序号确认；主机只允许一张未确认图片，1 秒未确认便停流并关闭该会话。`tx.finish()` 只是本地写完成，不代表对端已消费。采集侧只保留最新帧，按 1.5 MB/s JPEG payload 令牌桶节流并在等待后重新取最新图，过期 700 ms 的图片不发送。实际链路字节会包含协议头和重传。该策略针对当前 JPEG 基线；WI-006 换用视频编码时要重新测量时延、丢帧和带宽。具体媒体及输入字节契约见 [REMOTE.md](REMOTE.zh-CN.md)。

`Session::verification_code()` 从 QUIC/TLS exporter 结合会话 ID、精确权限及双端设备公钥派生双方一致的短码供人工核对，不使用或展示 grant token。RTT 仅指所选传输路径。

<a id="paths-and-relay"></a>

## 路径和 relay

`Session::path()` 读取 iroh 当前 **selected path**，显示 `direct`、`relay` 或 `connecting` 及该路径的 RTT。配置 relay URL 只是允许使用该服务器，并不证明数据实际经过它。`force_relay` 调用 `clear_ip_transports()`，本机测试确认双方无 IP transport 且 selected path 为 `relay`。本地测试绑定 `127.0.0.1`，不访问默认公共 lookup 或公共 relay，不修改防火墙。relay URL 必须为无凭据、查询、片段的 HTTPS，且协调服务只接受 `FARSAIL_RELAY_URLS` 名单中的 URL；客户端也拒绝拨打与本机配置不一致的 relay。TLS 使用标准证书验证，本地测试显式添加临时证书根，不存在 accept-all verifier。

自建服务器示例见 [deploy/relay](../deploy/relay/README.zh-CN.md)。公网 IP 可以作为 HTTPS 主机名使用，但证书必须匹配该 IP。公网部署、跨 NAT 穿透率、第二台真实 Windows、移动真机和长期链路性能仍待 WI-008 验证。

<a id="address-refresh-without-restart-p2p-019--019"></a>

## 无需重启的地址刷新（P2P-019 / 0.1.9）

新发起/入站连接在当前端点请求完整地址发现；已认证中继会话每 30 秒检查重试，端点级锁与 30 秒冷却合并并发请求。地址变化经 watcher 及时发布，上传核对生命周期和心跳代次并串行处理，避免延迟旧地址覆盖新端点。状态轮询不会触发网络刷新，直连会话不会单独触发定时重试。刷新不重建端点，不清空批准，不停画面；强制中继和未配置 relay 时跳过。界面显示当前中继传输、尝试直连、定时重试或强制中继，刷新请求成功不等于 QAD 或打洞成功。

Iroh 1.2.0 通过原样重新插入已配置 relay 触发 major/full 网络报告。真实回环 QAD 失败→恢复、认证会话不断流、地址代次及冷却专项见 [P2P-019](verification/WI-P2P-019.zh-CN.md)。跨 NAT 的既有会话升级仍需要双机实际验收；不保证所有 NAT 都能直连。

<a id="future-interfaces"></a>

## 后续接口

WI-004 的 Windows 桌面初始绑定 `can_host=false`，本地开启共享且 DXGI 探测成功后才通过带代次 API 更新为 true。WI-005 接入 `File` 类型、分块哈希、续传和限速，并在实际功能可用时才设置 `can_files`。批准操作仍逐次发生，不自动开启本机共享。
