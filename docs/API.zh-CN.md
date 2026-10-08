[English](API.md) | **简体中文**

<a id="farsail-coordinator-api-v1"></a>

# FarSail 协调服务 API v1

协调服务通过 HTTP 传输 JSON。本地服务绑定 `127.0.0.1:8787`；远程部署必须在回环监听器前配置验证 TLS 的反向代理。客户端必须使用可配置的 HTTPS 基础 URL，并验证服务端证书。此 API 不传输屏幕、输入或文件字节。

所有标识符均为 UUID 字符串。认证令牌和邀请码是不可解析的 256 位随机值，仅返回一次，并以 SHA-256 摘要形式存储。用户接口设置 `Authorization: Bearer <access_token>`。设备专用接口设置 `Authorization: Bearer <device_token>`。远程请求使用用户 Bearer，加上 `X-Farsail-Device-Token: <source_device_token>`。检查授权时使用授权 Bearer，并在该请求头中提供参与设备的令牌。切勿将凭据放入 URL、日志或 WebView localStorage。

错误使用 HTTP 状态码和 `{ "error": "..." }`：400 表示输入格式错误或已过期，401 表示凭据缺失或无效，403 表示已认证但不允许，404 表示资源超出权限范围，409 表示状态冲突或已过时，429 表示触发限流。大多数成功的变更请求返回 `200`，正文为 JSON 或空。`GET /healthz` 检查 PostgreSQL，就绪时返回 `200`。

<a id="account"></a>

## 账户

| 方法与路径                           | 请求体 / 结果                                                                    | 凭据                             |
| ------------------------------------ | -------------------------------------------------------------------------------- | -------------------------------- |
| `POST /v1/auth/register`             | `{email,password,invite_code?}` → `{id}`；密码为 12–1024 字节                    | 无；`invite_only` 模式需要邀请码 |
| `POST /v1/auth/verify`               | `{token}`；消耗邮箱验证令牌                                                      | 无                               |
| `POST /v1/auth/verify/resend`        | `{email,password}`；为未验证账户重新发送验证邮件                                 | 无                               |
| `POST /v1/auth/login`                | `{email,password}` → `{access_token,refresh_token,session_id,access_expires_in}` | 无                               |
| `POST /v1/auth/refresh`              | `{refresh_token}` → 新令牌对；重复使用会撤销会话                                 | 刷新令牌                         |
| `POST /v1/auth/logout`               | 撤销当前登录及其远程会话                                                         | 用户                             |
| `POST /v1/auth/password`             | `{old_password,new_password}`；撤销账户的所有会话和远程授权                      | 用户                             |
| `POST /v1/auth/recovery/request`     | `{email}`；统一返回成功，账户存在时发送一次性邮件                                | 无                               |
| `POST /v1/auth/recovery/complete`    | `{token,password}`；消耗所有待处理的找回令牌，并撤销会话/授权                    | 找回令牌                         |
| `GET /v1/me`                         | `{id,email,role,session_id}`                                                     | 用户                             |
| `GET /v1/auth/sessions`              | 当前账户的登录会话                                                               | 用户                             |
| `POST /v1/auth/sessions/{id}/revoke` | 撤销所属登录及其授权                                                             | 用户                             |

邮箱统一转换为小写。邮箱验证有效期为 24 小时，密码找回为 30 分钟，访问令牌为 15 分钟，刷新令牌为 30 天。Argon2id 密码处理最多使用四个阻塞工作线程。注册、登录、找回、挑战和邀请流程使用 PostgreSQL 支持的限流；TLS 代理还应按 IP 限流。如果注册后邮件发送失败，可使用账户密码调用 `verify/resend` 恢复验证流程。

<a id="device"></a>

## 设备

客户端生成长期 Ed25519 密钥对，并将私钥保存在平台安全存储中。32 字节公钥以小写十六进制发送。向 `POST /v1/devices/challenge` 发送 `{public_key}`，返回 `{challenge_id,nonce,message,expires_in}`。对返回的 UTF-8 `message` 原样签名，再向 `POST /v1/devices/bind` 发送 `{challenge_id,signature,name,platform,can_host,can_files}`。签名为 64 字节小写十六进制；`platform` 为 `windows`、`android` 或 `ios`。响应为 `{id,device_token}`。挑战只能使用一次，五分钟后过期。同一所有者重新绑定会轮换设备令牌；其他所有者不能接管同一公钥。管理员禁用的设备必须经管理员明确启用后才能重新绑定。

| 方法与路径                                              | 请求体 / 结果                                                                                                        | 凭据 |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ---- |
| `GET /v1/devices?limit=50&after=<uuid>&host_only=false` | 当前绑定到自己的所有设备，按 UUID 排序，包括离线和管理员禁用的设备；包含 `online`、`enabled`、`last_seen_at`         | 用户 |
| `GET /v1/devices/{id}`                                  | 自己绑定的设备                                                                                                       | 用户 |
| `PATCH /v1/devices/{id}`                                | `{name}`                                                                                                             | 用户 |
| `DELETE /v1/devices/{id}`                               | 解绑自己已启用的设备，撤销凭据、邀请和授权                                                                           | 用户 |
| `POST /v1/devices/heartbeat`                            | `{generation:null}` 开始新的连接代次；后续 `{generation:n}` 续期 60 秒租约                                           | 设备 |
| `POST /v1/devices/capability`                           | `{generation:n,can_host:true\|false}` 更新在线设备的被控能力；设为 false 会在事务中撤销待审批/已批准的查看和控制请求 | 设备 |

