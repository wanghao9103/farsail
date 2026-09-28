use crate::account::rate_limit;
use crate::device::device_principal;
use crate::{AppState, Error, Result, audit, bearer, hash, principal, random_bytes, token};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use farsail_core::{GRANT_SECONDS, RemotePermission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
type RemoteRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    Option<chrono::DateTime<chrono::Utc>>,
);
type GrantRow = (String, Vec<u8>, Vec<u8>, Vec<u8>);
type GrantInspectRow = (
    String,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    chrono::DateTime<chrono::Utc>,
);

#[derive(Deserialize)]
pub struct InviteInput {
    pub target_device_id: Uuid,
    pub permission: RemotePermission,
}
#[derive(Serialize, Deserialize)]
pub struct InviteOutput {
    pub id: Uuid,
    pub code: String,
    pub expires_in: i64,
}
#[derive(Deserialize)]
pub struct RequestInput {
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub permission: RemotePermission,
    pub invitation_code: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct RequestOutput {
    pub id: Uuid,
    pub state: String,
}
#[derive(Deserialize)]
pub struct Decision {
    pub approve: bool,
}
#[derive(Serialize, Deserialize)]
pub struct GrantOutput {
    pub session_id: Uuid,
    pub grant_token: String,
    pub permission: String,
    pub source_public_key: String,
    pub target_public_key: String,
    pub nonce: String,
    pub expires_in: i64,
}
#[derive(Serialize)]
pub struct PendingView {
    pub id: Uuid,
    pub requester_id: Uuid,
    pub source_device_id: Uuid,
    pub permission: String,
}
#[derive(Serialize, Deserialize)]
pub struct RemoteView {
    pub id: Uuid,
    pub requester_id: Uuid,
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub permission: String,
    pub state: String,
    pub grant_expires_at: Option<String>,
}
pub async fn sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RemoteView>>> {
    let p = principal(&state, &headers).await?;
    let rows:Vec<RemoteRow>=sqlx::query_as(
        "SELECT r.id,r.requester_id,r.source_device_id,r.target_device_id,r.permission,CASE WHEN r.state='pending' AND r.created_at<=now()-interval '5 minutes' THEN 'expired' WHEN r.state='approved' AND r.grant_until<=now() THEN 'expired' ELSE r.state END,r.grant_until FROM remote_sessions r JOIN devices t ON t.id=r.target_device_id WHERE r.requester_id=$1 OR t.owner_id=$1 ORDER BY r.created_at DESC LIMIT 100")
        .bind(p.user_id).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    id,
                    requester_id,
                    source_device_id,
                    target_device_id,
                    permission,
                    state,
                    expiry,
                )| RemoteView {
                    id,
                    requester_id,
                    source_device_id,
                    target_device_id,
                    permission,
                    state,
                    grant_expires_at: expiry.map(|x| x.to_rfc3339()),
                },
            )
            .collect(),
    ))
}
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<RemoteView>> {
    let requester = principal(&state, &headers).await.ok().map(|p| p.user_id);
    let proof = device_header(&headers)
        .ok()
        .map(|x| hash(&x))
        .unwrap_or_default();
    let row:Option<RemoteRow>=sqlx::query_as(
        "SELECT r.id,r.requester_id,r.source_device_id,r.target_device_id,r.permission,CASE WHEN r.state='pending' AND r.created_at<=now()-interval '5 minutes' THEN 'expired' WHEN r.state='approved' AND r.grant_until<=now() THEN 'expired' ELSE r.state END,r.grant_until FROM remote_sessions r JOIN devices s ON s.id=r.source_device_id JOIN devices t ON t.id=r.target_device_id JOIN users su ON su.id=s.owner_id JOIN users tu ON tu.id=t.owner_id JOIN auth_sessions sa ON sa.id=s.credential_session_id JOIN auth_sessions ta ON ta.id=t.credential_session_id WHERE r.id=$1 AND (r.requester_id=$2 OR t.owner_id=$2 OR (s.credential_hash=$3 AND s.enabled AND s.bound AND su.enabled AND sa.revoked_at IS NULL AND sa.refresh_expires_at>now()) OR (t.credential_hash=$3 AND t.enabled AND t.bound AND tu.enabled AND ta.revoked_at IS NULL AND ta.refresh_expires_at>now()))")
        .bind(id).bind(requester).bind(proof).fetch_optional(&state.pool).await?;
    let (id, requester_id, source_device_id, target_device_id, permission, state, expiry) =
        row.ok_or(Error::NotFound)?;
    Ok(Json(RemoteView {
        id,
        requester_id,
        source_device_id,
        target_device_id,
        permission,
        state,
        grant_expires_at: expiry.map(|x| x.to_rfc3339()),
    }))
}

