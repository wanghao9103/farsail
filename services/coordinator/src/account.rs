use crate::{AppState, Error, IdResponse, MailPurpose, Result, audit, hash, principal, token};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use farsail_core::{ACCESS_SECONDS, REFRESH_SECONDS};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
    pub invite_code: Option<String>,
}
#[derive(Deserialize)]
pub struct EmailToken {
    pub token: String,
    pub email: Option<String>,
}
#[derive(Deserialize)]
pub struct RecoveryRequest {
    pub email: String,
}
#[derive(Deserialize)]
pub struct RecoveryComplete {
    pub token: String,
    pub password: String,
}
#[derive(Deserialize)]
pub struct ChangePassword {
    pub old_password: String,
    pub new_password: String,
}
#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub session_id: Uuid,
    pub access_expires_in: i64,
}

fn normalized(email: &str) -> Result<String> {
    let email = email.trim().to_ascii_lowercase();
    if email.len() > 254
        || email.len() < 3
        || !email.contains('@')
        || email.contains(char::is_whitespace)
    {
        return Err(Error::Invalid("invalid email"));
    }
    Ok(email)
}
fn password_hash_sync(password: &str) -> Result<String> {
    if password.len() < 12 || password.len() > 1024 {
        return Err(Error::Invalid("password length must be 12..1024"));
    }
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| Error::Internal(anyhow::anyhow!("password hash: {e}")))?
        .to_string())
}
fn check_password_sync(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|p| {
        Argon2::default()
            .verify_password(password.as_bytes(), &p)
            .is_ok()
    })
}
fn password_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(4)))
}
async fn password_hash(password: String) -> Result<String> {
    let permit = password_slots()
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| Error::Internal(e.into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        password_hash_sync(&password)
    })
    .await
    .map_err(|e| Error::Internal(e.into()))?
}
async fn check_password(password: String, encoded: String) -> Result<bool> {
    let permit = password_slots()
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| Error::Internal(e.into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        check_password_sync(&password, &encoded)
    })
    .await
    .map_err(|e| Error::Internal(e.into()))
}

pub(crate) async fn rate_limit(state: &AppState, key: &str, max: i32) -> Result<()> {
    if rand::random::<u8>() == 0 {
        sqlx::query("DELETE FROM rate_limits WHERE window_start<now()-interval '1 day'")
            .execute(&state.pool)
            .await?;
    }
    let count: (i32,) = sqlx::query_as(
        "INSERT INTO rate_limits(key,attempts,window_start) VALUES($1,1,now()) ON CONFLICT(key) DO UPDATE SET attempts=CASE WHEN rate_limits.window_start<now()-interval '15 minutes' THEN 1 ELSE rate_limits.attempts+1 END, window_start=CASE WHEN rate_limits.window_start<now()-interval '15 minutes' THEN now() ELSE rate_limits.window_start END RETURNING attempts")
        .bind(hex::encode(hash(key))).fetch_one(&state.pool).await?;
    if count.0 > max {
        Err(Error::RateLimited)
    } else {
        Ok(())
    }
}

fn verification_hash(email: &str, code: &str) -> Vec<u8> {
    hash(&format!("verify-code:{email}:{code}"))
}

async fn issue_verification(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    email: &str,
) -> Result<String> {
    // Serialize issuance with verification and concurrent resends for this account.
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await?;
    for _ in 0..8 {
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000_u32));
        let digest = verification_hash(email, &code);
        let inserted = sqlx::query("INSERT INTO email_tokens(id,user_id,kind,token_hash,expires_at) VALUES($1,$2,'verify',$3,now()+interval '10 minutes') ON CONFLICT(token_hash) DO NOTHING")
            .bind(Uuid::new_v4()).bind(user_id).bind(&digest).execute(&mut **tx).await?;
        if inserted.rows_affected() == 0 {
            continue;
        }
        sqlx::query("UPDATE email_tokens SET consumed_at=now() WHERE user_id=$1 AND kind='verify' AND token_hash<>$2 AND consumed_at IS NULL")
            .bind(user_id).bind(&digest).execute(&mut **tx).await?;
        return Ok(code);
    }
    Err(Error::Internal(anyhow::anyhow!(
        "verification code allocation failed"
    )))
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<Credentials>,
) -> Result<Json<IdResponse>> {
    let email = normalized(&input.email)?;
    rate_limit(&state, &format!("register:{email}"), 5).await?;
    rate_limit(&state, "register:global", 1000).await?;
    let encoded = password_hash(input.password).await?;
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    let setting: (String,) =
        sqlx::query_as("SELECT value FROM settings WHERE key='registration' FOR SHARE")
            .fetch_one(&mut *tx)
            .await?;
    match setting.0.as_str() {
        "open" => {}
        "invite_only" => {
            let code = input.invite_code.ok_or(Error::Forbidden)?;
            let invite=sqlx::query("UPDATE signup_invitations SET consumed_at=now() WHERE code_hash=$1 AND email=$2 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>now()")
                .bind(hash(&code)).bind(&email).execute(&mut *tx).await?;
            if invite.rows_affected() == 0 {
                return Err(Error::Forbidden);
            };
        }
        _ => return Err(Error::Forbidden),
    }
    let inserted = sqlx::query(
        "INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3) ON CONFLICT(email) DO NOTHING",
    )
    .bind(id)
    .bind(&email)
    .bind(encoded)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(Error::Conflict);
    }
    let verify_token = issue_verification(&mut tx, id, &email).await?;
    tx.commit().await?;
    state
        .mailer
        .send(&email, MailPurpose::Verification, &verify_token)
        .await?;
    audit(&state.pool, Some(id), "register", Some(id), "ok").await?;
    Ok(Json(IdResponse { id }))
}