过时的代次收到 409，不能更改较新连接的租约。设备凭据关联到签发它的登录会话；退出登录、撤销登录、修改/找回密码及禁用账户都会使其失效并清除在线状态。之后再次登录时，需签署新的绑定挑战以获取新令牌。解绑会保留非活动的历史行供审计引用，并释放其公钥，以便通过新的明确证明重新绑定；管理员禁用保留绑定，必须经管理员启用后才能重新证明。`device_token` 不能调用账户或列表接口。

Windows 桌面绑定初始使用 `can_host=false` 和 `can_files=false`。原生客户端探测 DXGI，并要求本地共享开关开启后才能设置 `can_host=true`。协调服务要求存在匹配且有效的心跳代次。桌面端在未开启共享的情况下启动时，会清除异常退出遗留的过时被控声明。在 WI-005 之前，`can_files` 始终为 false。

<a id="invitation-and-remote-authorization"></a>

## 邀请与远程授权

权限为 `view`、`control`、`files`，三者彼此独立。邀请有效期为十分钟，只能使用一次，限定于目标设备和权限。请求方必须证明自己的在线来源设备。目标设备必须在线且具备相应能力。同一所有者的请求无需邀请；不同所有者之间的请求必须消耗有效邀请。所有请求，包括同一所有者的请求，仍以 `pending` 状态开始。只有目标设备凭据可以批准或拒绝。管理员角色不能批准。

| 方法与路径                         | 请求体 / 结果                                                                              | 凭据                            |
| ---------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------- |
| `POST /v1/invitations`             | `{target_device_id,permission}` → `{id,code,expires_in}`                                   | 目标所有者用户                  |
| `POST /v1/invitations/{id}/revoke` | 撤销邀请码及其关联授权                                                                     | 目标所有者用户                  |
| `POST /v1/remote/request`          | `{source_device_id,target_device_id,permission,invitation_code?}` → `{id,state:"pending"}` | 请求方用户 + 来源设备请求头     |
| `GET /v1/remote/pending`           | 目标设备的待审批请求，包含请求方邮箱和来源设备名称，帮助本地用户作出知情决定               | 目标设备                        |
| `POST /v1/remote/{id}/decide`      | `{approve:true\|false}` → 批准时返回授权，拒绝时返回 null                                  | 目标设备                        |
| `POST /v1/remote/{id}/renew`       | 先前的 30 秒授权仍有效且检查仍通过时，返回新授权令牌                                       | 目标设备                        |
| `POST /v1/remote/{id}/revoke`      | 撤销请求/授权                                                                              | 请求方用户或目标设备            |
| `GET /v1/remote/{id}/grant`        | 验证有效授权，并返回会话、权限、双方公钥、随机数和过期时间                                 | 授权令牌 + 参与设备请求头       |
| `GET /v1/remote/{id}`              | 待审批/已批准/已拒绝/已撤销/已过期的元数据；不含授权令牌                                   | 请求方/目标所有者用户或参与设备 |
| `GET /v1/remote/sessions`          | 最近 100 条属于自己或自己目标设备的请求；不含授权令牌                                      | 用户                            |

30 秒授权在 PostgreSQL 中绑定会话 ID、请求方账户登录、双方设备身份、确切权限、随机数和过期时间。检查和续期会重新检查账户状态、设备状态、在线租约、邀请撤销和登录撤销。过期授权无法续期；必须发起新请求并获得目标重新批准。目标仅通过经过设备认证的 iroh 连接交付授权；`GET /v1/remote/{id}` 仍是状态轮询。帧权限由原生传输层强制执行；屏幕/输入/文件处理器仍待实现。

<a id="wi-003-address-signaling-and-transport-validation"></a>

## WI-003 地址信令与传输验证

这些路由要求设备 Bearer 凭据和 `X-Farsail-Generation: <current heartbeat generation>`。过时进程不能发布或发现地址。地址包含序列化为 JSON 的 iroh `EndpointAddr`，其中端点 ID 必须等于已注册的 Ed25519 公钥。最多接受 16 个 IP/relay 地址，序列化后最多 2048 字节。不接受自定义传输。Relay URL 必须使用 HTTPS，不能包含 URL 凭据、查询或片段，并且必须出现在协调服务的 `FARSAIL_RELAY_URLS` 逗号分隔允许列表中。该列表默认为空。

