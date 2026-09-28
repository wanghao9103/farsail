use crate::account::rate_limit;
use crate::{AppState, Error, Result, audit, bearer, hash, principal, random_bytes, token};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use farsail_core::{CHALLENGE_SECONDS, DEVICE_LEASE_SECONDS, DevicePlatform};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
type DeviceRow = (
    Uuid,
    String,
    String,
    bool,
    bool,
    bool,
    Option<chrono::DateTime<chrono::Utc>>,
    bool,
);

#[derive(Deserialize)]
pub struct ChallengeInput {
    pub public_key: String,
}
#[derive(Serialize, Deserialize)]
pub struct ChallengeOutput {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub message: String,
    pub expires_in: i64,
}
#[derive(Deserialize)]
pub struct BindInput {
    pub challenge_id: Uuid,
    pub signature: String,
    pub name: String,
    pub platform: DevicePlatform,
    pub can_host: bool,
    pub can_files: bool,
}
#[derive(Serialize, Deserialize)]
pub struct BindOutput {
    pub id: Uuid,
    pub device_token: String,
}
#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub after: Option<Uuid>,
    pub host_only: Option<bool>,
}
#[derive(Serialize, Deserialize)]
pub struct DeviceView {
    pub id: Uuid,
    pub name: String,
    pub platform: String,
    pub can_host: bool,
    pub can_files: bool,
    pub online: bool,
    pub last_seen_at: Option<String>,
    pub enabled: bool,
}
#[derive(Deserialize)]
pub struct RenameInput {
    pub name: String,
}
#[derive(Deserialize)]
pub struct CapabilityInput {
    pub generation: i64,
    pub can_host: bool,
}
#[derive(Deserialize)]
pub struct HeartbeatInput {
    pub generation: Option<i64>,
}
#[derive(Serialize, Deserialize)]
pub struct HeartbeatOutput {
    pub generation: i64,
    pub expires_in: i64,
}

fn bytes_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    let decoded = hex::decode(value).map_err(|_| Error::Invalid("invalid hex"))?;
    decoded
        .try_into()
        .map_err(|_| Error::Invalid("wrong byte length"))
}
pub fn bind_message(id: Uuid, nonce: &[u8], owner: Uuid) -> String {
    format!("farsail:bind:v1:{id}:{}:{owner}", hex::encode(nonce))
}
fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(Error::Invalid("invalid device name"));
    }
    Ok(name)
}

pub async fn challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ChallengeInput>,
) -> Result<Json<ChallengeOutput>> {
    let p = principal(&state, &headers).await?;
    rate_limit(&state, &format!("challenge:{}", p.user_id), 30).await?;
    let key = bytes_hex::<32>(&input.public_key)?;
    VerifyingKey::from_bytes(&key).map_err(|_| Error::Invalid("invalid Ed25519 public key"))?;
    let id = Uuid::new_v4();
    let nonce = random_bytes();
    sqlx::query("INSERT INTO device_challenges(id,owner_id,public_key,nonce,expires_at) VALUES($1,$2,$3,$4,now()+($5 * interval '1 second'))")
        .bind(id).bind(p.user_id).bind(key.as_slice()).bind(&nonce).bind(CHALLENGE_SECONDS).execute(&state.pool).await?;
    Ok(Json(ChallengeOutput {
        challenge_id: id,
        nonce: hex::encode(&nonce),
        message: bind_message(id, &nonce, p.user_id),
        expires_in: CHALLENGE_SECONDS,
    }))
}

