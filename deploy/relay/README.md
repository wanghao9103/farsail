# 自建 iroh relay

使用固定的 `iroh-relay` 1.2.0 `server` 功能和 [relay.example.toml](relay.example.toml)。在自己的服务器准备证书和密钥后运行：

```sh
cargo install --locked iroh-relay --version 1.2.0 --features server
iroh-relay --config-path /etc/farsail/relay.toml
```

将模板中的 `PUBLIC_IP` 改为本机实际监听 IP。只有公网 IP、没有域名时也可使用 `https://<IP>/`，但证书的 Subject Alternative Name 必须包含该 IP，且客户端系统信任签发 CA；不能以关闭 TLS 验证替代。证书链和私钥留在服务器，不能提交到仓库。若使用域名，改为与证书匹配的域名及监听地址。模板把 relay 的 HTTP 探测端口仅绑定到本机，HTTPS 监听 443；防火墙和反向代理按部署环境配置。没有在用户公网服务器执行任何部署。

协调服务设置 `FARSAIL_RELAY_URLS=https://<IP>/`（多个用逗号分隔），客户端设置相同地址；协调服务仅接受登记该名单内的 relay URL，不接受含账号密码、查询串或片段的 URL。生产环境使用系统/标准 CA 验证。自动端口映射默认关闭；若环境需要 NAT 穿透或 QUIC 地址发现，先在服务器与客户端按实际网络另行配置及验证。

本地专项测试使用回环 HTTPS relay、临时证书和明确的测试 CA，在两端禁用 IP transports 后确认选中的路径为 `relay` 并实际传送数据。该测试不证明公网可达率或跨 NAT 性能。