| 方法与路径                             | 请求体 / 结果                                                                          | 凭据                                |
| -------------------------------------- | -------------------------------------------------------------------------------------- | ----------------------------------- |
| `POST /v1/devices/endpoint-address`    | `{generation,endpoint_addr}`；仅插入或更新当前代次，绝不允许延迟到达的旧代次覆盖新代次 | 设备 + 代次                         |
| `GET /v1/remote/{id}/peer-address`     | 有效已批准会话中另一台设备的 `{endpoint_addr,generation,expires_in}`                   | 参与设备 + 代次                     |
| `GET /v1/remote/{id}/transport-grant`  | 与授权检查相同的声明，加上服务端测量的 `expires_in`；绑定当前设备代次                  | 授权 Bearer + 参与设备请求头 + 代次 |
| `POST /v1/remote/{id}/transport-renew` | 轮换目标的有效授权令牌，仅向目标返回                                                   | 目标设备 + 代次                     |

`peer-address` 重新检查参与设备、登录、在线租约、邀请和批准状态，范围仅限当前会话。授权检查的 `expires_in` 依据协调服务时钟测量；原生传输减去本地 HTTP 已耗时间，并使用 `Instant` 作为截止依据。较早的 `/grant` 和 `/renew` 路由保留用于 WI-001 授权契约，而传输使用绑定代次的变体。认证交换、帧限制、续期和路径报告见 [TRANSPORT.md](TRANSPORT.zh-CN.md)。

<a id="admin"></a>

## 管理员

显式在本地执行 `cargo run -p farsail-coordinator -- bootstrap-admin <verified-email>`，为一个已存在、已验证、已启用的账户赋予管理员角色。不会自动提升首个注册者。管理员路由要求用户访问令牌，绝不签发远程授权。

| 方法与路径                                  | 功能                                                                                     |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `GET /v1/admin/users?limit=50&after=<uuid>` | 分页用户列表                                                                             |
| `POST /v1/admin/users/{id}/enabled`         | `{enabled}`；禁用时撤销会话、设备凭据、邀请和授权                                        |
| `POST /v1/admin/devices/{id}/revoke`        | 禁用设备、轮换凭据、撤销关联授权                                                         |
| `POST /v1/admin/devices/{id}/enabled`       | `{enabled:true}`；重新启用后所有者必须签署新的绑定挑战                                   |
| `GET/PUT /v1/admin/registration`            | `{value:"open"\|"invite_only"\|"closed"}`                                                |
| `POST /v1/admin/signup-invitations`         | `{email}` → `{id,code,expires_in}`；邀请码仅返回一次供人工交付，有效期七天且限定于该邮箱 |
| `DELETE /v1/admin/signup-invitations/{id}`  | 撤销未使用的注册邀请                                                                     |
| `GET /v1/admin/sessions`                    | 最近 100 条登录会话元数据                                                                |
| `GET /v1/admin/audit`                       | 最近 100 条元数据事件；不含密码、令牌、帧或输入                                          |

<a id="configuration"></a>

## 配置

必须设置 `FARSAIL_DATABASE_URL`。`FARSAIL_BIND` 默认为 `127.0.0.1:8787`，拒绝非回环地址。`FARSAIL_MAIL_MODE=smtp-local` 默认使用 `127.0.0.1:1025` 上的 Mailpit（本地 Compose 设置 `FARSAIL_SMTP_PORT=51025`）；它拒绝非回环 SMTP 主机。`FARSAIL_MAIL_MODE=smtp-tls` 要求 `FARSAIL_SMTP_HOST`、`FARSAIL_SMTP_USER`、`FARSAIL_SMTP_PASSWORD`、`FARSAIL_MAIL_FROM`，并使用验证 TLS 的 SMTP relay。`memory` 要求显式设置 `FARSAIL_DEV_MEMORY_MAIL=1`，且仅用于测试。审计事件仅含元数据；提交后的审计写入错误会记录日志，但不会将成功的变更变成结果不明确的 HTTP 失败。生产部署在公开服务前应提供持久化审计监控、外部按 IP 限流、备份和 TLS 代理配置。
<a id="internal-relay-admission-wi-008a"></a>

# 内部 relay 准入（WI-008A）

`POST /internal/relay-access` 只在设置至少 32 字节的 `FARSAIL_RELAY_ACCESS_TOKEN` 时挂载，必须使用匹配的 Bearer 令牌和 64 位十六进制 `X-Iroh-NodeId`。该公钥由 iroh-relay 连接握手证明；调用者仅应为可信本机 relay，客户端不能自行调用来代替设备证明。仅 `200 true` 表示注册设备仍绑定且启用、用户已验证且启用、绑定登录未撤销且刷新令牌未过期。未知/禁用设备返回 `200 false`；Bearer 令牌缺失或错误返回 401，公钥无效返回 400，数据库故障返回 500 或在 2 秒后超时。反向代理公网路径屏蔽整个 `/internal`。

这是连接准入，不等同于远控授权：已连接的 relay 不会持续回查；应用授权、30 秒租约与撤销规则不变。所有公网 `/v1/admin` 接口继续使用原有管理员鉴权，不因反向代理而开放匿名管理。