pub async fn bind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<BindInput>,
) -> Result<Json<BindOutput>> {
    let p = principal(&state, &headers).await?;
    let name = valid_name(&input.name)?;
    let signature = Signature::from_bytes(&bytes_hex::<64>(&input.signature)?);
    let mut tx = state.pool.begin().await?;
    let row: Option<(Vec<u8>,Vec<u8>)> = sqlx::query_as("SELECT public_key,nonce FROM device_challenges WHERE id=$1 AND owner_id=$2 AND consumed_at IS NULL AND expires_at>now() FOR UPDATE")
        .bind(input.challenge_id).bind(p.user_id).fetch_optional(&mut *tx).await?;
    let (key, nonce) = row.ok_or(Error::Invalid("invalid or expired challenge"))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| Error::Invalid("invalid public key"))?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| Error::Invalid("invalid public key"))?
        .verify(
            bind_message(input.challenge_id, &nonce, p.user_id).as_bytes(),
            &signature,
        )
        .map_err(|_| Error::Forbidden)?;
    sqlx::query("UPDATE device_challenges SET consumed_at=now() WHERE id=$1")
        .bind(input.challenge_id)
        .execute(&mut *tx)
        .await?;
    let existing: Option<(Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT id,owner_id,enabled FROM devices WHERE public_key=$1 AND bound FOR UPDATE",
    )
    .bind(key.as_slice())
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_some_and(|(_, owner, enabled)| owner != p.user_id || !enabled) {
        return Err(Error::Conflict);
    }
    let mut id = existing.map(|x| x.0).unwrap_or_else(Uuid::new_v4);
    let device_token = token();
    if existing.is_some() {
        sqlx::query("UPDATE devices SET name=$2,platform=$3,can_host=$4,can_files=$5,credential_hash=$6,credential_session_id=$7,generation=generation+1,lease_until=NULL WHERE id=$1")
            .bind(id).bind(name).bind(input.platform.as_str()).bind(input.can_host).bind(input.can_files).bind(hash(&device_token)).bind(p.session_id)
            .execute(&mut *tx).await?;
    } else {
        let inserted:Option<(Uuid,)> = sqlx::query_as("INSERT INTO devices(id,owner_id,public_key,name,platform,can_host,can_files,credential_hash,credential_session_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(public_key) DO UPDATE SET name=EXCLUDED.name,platform=EXCLUDED.platform,can_host=EXCLUDED.can_host,can_files=EXCLUDED.can_files,credential_hash=EXCLUDED.credential_hash,credential_session_id=EXCLUDED.credential_session_id,generation=devices.generation+1,lease_until=NULL WHERE devices.owner_id=EXCLUDED.owner_id AND devices.enabled AND devices.bound RETURNING id")
            .bind(id).bind(p.user_id).bind(key.as_slice()).bind(name).bind(input.platform.as_str()).bind(input.can_host).bind(input.can_files).bind(hash(&device_token)).bind(p.session_id)
            .fetch_optional(&mut *tx).await?;
        id = inserted.ok_or(Error::Conflict)?.0;
    }
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (source_device_id=$1 OR target_device_id=$1) AND state IN ('pending','approved')")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    audit(&state.pool, Some(p.user_id), "bind_device", Some(id), "ok").await?;
    Ok(Json(BindOutput { id, device_token }))
}

pub async fn device_principal(state: &AppState, headers: &HeaderMap) -> Result<(Uuid, Uuid)> {
    let t = bearer(headers)?;
    let row: Option<(Uuid,Uuid)> = sqlx::query_as("SELECT d.id,d.owner_id FROM devices d JOIN users u ON u.id=d.owner_id JOIN auth_sessions s ON s.id=d.credential_session_id WHERE d.credential_hash=$1 AND d.enabled AND d.bound AND u.enabled AND s.revoked_at IS NULL AND s.refresh_expires_at>now()")
        .bind(hash(t)).fetch_optional(&state.pool).await?;
    row.ok_or(Error::Unauthorized)
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<DeviceView>>> {
    let p = principal(&state, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let rows: Vec<DeviceRow> = sqlx::query_as(
        "SELECT id,name,platform,can_host,can_files,COALESCE(lease_until>now(),false),last_seen_at,enabled FROM devices WHERE owner_id=$1 AND bound AND ($2::uuid IS NULL OR id>$2) AND (NOT $3 OR can_host) ORDER BY id LIMIT $4")
        .bind(p.user_id).bind(q.after).bind(q.host_only.unwrap_or(false)).bind(limit).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, name, platform, can_host, can_files, online, last_seen_at, enabled)| {
                    DeviceView {
                        id,
                        name,
                        platform,
                        can_host,
                        can_files,
                        online: online && enabled,
                        last_seen_at: last_seen_at.map(|x| x.to_rfc3339()),
                        enabled,
                    }
                },
            )
            .collect(),
    ))
}

