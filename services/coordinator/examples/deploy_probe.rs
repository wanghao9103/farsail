//! Real external TLS relay probe; test fixture contains only disposable registered keys.
use iroh::{
    Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey,
    endpoint::{PortmapperConfig, presets},
};
use iroh_relay::tls::CaTlsConfig;
use rustls_pki_types::CertificateDer;
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Fixture {
    relay: String,
    ca_der: String,
    keys: Vec<String>,
    deny: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let path = PathBuf::from(std::env::var("FARSAIL_DEPLOY_FIXTURE")?);
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(&path)?)?;
    let ca = std::fs::read(&fixture.ca_der)?;
    let relay: RelayUrl = fixture.relay.parse()?;
    let mut endpoints = Vec::new();
    for key in &fixture.keys {
        let secret: [u8; 32] = hex::decode(key)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("key size"))?;
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&secret))
            .alpns(vec![b"farsail/deploy-probe/1".to_vec()])
            .clear_address_lookup()
            .clear_ip_transports()
            .portmapper_config(PortmapperConfig::Disabled)
            .relay_mode(RelayMode::custom([relay.clone()]))
            .ca_tls_config(CaTlsConfig::custom_roots([CertificateDer::from(
                ca.clone(),
            )]))
            .bind()
            .await?;
        let online = tokio::time::timeout(
            Duration::from_secs(if fixture.deny { 5 } else { 20 }),
            endpoint.online(),
        )
        .await;
        if fixture.deny {
            anyhow::ensure!(online.is_err(), "rejected identity unexpectedly admitted");
            endpoint.close().await;
            println!("relay denied identity or unavailable admission service");
            continue;
        }
        online.context("registered endpoint did not become online at relay")?;
        endpoints.push(endpoint);
    }
    if fixture.deny {
        return Ok(());
    }
    anyhow::ensure!(endpoints.len() == 2, "two keys required");
    let address = EndpointAddr::new(endpoints[1].id()).with_relay_url(relay);
    let connect = endpoints[0].connect(address, b"farsail/deploy-probe/1");
    let accept = async { endpoints[1].accept().await.unwrap().await };
    let (source, target) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(connect, accept)
    })
    .await
    .context("relay QUIC connection deadline")?;
    let (source, target) = (source?, target?);
    let send = async {
        let (mut tx, mut rx) = source.open_bi().await?;
        tx.write_all(b"deployment-relay-payload").await?;
        tx.finish()?;
        anyhow::ensure!(
            rx.read_to_end(64).await? == b"reply-through-relay",
            "wrong reply"
        );
        anyhow::Ok(())
    };
    let receive = async {
        let (mut tx, mut rx) = target.accept_bi().await?;
        anyhow::ensure!(
            rx.read_to_end(64).await? == b"deployment-relay-payload",
            "wrong payload"
        );
        tx.write_all(b"reply-through-relay").await?;
        tx.finish()?;
        anyhow::Ok(())
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::try_join!(send, receive)
    })
    .await??;
    anyhow::ensure!(
        source
            .paths()
            .iter()
            .any(|p| p.is_selected() && p.is_relay()),
        "selected path is not relay"
    );
    println!("verified TLS + registered identities + bidirectional payload + selected relay path");
    source.close(0u32.into(), b"done");
    target.close(0u32.into(), b"done");
    for endpoint in endpoints {
        endpoint.close().await;
    }
    Ok(())
}
