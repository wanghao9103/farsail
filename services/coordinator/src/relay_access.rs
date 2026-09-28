//! Internal relay admission. The relay has already proved possession of this key.
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};

use crate::{Error, Result, bearer, hash};

#[derive(Clone)]
struct AccessState {
    pool: PgPool,
    secret_hash: Arc<Vec<u8>>,
}

/// Mount only on the loopback coordinator; the public proxy must block /internal/.
pub fn router(pool: PgPool, secret: &str) -> anyhow::Result<Router> {
    anyhow::ensure!(
        secret.len() >= 32,
        "relay admission secret must be at least 32 bytes"
    );
    Ok(Router::new()
        .route("/internal/relay-access", post(check))
        .with_state(AccessState {
            pool,
            secret_hash: Arc::new(hash(secret)),
        }))
}

async fn check(State(state): State<AccessState>, headers: HeaderMap) -> Result<Json<bool>> {
    // Compare fixed-size digests without early exit; never log the bearer or headers.
    let presented = hash(bearer(&headers)?);
    if presented
        .iter()
        .zip(state.secret_hash.iter())
        .fold(0u8, |d, (a, b)| d | (a ^ b))
        != 0
    {
        return Err(Error::Unauthorized);
    }
    let key = headers
        .get("x-iroh-nodeid")
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.len() == 64)
        .and_then(|s| hex::decode(s).ok())
        .ok_or(Error::Invalid("invalid endpoint public key"))?;
    let query = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM devices d JOIN users u ON u.id=d.owner_id \
         JOIN auth_sessions s ON s.id=d.credential_session_id \
         WHERE d.public_key=$1 AND d.bound AND d.enabled AND u.enabled AND u.verified \
         AND s.revoked_at IS NULL AND s.refresh_expires_at>now())",
    )
    .bind(key)
    .fetch_one(&state.pool);
    let allowed = tokio::time::timeout(Duration::from_secs(2), query)
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("relay admission database timeout")))??;
    Ok(Json(allowed))
}
