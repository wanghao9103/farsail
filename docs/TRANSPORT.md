# FarSail 认证传输（WI-003）

`crates/transport` 使用 iroh / iroh-relay 1.2.0 的加密 QUIC。端点密钥直接读取客户端已用来注册设备的 Ed25519 私钥；连接的公钥必须与授权记录中的 source/target 公钥一致。协调服务只负责受限地址发现及 grant 校验，不转发业务帧。Tauri WebView 只收到会话状态、所选路径和 RTT，不能读取设备密钥、设备 token 或 grant。

## 使用流程

1. 客户端登录、绑定设备并进行带代次心跳。`NativeClient::start_transport` 从安全存储取同一设备密钥，绑定端点，向协调服务发布 `EndpointAddr`；心跳后更新地址。默认关闭端口映射与公网地址查询服务。`TransportConfig` 默认明确绑定 `127.0.0.1:0` 供本地测试；桌面设置可选择 UDP 绑定地址、自建 HTTPS relay 及强制 relay。
2. 发起端申请 `view`、`control` 或 `files`，目标端明确批准。目标原生内存保留唯一可获得的 grant。发起端在批准后以本机设备凭据及心跳代次查询对端地址。地址只返回会话另一参与设备，且双方设备、登录与租约仍有效。
3. 发起端对目标端注册公钥建立 iroh 连接。目标通过连接的认证公钥校验发起端，才在加密双向流中交付 grant；不会从状态轮询泄漏。发起端以自身设备凭据、代次和 grant 向协调服务检验会话 ID、精确权限、双方公钥、nonce 与剩余 TTL，随后回复 nonce 证明。目标验证回复后才开放数据流。握手每步限时 10 秒、消息限 4 KiB；目标同时处理至多 8 个握手，并缓存至多 4096 个会话 ID 拒绝重放。
4. 已认证的 `Session` 提供 `send(Frame)` / `receive()`、`permission()`、`path()`、`is_open()` 与 `close()`。协议版本字节为 1；帧类型独立为 `Control`、`Media`、`File`，上限分别为 64 KiB、1 MiB、256 KiB。`control` 只允许 `control` grant，`media` 允许 `view`/`control`，`file` 只允许 `files`。本项提供数据接口；画面采集/编码、输入处理和文件内容/续传由 WI-004/005 实现。
5. 目标每 15 秒从服务续期并在已认证连接中交付新 grant；双方约每 5 秒重验。服务返回的剩余 TTL 与 HTTP 开始时的本地 `Instant` 构成保守截止时刻，独立到期任务会关闭连接。撤权、禁用、解绑、登录退出、心跳代次变化、服务不可达或 grant 失效不再允许超出本地截止时刻的业务帧。目标续期旋转 token 与发起端收到更新之间有短暂窗口；发起端最多等待更新消息，但不会延长旧租约。
6. 登出和本地断开先关闭会话与端点，已建立会话的本地 grant 立即作废，并异步尽力通知服务撤销。端点重启后不能用原会话的旧批准重连，须重新申请、批准；尚未使用的有效批准在首次启动端点时保留。并发启动、连接的结果须匹配原生生命周期代次才会写入当前会话；旧结果会关闭。客户端最多保留 16 个活跃会话，关闭后自动移除。若断线时协调服务不可达，远端撤销会等待短租约到期，本地不会继续放行旧会话。

## 路径和 relay

`Session::path()` 读取 iroh 当前 **selected path**，显示 `direct`、`relay` 或 `connecting` 及该路径的 RTT。配置 relay URL 只是允许使用该服务器，并不证明数据实际经过它。`force_relay` 调用 `clear_ip_transports()`，本机测试确认双方无 IP transport 且 selected path 为 `relay`。本地测试绑定 `127.0.0.1`，不访问默认公共 lookup 或公共 relay，不修改防火墙。relay URL 必须为无凭据、查询、片段的 HTTPS，且协调服务只接受 `FARSAIL_RELAY_URLS` 名单中的 URL；客户端也拒绝拨打与本机配置不一致的 relay。TLS 使用标准证书验证，本地测试显式添加临时证书根，不存在 accept-all verifier。

自建服务器示例见 [deploy/relay](../deploy/relay/README.md)。公网 IP 可以作为 HTTPS 主机名使用，但证书必须匹配该 IP。公网部署、跨 NAT 穿透率、第二台真实 Windows、移动真机和长期链路性能仍待 WI-008 验证。

## 下项接口

WI-004 接入 `Session::send/receive` 的 `Media`/`Control` 类型，在批准主机侧明确确认采集/输入后，才能把注册设备的 `can_host` 设为 true。WI-005 接入 `File` 类型、分块哈希、续传和限速，并在实际功能可用时才设置 `can_files`。当前桌面绑定始终声明两者为 false；普通用户的批准操作不会自动共享画面或文件。
