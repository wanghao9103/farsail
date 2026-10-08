[English](README.md) | **简体中文**

<a id="self-hosted-iroh-relay"></a>

# 自建 iroh relay

公网服务器使用[完整离线部署包](../../docs/DEPLOYMENT.zh-CN.md)，无需编译 Rust。以下仅为开发主机的独立构建参考，固定 `iroh-relay` 1.2.0 `server` 功能和 [relay.example.toml](relay.example.toml)：

```sh
cargo install --locked iroh-relay --version 1.2.0 --features server
iroh-relay --config-path /etc/farsail/relay.toml
```

公网URL为 `https://<IP>:8443/`，API为443，本地监听 `0.0.0.0:8443`；EIP可能只是NAT映射，不应绑定非本地IP。证书SAN必须包含公网IP，客户端正常验证CA，不能关闭TLS校验。证书私钥留服务器。完整包通过共享网络命名空间保持协调服务loopback，独立模板须自行提供回环8788准入代理。

协调服务设置 `FARSAIL_RELAY_URLS=https://<IP>:8443/`，客户端同址。设置私有 `IROH_RELAY_HTTP_BEARER_TOKEN`，relay验证连接公钥后POST `X-Iroh-NodeId`，仅200正文true放行有效注册/启用设备及其有效账号、登录会话。未知、禁用、撤销登录、缺bearer、服务故障拒绝新连接。准入只在连接时检查；已有应用流仍由30秒grant管理。

upstream HTTP access client未配置总请求超时，完整包的回环Nginx限制连接1秒、读写3秒，协调数据库查询限2秒。有效限速为每客户端接收2,000,000 B/s、突发4,000,000 B；`accept_conn_limit`/`accept_conn_burst` 在1.2.0源码明确未实现，不能当作保护。没有全局流量配额或DDoS防护承诺。自动映射/公共发现默认关闭，跨NAT须另验。

本地专项测试使用回环 HTTPS relay、临时证书和明确的测试 CA，在两端禁用 IP transports 后确认选中的路径为 `relay` 并实际传送数据。该测试不证明公网可达率或跨 NAT 性能。