async fn owned(state: &AppState, owner: Uuid, id: Uuid) -> Result<DeviceView> {
    let row: Option<DeviceRow> = sqlx::query_as(
        "SELECT id,name,platform,can_host,can_files,COALESCE(lease_until>now(),false),last_seen_at,enabled FROM devices WHERE id=$1 AND owner_id=$2 AND bound")
        .bind(id).bind(owner).fetch_optional(&state.pool).await?;
    row.map(
        |(id, name, platform, can_host, can_files, online, last_seen_at, enabled)| DeviceView {
            id,
            name,
            platform,
            can_host,
            can_files,
            online: online && enabled,
            last_seen_at: last_seen_at.map(|x| x.to_rfc3339()),
            enabled,
        },
    )
    .ok_or(Error::NotFound)
}
pub async fn get_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<DeviceView>> {
    let p = principal(&state, &headers).await?;
    Ok(Json(owned(&state, p.user_id, id).await?))
}
pub async fn rename(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<RenameInput>,
) -> Result<Json<DeviceView>> {
    let p = principal(&state, &headers).await?;
    let name = valid_name(&input.name)?;
    let r = sqlx::query("UPDATE devices SET name=$3 WHERE id=$1 AND owner_id=$2")
        .bind(id)
        .bind(p.user_id)
        .bind(name)
        .execute(&state.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    };
    Ok(Json(owned(&state, p.user_id, id).await?))
}
/// A live device may change only its implemented host capability. Disabling
/// invalidates outstanding requests and grants before the response is sent.
pub async fn capability(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CapabilityInput>,
) -> Result<Json<DeviceView>> {
    let (id, owner) = device_principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE devices SET can_host=$2 WHERE id=$1 AND generation=$3 AND lease_until>now() AND enabled AND bound AND platform='windows'")
        .bind(id).bind(input.can_host).bind(input.generation).execute(&mut *tx).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::Conflict);
    }
    if !input.can_host {
        sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE target_device_id=$1 AND permission IN ('view','control') AND state IN ('pending','approved')")
            .bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(owned(&state, owner, id).await?))
}
pub async fn unbind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let p = principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    // Keep a historical row for session/audit FKs while freeing the active public-key identity.
    let r=sqlx::query("UPDATE devices SET bound=false,enabled=false,lease_until=NULL,credential_hash=$3,public_key=$4 WHERE id=$1 AND owner_id=$2 AND bound AND enabled")
        .bind(id).bind(p.user_id).bind(hash(&token())).bind(random_bytes()).execute(&mut *tx).await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    };
    sqlx::query(
        "UPDATE invitations SET revoked_at=now() WHERE target_device_id=$1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (source_device_id=$1 OR target_device_id=$1) AND state IN ('pending','approved')")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "unbind_device",
        Some(id),
        "ok",
    )
    .await?;
    Ok(())
}
pub async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<HeartbeatInput>,
) -> Result<Json<HeartbeatOutput>> {
    let (id, _) = device_principal(&state, &headers).await?;
    let row: Option<(i64,)> = if let Some(generation) = input.generation {
        sqlx::query_as("UPDATE devices SET lease_until=now()+($2 * interval '1 second'),last_seen_at=now() WHERE id=$1 AND generation=$3 AND enabled RETURNING generation")
            .bind(id).bind(DEVICE_LEASE_SECONDS).bind(generation).fetch_optional(&state.pool).await?
    } else {
        sqlx::query_as("UPDATE devices SET generation=generation+1,lease_until=now()+($2 * interval '1 second'),last_seen_at=now() WHERE id=$1 AND enabled RETURNING generation")
            .bind(id).bind(DEVICE_LEASE_SECONDS).fetch_optional(&state.pool).await?
    };
    let (generation,) = row.ok_or(Error::Conflict)?;
    Ok(Json(HeartbeatOutput {
        generation,
        expires_in: DEVICE_LEASE_SECONDS,
    }))
}
