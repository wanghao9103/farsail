**English** | [简体中文](README.zh-CN.md)

<a id="自建-iroh-relay"></a>

# Self-hosted iroh relay

Public servers use the [complete offline deployment bundle](../../docs/DEPLOYMENT.md), without compiling Rust. The following is only a standalone build reference for a development host, pinned to the `iroh-relay` 1.2.0 `server` feature and [relay.example.toml](relay.example.toml):

```sh
cargo install --locked iroh-relay --version 1.2.0 --features server
iroh-relay --config-path /etc/farsail/relay.toml
```

The public URL is `https://<IP>:8443/`, the API uses 443, and the local listener is `0.0.0.0:8443`. An EIP may only be a NAT mapping; do not bind a non-local IP. The certificate SAN must contain the public IP, clients must validate the CA normally, and TLS verification cannot be disabled. The certificate private key stays on the server. The complete bundle uses a shared network namespace to keep the coordinator on loopback; a standalone template must provide its own loopback 8788 admission proxy.

Set `FARSAIL_RELAY_URLS=https://<IP>:8443/` on the coordinator and use the same address on clients. Set a private `IROH_RELAY_HTTP_BEARER_TOKEN`. After verifying the connection public key, the relay sends a POST with `X-Iroh-NodeId`; only a 200 response with a true body admits a valid registered/enabled device with a valid account and login session. Unknown or disabled devices, revoked logins, a missing Bearer token or service failure reject new connections. Admission is checked only at connection time; existing application streams remain governed by the 30-second grant.

The upstream HTTP access client has no total request timeout configured. The complete bundle's loopback Nginx limits connection time to 1 second and reads/writes to 3 seconds; coordinator database queries are limited to 2 seconds. The effective limit is 2,000,000 B/s received per client with a 4,000,000 B burst. The 1.2.0 source explicitly leaves `accept_conn_limit`/`accept_conn_burst` unimplemented, so they cannot be treated as protection. There is no promise of a global traffic quota or DDoS protection. Automatic mapping/public discovery are disabled by default; cross-NAT operation needs separate validation.

Focused local tests use a loopback HTTPS relay, temporary certificates and an explicit test CA. With IP transports disabled on both sides, they confirm that the selected path is `relay` and actually transfer data. These tests do not prove public-network reachability rates or cross-NAT performance.