pub async fn verify(State(state): State<AppState>, Json(input): Json<EmailToken>) -> Result<()> {
    rate_limit(&state, "verify:global", 1000).await?;
    let code = input.token.trim();
    let numeric = code.len() == 6 && code.bytes().all(|c| c.is_ascii_digit());
    let email = input.email.as_deref().map(normalized).transpose()?;
    let digest = if numeric {
        let email = email
            .as_deref()
            .ok_or(Error::Invalid("email required for verification code"))?;
        rate_limit(&state, &format!("verify:{email}"), 5).await?;
        verification_hash(email, code)
    } else {
        // Keep already-issued, high-entropy legacy tokens valid during upgrade.
        if code.len() != 43
            || !code
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(Error::Invalid("invalid or expired verification code"));
        }
        hash(code)
    };
    let mut tx = state.pool.begin().await?;
    // Lock the user before consuming the code, matching resend lock order.
    let owner: Option<(Uuid,)> = sqlx::query_as("SELECT u.id FROM users u JOIN email_tokens t ON t.user_id=u.id WHERE t.token_hash=$1 AND t.kind='verify' AND ($2::text IS NULL OR u.email=$2) AND u.enabled FOR UPDATE OF u")
        .bind(&digest).bind(&email).fetch_optional(&mut *tx).await?;
    let (owner_id,) = owner.ok_or(Error::Invalid("invalid or expired verification code"))?;
    let row: Option<(Uuid,)> = sqlx::query_as("UPDATE email_tokens SET consumed_at=now() WHERE token_hash=$1 AND user_id=$2 AND kind='verify' AND consumed_at IS NULL AND expires_at>now() RETURNING user_id")
        .bind(&digest).bind(owner_id).fetch_optional(&mut *tx).await?;
    let (user_id,) = row.ok_or(Error::Invalid("invalid or expired verification code"))?;
    sqlx::query("UPDATE users SET verified=true WHERE id=$1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(user_id),
        "verify_email",
        Some(user_id),
        "ok",
    )
    .await?;
    Ok(())
}

pub async fn resend_verification(
    State(state): State<AppState>,
    Json(input): Json<Credentials>,
) -> Result<()> {
    let email = normalized(&input.email)?;
    rate_limit(&state, &format!("resend:{email}"), 5).await?;
    rate_limit(&state, "resend:global", 1000).await?;
    tracing::info!(event = "verification_resend", outcome = "requested");
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id,password_hash FROM users WHERE email=$1 AND enabled AND NOT verified",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    if let Some((id, encoded)) = row {
        if !check_password(input.password, encoded).await? {
            tracing::info!(
                event = "verification_resend",
                outcome = "skipped",
                reason = "password_mismatch"
            );
            return Ok(());
        }
        let mut tx = state.pool.begin().await?;
        let eligible: (bool,) =
            sqlx::query_as("SELECT enabled AND NOT verified FROM users WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if !eligible.0 {
            tracing::info!(
                event = "verification_resend",
                outcome = "skipped",
                reason = "account_not_eligible"
            );
            return Ok(());
        }
        let verify_token = issue_verification(&mut tx, id, &email).await?;
        tx.commit().await?;
        state
            .mailer
            .send(&email, MailPurpose::Verification, &verify_token)
            .await?;
        tracing::info!(
            event = "verification_resend",
            outcome = "submitted_to_mailer"
        );
    } else {
        tracing::info!(
            event = "verification_resend",
            outcome = "skipped",
            reason = "account_not_eligible"
        );
    }
    Ok(())
}

