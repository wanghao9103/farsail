mod account;
mod admin;
mod device;
mod endpoint_address;
mod remote;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    Invalid(&'static str),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("too many attempts")]
    RateLimited,
    #[error("internal server error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if matches!(self, Self::Internal(_)) {
            tracing::error!(error = ?self, "request failed");
        }
        (status, Json(serde_json::json!({"error": self.to_string()}))).into_response()
    }
}

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Self::Internal(value.into())
    }
}

#[derive(Clone)]
pub enum Mailer {
    Memory(Arc<Mutex<Vec<Mail>>>),
    Smtp(lettre::AsyncSmtpTransport<lettre::Tokio1Executor>, String),
}

#[derive(Clone, Debug)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub token: String,
}

impl Mailer {
    pub fn memory() -> Self {
        Self::Memory(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn messages(&self) -> Vec<Mail> {
        match self {
            Self::Memory(m) => m.lock().expect("mailer mutex poisoned").clone(),
            Self::Smtp(..) => Vec::new(),
        }
    }

    pub(crate) async fn send(&self, to: &str, subject: &str, token: &str) -> Result<()> {
        match self {
            Self::Memory(m) => m.lock().expect("mailer mutex poisoned").push(Mail {
                to: to.to_owned(),
                subject: subject.to_owned(),
                token: token.to_owned(),
            }),
            Self::Smtp(transport, from) => {
                use lettre::{AsyncTransport, Message};
                let message = Message::builder()
                    .from(from.parse().map_err(|e| {
                        Error::Internal(anyhow::anyhow!("invalid mail sender: {e}"))
                    })?)
                    .to(to.parse().map_err(|_| Error::Invalid("invalid email"))?)
                    .subject(subject)
                    .body(format!("FarSail one-time token: {token}\n"))
                    .map_err(|e| Error::Internal(e.into()))?;
                transport
                    .send(message)
                    .await
                    .map_err(|e| Error::Internal(e.into()))?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub mailer: Mailer,
    pub allowed_relays: Arc<Vec<iroh::RelayUrl>>,
}

impl AppState {
    pub async fn connect(database_url: &str, mailer: Mailer) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| Error::Internal(e.into()))?;
        let mut allowed_relays = Vec::new();
        if let Ok(value) = std::env::var("FARSAIL_RELAY_URLS") {
            for raw in value.split(',').filter(|s| !s.is_empty()) {
                let url: iroh::RelayUrl = raw
                    .parse()
                    .map_err(|_| Error::Invalid("invalid relay URL"))?;
                if !endpoint_address::valid_relay(&url) {
                    return Err(Error::Invalid("invalid relay URL"));
                }
                allowed_relays.push(url);
            }
        }
        Ok(Self {
            pool,
            mailer,
            allowed_relays: Arc::new(allowed_relays),
        })
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/auth/register", post(account::register))
        .route("/v1/auth/verify", post(account::verify))
        .route("/v1/auth/verify/resend", post(account::resend_verification))
        .route("/v1/auth/login", post(account::login))
        .route("/v1/auth/refresh", post(account::refresh))
        .route("/v1/auth/logout", post(account::logout))
        .route("/v1/auth/password", post(account::change_password))
        .route("/v1/auth/recovery/request", post(account::request_recovery))
        .route(
            "/v1/auth/recovery/complete",
            post(account::complete_recovery),
        )
        .route("/v1/auth/sessions", get(account::sessions))
        .route("/v1/me", get(account::me))
        .route(
            "/v1/auth/sessions/{id}/revoke",
            post(account::revoke_session),
        )
        .route("/v1/devices/challenge", post(device::challenge))
        .route("/v1/devices/bind", post(device::bind))
        .route("/v1/devices", get(device::list))
        .route(
            "/v1/devices/{id}",
            get(device::get_device)
                .patch(device::rename)
                .delete(device::unbind),
        )
        .route("/v1/devices/heartbeat", post(device::heartbeat))
        .route(
            "/v1/devices/endpoint-address",
            post(endpoint_address::publish),
        )
        .route("/v1/invitations", post(remote::invite))
        .route(
            "/v1/invitations/{id}/revoke",
            post(remote::revoke_invitation),
        )
        .route("/v1/remote/request", post(remote::request))
        .route("/v1/remote/sessions", get(remote::sessions))
        .route("/v1/remote/pending", get(remote::pending))
        .route("/v1/remote/{id}", get(remote::status))
        .route("/v1/remote/{id}/decide", post(remote::decide))
        .route("/v1/remote/{id}/renew", post(remote::renew))
        .route(
            "/v1/remote/{id}/transport-renew",
            post(remote::transport_renew),
        )
        .route("/v1/remote/{id}/revoke", post(remote::revoke))
        .route("/v1/remote/{id}/grant", get(remote::inspect_grant))
        .route(
            "/v1/remote/{id}/transport-grant",
            get(remote::inspect_transport_grant),
        )
        .route("/v1/remote/{id}/peer-address", get(endpoint_address::peer))
        .route("/v1/admin/users", get(admin::users))
        .route(
            "/v1/admin/users/{id}/enabled",
            post(admin::set_user_enabled),
        )
        .route("/v1/admin/devices/{id}/revoke", post(admin::revoke_device))
        .route(
            "/v1/admin/devices/{id}/enabled",
            post(admin::set_device_enabled),
        )
        .route(
            "/v1/admin/registration",
            get(admin::registration).put(admin::set_registration),
        )
        .route(
            "/v1/admin/signup-invitations",
            post(admin::create_signup_invitation),
        )
        .route(
            "/v1/admin/signup-invitations/{id}",
            axum::routing::delete(admin::revoke_signup_invitation),
        )
        .route("/v1/admin/sessions", get(admin::sessions))
        .route("/v1/admin/audit", get(admin::audit))
        .with_state(state)
}

pub fn token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

pub fn hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

pub fn random_bytes() -> Vec<u8> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.to_vec()
}

#[derive(Clone, Debug)]
pub struct Principal {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub admin: bool,
}

pub async fn principal(state: &AppState, headers: &HeaderMap) -> Result<Principal> {
    let token = bearer(headers)?;
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT s.id,s.user_id,u.role FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.access_hash=$1 AND s.revoked_at IS NULL AND s.access_expires_at>now() AND u.enabled")
        .bind(hash(token)).fetch_optional(&state.pool).await?;
    let (session_id, user_id, role) = row.ok_or(Error::Unauthorized)?;
    Ok(Principal {
        user_id,
        session_id,
        admin: role == "admin",
    })
}

pub fn bearer(headers: &HeaderMap) -> Result<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|s| !s.is_empty())
        .ok_or(Error::Unauthorized)
}

pub async fn audit(
    pool: &PgPool,
    actor: Option<Uuid>,
    action: &str,
    object: Option<Uuid>,
    result: &str,
) -> Result<()> {
    if let Err(error) = sqlx::query(
        "INSERT INTO audit_events(actor_id,action,object_id,result) VALUES($1,$2,$3,$4)",
    )
    .bind(actor)
    .bind(action)
    .bind(object)
    .bind(result)
    .execute(pool)
    .await
    {
        tracing::error!(%error, %action, "audit persistence failed after committed action");
    }
    Ok(())
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: Uuid,
}

pub async fn health(State(state): State<AppState>) -> Result<StatusCode> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(StatusCode::OK)
}
