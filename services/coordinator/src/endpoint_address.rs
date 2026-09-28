//! Address signaling is scoped to an authenticated device and a live approved session.
use crate::remote::transport_generation;
use crate::{AppState, Error, Result, device::device_principal};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use iroh::{EndpointAddr, RelayUrl, TransportAddr};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct PublishInput {
    pub generation: i64,
    pub endpoint_addr: EndpointAddr,
}

#[derive(Serialize)]
pub struct PeerAddress {
    pub endpoint_addr: EndpointAddr,
    pub generation: i64,
    pub expires_in: i64,
}

pub(crate) fn valid_relay(url: &RelayUrl) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/"
}
fn valid_addr(addr: &EndpointAddr, allowed: &[RelayUrl]) -> bool {
    !addr.is_empty()
        && addr.addrs.len() <= 16
        && addr.addrs.iter().all(|a| match a {
            TransportAddr::Ip(_) => true,
            TransportAddr::Relay(url) => valid_relay(url) && allowed.contains(url),
            _ => false,
        })
}

pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PublishInput>,
) -> Result<()> {
    let (device_id, _) = device_principal(&state, &headers).await?;
    let generation = transport_generation(&headers)?;
    if generation != input.generation {
        return Err(Error::Forbidden);
    }
    let serialized = serde_json::to_string(&input.endpoint_addr)
        .map_err(|_| Error::Invalid("invalid endpoint address"))?;
    if !valid_addr(&input.endpoint_addr, &state.allowed_relays) || serialized.len() > 2048 {
        return Err(Error::Invalid("invalid endpoint address"));
    }
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT public_key FROM devices WHERE id=$1 AND generation=$2 AND lease_until>now() AND enabled AND bound",
    )
    .bind(device_id)
    .bind(input.generation)
    .fetch_optional(&state.pool)
    .await?;
    let (registered_key,) = row.ok_or(Error::Conflict)?;
    if input.endpoint_addr.id.as_bytes().as_slice() != registered_key {
        return Err(Error::Forbidden);
    }
    // Recheck generation in the INSERT statement so a concurrent heartbeat cannot
    // create a new generation after the SELECT above.
    let changed = sqlx::query(
        "INSERT INTO device_endpoint_addresses(device_id,generation,endpoint_addr) SELECT id,$2,$3 FROM devices WHERE id=$1 AND generation=$2 AND lease_until>now() AND enabled AND bound ON CONFLICT(device_id) DO UPDATE SET generation=EXCLUDED.generation,endpoint_addr=EXCLUDED.endpoint_addr,updated_at=now() WHERE device_endpoint_addresses.generation<=EXCLUDED.generation",
    )
    .bind(device_id)
    .bind(input.generation)
    .bind(serialized)
    .execute(&state.pool)
    .await?;
    if changed.rows_affected() == 0 {
        return Err(Error::Conflict);
    }
    Ok(())
}

pub async fn peer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(remote_id): Path<Uuid>,
) -> Result<Json<PeerAddress>> {
    let (device_id, _) = device_principal(&state, &headers).await?;
    let generation = transport_generation(&headers)?;
    let row: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT e.endpoint_addr,e.generation,GREATEST(0,EXTRACT(EPOCH FROM (peer.lease_until-now()))::bigint) FROM remote_sessions r JOIN devices peer ON peer.id=CASE WHEN r.source_device_id=$2 THEN r.target_device_id ELSE r.source_device_id END JOIN device_endpoint_addresses e ON e.device_id=peer.id AND e.generation=peer.generation JOIN devices actor ON actor.id=$2 JOIN auth_sessions a ON a.id=r.auth_session_id JOIN auth_sessions pa ON pa.id=peer.credential_session_id JOIN auth_sessions aa ON aa.id=actor.credential_session_id JOIN users requester ON requester.id=r.requester_id JOIN users owner ON owner.id=peer.owner_id WHERE r.id=$1 AND $2 IN (r.source_device_id,r.target_device_id) AND r.state='approved' AND r.grant_until>now() AND actor.generation=$3 AND actor.lease_until>now() AND peer.lease_until>now() AND actor.enabled AND peer.enabled AND actor.bound AND peer.bound AND requester.enabled AND owner.enabled AND a.revoked_at IS NULL AND a.refresh_expires_at>now() AND pa.revoked_at IS NULL AND pa.refresh_expires_at>now() AND aa.revoked_at IS NULL AND aa.refresh_expires_at>now() AND (r.invitation_id IS NULL OR EXISTS(SELECT 1 FROM invitations i WHERE i.id=r.invitation_id AND i.revoked_at IS NULL))",
    )
    .bind(remote_id)
    .bind(device_id)
    .bind(generation)
    .fetch_optional(&state.pool)
    .await?;
    let (json, generation, expires_in) = row.ok_or(Error::NotFound)?;
    let endpoint_addr: EndpointAddr = serde_json::from_str(&json)
        .map_err(|_| Error::Internal(anyhow::anyhow!("stored endpoint address invalid")))?;
    Ok(Json(PeerAddress {
        endpoint_addr,
        generation,
        expires_in,
    }))
}