async fn issue(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> Result<Tokens> {
    let access_token = token();
    let refresh_token = token();
    let session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,access_hash,refresh_hash,access_expires_at,refresh_expires_at) VALUES($1,$2,$3,$4,now()+($5 * interval '1 second'),now()+($6 * interval '1 second'))")
        .bind(session_id).bind(user_id).bind(hash(&access_token)).bind(hash(&refresh_token))
        .bind(ACCESS_SECONDS).bind(REFRESH_SECONDS).execute(&mut **tx).await?;
    Ok(Tokens {
        access_token,
        refresh_token,
        session_id,
        access_expires_in: ACCESS_SECONDS,
    })
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<Credentials>,
) -> Result<Json<Tokens>> {
    let email = normalized(&input.email)?;
    rate_limit(&state, &format!("login:{email}"), 10).await?;
    rate_limit(&state, "login:global", 1000).await?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id,password_hash,verified,enabled FROM users WHERE email=$1 FOR UPDATE",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    let (id, encoded, verified, enabled) = row.ok_or(Error::LoginFailure("account_not_found"))?;
    if !check_password(input.password, encoded).await? {
        return Err(Error::LoginFailure("invalid_password"));
    }
    if !enabled {
        return Err(Error::LoginFailure("account_disabled"));
    }
    if !verified {
        return Err(Error::LoginFailure("email_not_verified"));
    }
    let tokens = issue(&mut tx, id).await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(id),
        "login",
        Some(tokens.session_id),
        "ok",
    )
    .await?;
    Ok(Json(tokens))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(input): Json<RefreshRequest>,
) -> Result<Json<Tokens>> {
    let presented = hash(&input.refresh_token);
    let mut tx = state.pool.begin().await?;
    let row: Option<(Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT s.id,s.user_id,u.enabled FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.refresh_hash=$1 AND s.revoked_at IS NULL AND s.refresh_expires_at>now() FOR UPDATE OF s")
        .bind(&presented).fetch_optional(&mut *tx).await?;
    if let Some((id, _user_id, enabled)) = row {
        if !enabled {
            return Err(Error::Unauthorized);
        }
        let access_token = token();
        let refresh_token = token();
        sqlx::query("INSERT INTO used_refresh_tokens(token_hash,session_id) VALUES($1,$2)")
            .bind(&presented)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE auth_sessions SET access_hash=$2,refresh_hash=$3,access_expires_at=now()+($4 * interval '1 second'),refresh_expires_at=now()+($5 * interval '1 second'),last_used_at=now() WHERE id=$1")
            .bind(id).bind(hash(&access_token)).bind(hash(&refresh_token)).bind(ACCESS_SECONDS).bind(REFRESH_SECONDS).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(Json(Tokens {
            access_token,
            refresh_token,
            session_id: id,
            access_expires_in: ACCESS_SECONDS,
        }));
    }
    let reused: Option<(Uuid,)> =
        sqlx::query_as("SELECT session_id FROM used_refresh_tokens WHERE token_hash=$1")
            .bind(&presented)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some((id,)) = reused {
        revoke_auth(&mut tx, id).await?;
        tx.commit().await?;
    }
    Err(Error::Unauthorized)
}

async fn revoke_auth(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE auth_sessions SET revoked_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE auth_session_id=$1 AND state IN ('pending','approved')")
        .bind(id).execute(&mut **tx).await?;
    sqlx::query("UPDATE devices SET lease_until=NULL WHERE credential_session_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (source_device_id IN (SELECT id FROM devices WHERE credential_session_id=$1) OR target_device_id IN (SELECT id FROM devices WHERE credential_session_id=$1)) AND state IN ('pending','approved')")
        .bind(id).execute(&mut **tx).await?;
    Ok(())
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<()> {
    let p = principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    revoke_auth(&mut tx, p.session_id).await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "logout",
        Some(p.session_id),
        "ok",
    )
    .await?;
    Ok(())
}

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ChangePassword>,
) -> Result<()> {
    let p = principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    let row: (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id=$1 FOR UPDATE")
        .bind(p.user_id)
        .fetch_one(&mut *tx)
        .await?;
    if !check_password(input.old_password, row.0).await? {
        return Err(Error::Unauthorized);
    }
    let encoded = password_hash(input.new_password).await?;
    sqlx::query(
        "UPDATE users SET password_hash=$2,credential_version=credential_version+1 WHERE id=$1",
    )
    .bind(p.user_id)
    .bind(encoded)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE email_tokens SET consumed_at=now() WHERE user_id=$1 AND kind='reset' AND consumed_at IS NULL")
        .bind(p.user_id).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL",
    )
    .bind(p.user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (requester_id=$1 OR target_device_id IN (SELECT id FROM devices WHERE owner_id=$1)) AND state IN ('pending','approved')")
        .bind(p.user_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE devices SET lease_until=NULL WHERE owner_id=$1")
        .bind(p.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "change_password",
        Some(p.user_id),
        "ok",
    )
    .await?;
    Ok(())
}

pub async fn request_recovery(
    State(state): State<AppState>,
    Json(input): Json<RecoveryRequest>,
) -> Result<()> {
    let email = normalized(&input.email)?;
    rate_limit(&state, &format!("recover:{email}"), 5).await?;
    rate_limit(&state, "recover:global", 1000).await?;
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email=$1 AND enabled")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    if let Some((id,)) = row {
        let reset_token = token();
        sqlx::query("INSERT INTO email_tokens(id,user_id,kind,token_hash,expires_at) VALUES($1,$2,'reset',$3,now()+interval '30 minutes')")
            .bind(Uuid::new_v4()).bind(id).bind(hash(&reset_token)).execute(&state.pool).await?;
        state
            .mailer
            .send(&email, MailPurpose::Recovery, &reset_token)
            .await?;
    }
    Ok(())
}

pub async fn complete_recovery(
    State(state): State<AppState>,
    Json(input): Json<RecoveryComplete>,
) -> Result<()> {
    rate_limit(
        &state,
        &format!("reset:{}", hex::encode(hash(&input.token))),
        10,
    )
    .await?;
    rate_limit(&state, "reset:global", 1000).await?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT user_id FROM email_tokens WHERE token_hash=$1 AND kind='reset' AND consumed_at IS NULL AND expires_at>now()")
        .bind(hash(&input.token)).fetch_optional(&mut *tx).await?;
    let (id,) = row.ok_or(Error::Invalid("invalid or expired token"))?;
    let locked: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE id=$1 AND enabled FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if locked.is_none() {
        return Err(Error::Forbidden);
    };
    let consumed=sqlx::query("UPDATE email_tokens SET consumed_at=now() WHERE token_hash=$1 AND consumed_at IS NULL AND expires_at>now()")
        .bind(hash(&input.token)).execute(&mut *tx).await?;
    if consumed.rows_affected() == 0 {
        return Err(Error::Invalid("invalid or expired token"));
    };
    let encoded = password_hash(input.password).await?;
    sqlx::query(
        "UPDATE users SET password_hash=$2,credential_version=credential_version+1 WHERE id=$1",
    )
    .bind(id)
    .bind(encoded)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE email_tokens SET consumed_at=now() WHERE user_id=$1 AND kind='reset' AND consumed_at IS NULL")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE remote_sessions SET state='revoked',grant_until=NULL WHERE (requester_id=$1 OR target_device_id IN (SELECT id FROM devices WHERE owner_id=$1)) AND state IN ('pending','approved')")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE devices SET lease_until=NULL WHERE owner_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(&state.pool, Some(id), "recover_password", Some(id), "ok").await?;
    Ok(())
}

