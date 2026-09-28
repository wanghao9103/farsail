use crate::{AppState, Error, Result, audit as record_audit, hash, principal, token};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
type AuditRow = (
    i64,
    Option<Uuid>,
    String,
    Option<Uuid>,
    String,
    chrono::DateTime<chrono::Utc>,
);

async fn admin(state: &AppState, headers: &HeaderMap) -> Result<Uuid> {
    let p = principal(state, headers).await?;
    if !p.admin {
        return Err(Error::Forbidden);
    };
    Ok(p.user_id)
}
#[derive(Deserialize)]
pub struct Page {
    pub limit: Option<i64>,
    pub after: Option<Uuid>,
}
#[derive(Serialize)]
pub struct UserView {
    pub id: Uuid,
    pub email: String,
    pub enabled: bool,
    pub verified: bool,
    pub role: String,
}
pub async fn users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Vec<UserView>>> {
    admin(&state, &headers).await?;
    let rows:Vec<(Uuid,String,bool,bool,String)>=sqlx::query_as("SELECT id,email,enabled,verified,role FROM users WHERE ($1::uuid IS NULL OR id>$1) ORDER BY id LIMIT $2")
        .bind(page.after).bind(page.limit.unwrap_or(50).clamp(1,100)).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, email, enabled, verified, role)| UserView {
                id,
                email,
                enabled,
                verified,
                role,
            })
            .collect(),
    ))
}
#[derive(Deserialize)]
pub struct Enabled {
    pub enabled: bool,
}
pub async fn set_user_enabled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<Enabled>,
) -> Result<()> {
    let actor = admin(&state, &headers).await?;
    if id == actor && !input.enabled {
        return Err(Error::Invalid("cannot disable self"));
    };
    let mut tx = state.pool.begin().await?;
    let r = sqlx::query("UPDATE users SET enabled=$2 WHERE id=$1")
        .bind(id)
        .bind(input.enabled)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    };
    if !input.enabled {
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let device_ids: Vec<(Uuid,)> =
            sqlx::query_as("SELECT id FROM devices WHERE owner_id=$1 AND bound FOR UPDATE")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        for (device_id,) in device_ids {
            sqlx::query("UPDATE devices SET lease_until=NULL,credential_hash=$2 WHERE id=$1")
                .bind(device_id)
                .bind(hash(&token()))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "UPDATE invitations SET revoked_at=now() WHERE creator_id=$1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (requester_id=$1 OR target_device_id IN (SELECT id FROM devices WHERE owner_id=$1)) AND state IN ('pending','approved')")
            .bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    record_audit(
        &state.pool,
        Some(actor),
        "set_user_enabled",
        Some(id),
        if input.enabled { "enabled" } else { "disabled" },
    )
    .await?;
    Ok(())
}
pub async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let actor = admin(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    let r=sqlx::query("UPDATE devices SET enabled=false,lease_until=NULL,credential_hash=$2 WHERE id=$1 AND bound AND enabled")
        .bind(id).bind(hash(&token())).execute(&mut *tx).await?;
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
    record_audit(
        &state.pool,
        Some(actor),
        "admin_revoke_device",
        Some(id),
        "ok",
    )
    .await?;
    Ok(())
}
pub async fn set_device_enabled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<Enabled>,
) -> Result<()> {
    let actor = admin(&state, &headers).await?;
    if !input.enabled {
        return revoke_device(State(state), headers, Path(id)).await;
    };
    let r = sqlx::query(
        "UPDATE devices SET enabled=true,lease_until=NULL WHERE id=$1 AND bound AND NOT enabled",
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    };
    record_audit(
        &state.pool,
        Some(actor),
        "admin_enable_device",
        Some(id),
        "ok",
    )
    .await?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
pub struct Registration {
    pub value: String,
}
pub async fn registration(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Registration>> {
    admin(&state, &headers).await?;
    let row: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key='registration'")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(Registration { value: row.0 }))
}
pub async fn set_registration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Registration>,
) -> Result<Json<Registration>> {
    let actor = admin(&state, &headers).await?;
    if input.value != "open" && input.value != "closed" && input.value != "invite_only" {
        return Err(Error::Invalid(
            "registration must be open, invite_only or closed",
        ));
    };
    sqlx::query("UPDATE settings SET value=$1 WHERE key='registration'")
        .bind(&input.value)
        .execute(&state.pool)
        .await?;
    record_audit(
        &state.pool,
        Some(actor),
        "registration_policy",
        None,
        &input.value,
    )
    .await?;
    Ok(Json(input))
}
#[derive(Serialize)]
pub struct SessionView {
    pub id: Uuid,
    pub user_id: Uuid,
    pub created_at: String,
    pub revoked: bool,
}
pub async fn sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionView>>> {
    admin(&state, &headers).await?;
    let rows:Vec<(Uuid,Uuid,chrono::DateTime<chrono::Utc>,bool)>=sqlx::query_as("SELECT id,user_id,created_at,revoked_at IS NOT NULL FROM auth_sessions ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, user_id, created_at, revoked)| SessionView {
                id,
                user_id,
                created_at: created_at.to_rfc3339(),
                revoked,
            })
            .collect(),
    ))
}
#[derive(Serialize)]
pub struct AuditView {
    pub id: i64,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub object_id: Option<Uuid>,
    pub result: String,
    pub created_at: String,
}
pub async fn audit(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AuditView>>> {
    admin(&state, &headers).await?;
    let rows:Vec<AuditRow>=sqlx::query_as("SELECT id,actor_id,action,object_id,result,created_at FROM audit_events ORDER BY id DESC LIMIT 100")
        .fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, actor_id, action, object_id, result, created_at)| AuditView {
                    id,
                    actor_id,
                    action,
                    object_id,
                    result,
                    created_at: created_at.to_rfc3339(),
                },
            )
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct SignupInviteInput {
    pub email: String,
}
#[derive(Serialize)]
pub struct SignupInviteOutput {
    pub id: Uuid,
    pub code: String,
    pub expires_in: i64,
}
pub async fn create_signup_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SignupInviteInput>,
) -> Result<Json<SignupInviteOutput>> {
    let actor = admin(&state, &headers).await?;
    let email = input.email.trim().to_ascii_lowercase();
    if email.len() > 254 || !email.contains('@') || email.contains(char::is_whitespace) {
        return Err(Error::Invalid("invalid email"));
    }
    let id = Uuid::new_v4();
    let code = token();
    sqlx::query("INSERT INTO signup_invitations(id,created_by,email,code_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '7 days')")
        .bind(id).bind(actor).bind(email).bind(hash(&code)).execute(&state.pool).await?;
    record_audit(
        &state.pool,
        Some(actor),
        "create_signup_invitation",
        Some(id),
        "ok",
    )
    .await?;
    Ok(Json(SignupInviteOutput {
        id,
        code,
        expires_in: 604800,
    }))
}
pub async fn revoke_signup_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let actor = admin(&state, &headers).await?;
    let r=sqlx::query("UPDATE signup_invitations SET revoked_at=now() WHERE id=$1 AND consumed_at IS NULL AND revoked_at IS NULL")
        .bind(id).execute(&state.pool).await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    record_audit(
        &state.pool,
        Some(actor),
        "revoke_signup_invitation",
        Some(id),
        "ok",
    )
    .await?;
    Ok(())
}