fn device_header(headers: &HeaderMap) -> Result<String> {
    headers
        .get("x-farsail-device-token")
        .and_then(|x| x.to_str().ok())
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
        .ok_or(Error::Unauthorized)
}
async fn source_proof(state: &AppState, headers: &HeaderMap, id: Uuid, owner: Uuid) -> Result<()> {
    let token = device_header(headers)?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT d.id FROM devices d JOIN auth_sessions a ON a.id=d.credential_session_id WHERE d.id=$1 AND d.owner_id=$2 AND d.credential_hash=$3 AND d.enabled AND d.bound AND a.revoked_at IS NULL AND a.refresh_expires_at>now()",
    )
    .bind(id)
    .bind(owner)
    .bind(hash(&token))
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(Error::Forbidden)?;
    Ok(())
}

pub async fn invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<InviteInput>,
) -> Result<Json<InviteOutput>> {
    let p = principal(&state, &headers).await?;
    rate_limit(&state, &format!("invite:{}", p.user_id), 30).await?;
    let row: Option<(bool, bool)> = sqlx::query_as(
        "SELECT can_host,can_files FROM devices WHERE id=$1 AND owner_id=$2 AND enabled",
    )
    .bind(input.target_device_id)
    .bind(p.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let (host, files) = row.ok_or(Error::NotFound)?;
    if (input.permission == RemotePermission::Files && !files)
        || (input.permission != RemotePermission::Files && !host)
    {
        return Err(Error::Forbidden);
    };
    let id = Uuid::new_v4();
    let code = token();
    sqlx::query("INSERT INTO invitations(id,creator_id,target_device_id,code_hash,permission,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '10 minutes')")
        .bind(id).bind(p.user_id).bind(input.target_device_id).bind(hash(&code)).bind(input.permission.as_str()).execute(&state.pool).await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "create_invitation",
        Some(id),
        "ok",
    )
    .await?;
    Ok(Json(InviteOutput {
        id,
        code,
        expires_in: 600,
    }))
}
pub async fn revoke_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let p = principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    let r=sqlx::query("UPDATE invitations SET revoked_at=now() WHERE id=$1 AND creator_id=$2 AND revoked_at IS NULL")
        .bind(id).bind(p.user_id).execute(&mut *tx).await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    };
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE invitation_id=$1 AND state IN ('pending','approved')")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RequestInput>,
) -> Result<Json<RequestOutput>> {
    let p = principal(&state, &headers).await?;
    rate_limit(&state, &format!("remote_request:{}", p.user_id), 60).await?;
    source_proof(&state, &headers, input.source_device_id, p.user_id).await?;
    if input.source_device_id == input.target_device_id {
        return Err(Error::Invalid("source equals target"));
    };
    let mut tx = state.pool.begin().await?;
    let source:Option<(bool,)> = sqlx::query_as("SELECT COALESCE(lease_until>now(),false) FROM devices WHERE id=$1 AND owner_id=$2 AND enabled FOR UPDATE")
        .bind(input.source_device_id).bind(p.user_id).fetch_optional(&mut *tx).await?;
    if !source.is_some_and(|(online,)| online) {
        return Err(Error::Forbidden);
    };
    let target:Option<(Uuid,bool,bool,bool)> = sqlx::query_as("SELECT owner_id,can_host,can_files,COALESCE(lease_until>now(),false) FROM devices WHERE id=$1 AND enabled FOR UPDATE")
        .bind(input.target_device_id).fetch_optional(&mut *tx).await?;
    let (owner, host, files, online) = target.ok_or(Error::NotFound)?;
    if !online
        || (input.permission == RemotePermission::Files && !files)
        || (input.permission != RemotePermission::Files && !host)
    {
        return Err(Error::Forbidden);
    };
    let mut invitation_id = None;
    if owner != p.user_id {
        let code = input.invitation_code.ok_or(Error::Forbidden)?;
        let row:Option<(Uuid,)> = sqlx::query_as("UPDATE invitations SET consumed_at=now() WHERE code_hash=$1 AND target_device_id=$2 AND permission=$3 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>now() RETURNING id")
            .bind(hash(&code)).bind(input.target_device_id).bind(input.permission.as_str()).fetch_optional(&mut *tx).await?;
        invitation_id = Some(row.ok_or(Error::Forbidden)?.0);
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO remote_sessions(id,requester_id,source_device_id,target_device_id,permission,state,nonce,auth_session_id,invitation_id) VALUES($1,$2,$3,$4,$5,'pending',$6,$7,$8)")
        .bind(id).bind(p.user_id).bind(input.source_device_id).bind(input.target_device_id).bind(input.permission.as_str())
        .bind(random_bytes()).bind(p.session_id).bind(invitation_id).execute(&mut *tx).await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "request_remote",
        Some(id),
        "pending",
    )
    .await?;
    Ok(Json(RequestOutput {
        id,
        state: "pending".into(),
    }))
}