#[derive(Serialize)]
pub struct SessionView {
    id: Uuid,
    created_at: String,
    last_used_at: String,
    current: bool,
}
#[derive(Serialize)]
pub struct MeView {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub session_id: Uuid,
}
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<MeView>> {
    let p = principal(&state, &headers).await?;
    let row: (String, String) = sqlx::query_as("SELECT email,role FROM users WHERE id=$1")
        .bind(p.user_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(MeView {
        id: p.user_id,
        email: row.0,
        role: row.1,
        session_id: p.session_id,
    }))
}
pub async fn sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionView>>> {
    let p = principal(&state, &headers).await?;
    let rows: Vec<(Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id,created_at,last_used_at FROM auth_sessions WHERE user_id=$1 AND revoked_at IS NULL AND refresh_expires_at>now() ORDER BY created_at DESC")
        .bind(p.user_id).fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, created, last)| SessionView {
                id,
                created_at: created.to_rfc3339(),
                last_used_at: last.to_rfc3339(),
                current: id == p.session_id,
            })
            .collect(),
    ))
}
pub async fn revoke_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<()> {
    let p = principal(&state, &headers).await?;
    let mut tx = state.pool.begin().await?;
    let owner: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM auth_sessions WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if owner.map(|x| x.0) != Some(p.user_id) {
        return Err(Error::NotFound);
    }
    revoke_auth(&mut tx, id).await?;
    tx.commit().await?;
    audit(
        &state.pool,
        Some(p.user_id),
        "revoke_session",
        Some(id),
        "ok",
    )
    .await?;
    Ok(())
}