pub async fn pending(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PendingView>>> {
    let (device_id, _) = device_principal(&state, &headers).await?;
    let rows:Vec<(Uuid,Uuid,Uuid,String)> = sqlx::query_as("SELECT id,requester_id,source_device_id,permission FROM remote_sessions WHERE target_device_id=$1 AND state='pending' ORDER BY created_at LIMIT 100")
        .bind(device_id).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, requester_id, source_device_id, permission)| PendingView {
                    id,
                    requester_id,
                    source_device_id,
                    permission,
                },
            )
            .collect(),
    ))
}

async fn grant(
    state: &AppState,
    id: Uuid,
    target: Uuid,
    require_active: bool,
) -> Result<GrantOutput> {
    let mut tx = state.pool.begin().await?;
    let row:Option<GrantRow> = sqlx::query_as(
        "SELECT r.permission,s.public_key,t.public_key,r.nonce FROM remote_sessions r JOIN devices s ON s.id=r.source_device_id JOIN devices t ON t.id=r.target_device_id JOIN auth_sessions a ON a.id=r.auth_session_id JOIN auth_sessions sa ON sa.id=s.credential_session_id JOIN auth_sessions ta ON ta.id=t.credential_session_id JOIN users u ON u.id=r.requester_id JOIN users tu ON tu.id=t.owner_id WHERE r.id=$1 AND r.target_device_id=$2 AND r.state='approved' AND (NOT $3 OR r.grant_until>now()) AND s.enabled AND t.enabled AND u.enabled AND tu.enabled AND a.revoked_at IS NULL AND a.refresh_expires_at>now() AND sa.revoked_at IS NULL AND sa.refresh_expires_at>now() AND ta.revoked_at IS NULL AND ta.refresh_expires_at>now() AND s.lease_until>now() AND t.lease_until>now() AND (r.invitation_id IS NULL OR EXISTS(SELECT 1 FROM invitations i WHERE i.id=r.invitation_id AND i.revoked_at IS NULL)) FOR UPDATE OF r")
        .bind(id).bind(target).bind(require_active).fetch_optional(&mut *tx).await?;
    let (permission, source_key, target_key, nonce) = row.ok_or(Error::Forbidden)?;
    let grant_token = token();
    sqlx::query("UPDATE remote_sessions SET grant_hash=$2,grant_until=now()+($3 * interval '1 second'),updated_at=now() WHERE id=$1")
        .bind(id).bind(hash(&grant_token)).bind(GRANT_SECONDS).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(GrantOutput {
        session_id: id,
        grant_token,
        permission,
        source_public_key: hex::encode(source_key),
        target_public_key: hex::encode(target_key),
        nonce: hex::encode(nonce),
        expires_in: GRANT_SECONDS,
    })
}

pub async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<Decision>,
) -> Result<Json<Option<GrantOutput>>> {
    let (target, _) = device_principal(&state, &headers).await?;
    let new_state = if input.approve { "approved" } else { "denied" };
    let r=sqlx::query("UPDATE remote_sessions SET state=$3,updated_at=now() WHERE id=$1 AND target_device_id=$2 AND state='pending' AND created_at>now()-interval '5 minutes' AND EXISTS(SELECT 1 FROM devices d WHERE d.id=$2 AND d.enabled AND d.lease_until>now())")
        .bind(id).bind(target).bind(new_state).execute(&state.pool).await?;
    if r.rows_affected() == 0 {
        return Err(Error::Conflict);
    };
    audit(&state.pool, None, "decide_remote", Some(id), new_state).await?;
    if input.approve {
        match grant(&state, id, target, false).await {
            Ok(g) => Ok(Json(Some(g))),
            Err(e) => {
                sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE id=$1 AND state='approved'")
                    .bind(id).execute(&state.pool).await?;
                Err(e)
            }
        }
    } else {
        Ok(Json(None))
    }
}
pub async fn renew(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<GrantOutput>> {
    let (target, _) = device_principal(&state, &headers).await?;
    Ok(Json(grant(&state, id, target, true).await?))
}
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let mut authorized = false;
    if let Ok(p) = principal(&state, &headers).await {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM remote_sessions WHERE id=$1 AND requester_id=$2")
                .bind(id)
                .bind(p.user_id)
                .fetch_optional(&state.pool)
                .await?;
        authorized = row.is_some();
    }
    if !authorized {
        let (device, _) = device_principal(&state, &headers).await?;
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM remote_sessions WHERE id=$1 AND target_device_id=$2")
                .bind(id)
                .bind(device)
                .fetch_optional(&state.pool)
                .await?;
        authorized = row.is_some();
    }
    if !authorized {
        return Err(Error::NotFound);
    };
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL,updated_at=now() WHERE id=$1 AND state IN ('pending','approved')")
        .bind(id).execute(&state.pool).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct GrantView {
    pub session_id: Uuid,
    pub permission: String,
    pub source_public_key: String,
    pub target_public_key: String,
    pub nonce: String,
    pub expires_at: String,
}
pub async fn inspect_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<GrantView>> {
    let grant_token = bearer(&headers)?;
    let proof = device_header(&headers)?;
    let row:Option<GrantInspectRow> = sqlx::query_as(
        "SELECT r.permission,s.public_key,t.public_key,r.nonce,r.grant_until FROM remote_sessions r JOIN devices s ON s.id=r.source_device_id JOIN devices t ON t.id=r.target_device_id JOIN auth_sessions a ON a.id=r.auth_session_id JOIN auth_sessions sa ON sa.id=s.credential_session_id JOIN auth_sessions ta ON ta.id=t.credential_session_id JOIN users u ON u.id=r.requester_id JOIN users tu ON tu.id=t.owner_id WHERE r.id=$1 AND r.grant_hash=$2 AND r.state='approved' AND r.grant_until>now() AND s.enabled AND t.enabled AND u.enabled AND tu.enabled AND a.revoked_at IS NULL AND a.refresh_expires_at>now() AND sa.revoked_at IS NULL AND sa.refresh_expires_at>now() AND ta.revoked_at IS NULL AND ta.refresh_expires_at>now() AND s.lease_until>now() AND t.lease_until>now() AND (s.credential_hash=$3 OR t.credential_hash=$3) AND (r.invitation_id IS NULL OR EXISTS(SELECT 1 FROM invitations i WHERE i.id=r.invitation_id AND i.revoked_at IS NULL))")
        .bind(id).bind(hash(grant_token)).bind(hash(&proof)).fetch_optional(&state.pool).await?;
    let (permission, source, target, nonce, expires_at) = row.ok_or(Error::Forbidden)?;
    Ok(Json(GrantView {
        session_id: id,
        permission,
        source_public_key: hex::encode(source),
        target_public_key: hex::encode(target),
        nonce: hex::encode(nonce),
        expires_at: expires_at.to_rfc3339(),
    }))
}
