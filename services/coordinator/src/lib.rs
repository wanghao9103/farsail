mod account;
mod admin;
mod device;
mod endpoint_address;
pub mod relay_access;
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
    #[error("{0}")]
    LoginFailure(&'static str),
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
            Self::Unauthorized | Self::LoginFailure(_) => StatusCode::UNAUTHORIZED,
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

#[derive(Clone, Copy)]
pub(crate) enum MailPurpose {
    Verification,
    Recovery,
}

impl MailPurpose {
    fn content(self, code: &str) -> (String, String, String) {
        let (title, label, purpose, instructions, minutes) = match self {
            Self::Verification => (
                "邮箱验证",
                "验证码",
                "完成 FarSail 账号的邮箱验证",
                "请返回 FarSail 的“验证邮箱”页面，输入以下验证码。",
                10,
            ),
            Self::Recovery => (
                "密码重置",
                "密码重置码",
                "重置 FarSail 账号密码",
                "请返回 FarSail 的密码重置页面，复制完整重置码并设置新密码。",
                30,
            ),
        };
        let subject = format!("FarSail 遥舟｜{title}");
        let text = format!(
            "您好：\n\n您正在{purpose}。\n{instructions}\n\n{label}：{code}\n\n有效期为 {minutes} 分钟，请尽快完成操作。此代码仅可使用一次，请勿转发或告知他人。\n如重新申请，请使用最新邮件中的代码。\n\n如果您未发起此操作，请忽略本邮件。\n本邮件由系统自动发送，请勿直接回复。\n\nFarSail 遥舟\n连接你的电脑，继续你的工作。\n"
        );
        let escaped = code
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;");
        let html = format!(
            r#"<!doctype html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1"></head><body style="margin:0;background:#f4f7f8;color:#24343c;font-family:Arial,'Microsoft YaHei',sans-serif"><table role="presentation" style="width:100%;border-collapse:collapse"><tr><td style="padding:32px 16px"><table role="presentation" style="max-width:560px;width:100%;margin:auto;background:#fff;border:1px solid #e0e7ea;border-radius:12px"><tr><td style="padding:32px"><p style="margin:0 0 24px;color:#087a78;font-size:18px;font-weight:bold">FarSail 遥舟</p><h1 style="font-size:24px;margin:0 0 24px">{title}</h1><p>您好：</p><p style="line-height:1.8">您正在{purpose}。<br>{instructions}</p><p style="margin:24px 0 8px;color:#5b6b73">{label}</p><div style="padding:20px;background:#eef8f6;border:1px solid #cce8e1;border-radius:8px;font-family:monospace;font-size:28px;font-weight:bold;letter-spacing:3px;overflow-wrap:anywhere;word-break:break-all">{escaped}</div><p style="line-height:1.8">有效期为 <strong>{minutes} 分钟</strong>，请尽快完成操作。此代码仅可使用一次，请勿转发或告知他人。如重新申请，请使用最新邮件中的代码。</p><p style="line-height:1.8;color:#5b6b73">如果您未发起此操作，请忽略本邮件。<br>本邮件由系统自动发送，请勿直接回复。</p><hr style="border:0;border-top:1px solid #e0e7ea;margin:24px 0"><p style="font-size:13px;color:#5b6b73;margin:0">FarSail 遥舟 · 连接你的电脑，继续你的工作。</p></td></tr></table></td></tr></table></body></html>"#
        );
        (subject, text, html)
    }
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

    pub(crate) async fn send(&self, to: &str, purpose: MailPurpose, token: &str) -> Result<()> {
        let (subject, text, html) = purpose.content(token);
        tracing::info!(event = "mail_delivery", outcome = "started");
        match self {
            Self::Memory(m) => m.lock().expect("mailer mutex poisoned").push(Mail {
                to: to.to_owned(),
                subject: subject.clone(),
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
                    .multipart(lettre::message::MultiPart::alternative_plain_html(
                        text, html,
                    ))
                    .map_err(|e| Error::Internal(e.into()))?;
                if let Err(error) = transport.send(message).await {
                    tracing::error!(event = "mail_delivery", outcome = "failed");
                    return Err(Error::Internal(error.into()));
                }
                tracing::info!(event = "mail_delivery", outcome = "smtp_accepted");
            }
        }
        if matches!(self, Self::Memory(_)) {
            tracing::info!(event = "mail_delivery", outcome = "test_memory_only");
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
        .route("/v1/devices/capability", post(device::capability))
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

#[cfg(test)]
mod mail_tests {
    use super::*;

    #[test]
    fn formal_mail_has_matching_plain_and_html_content() {
        for (purpose, code, minutes) in [
            (MailPurpose::Verification, "012345", 10),
            (MailPurpose::Recovery, "synthetic-reset-code", 30),
        ] {
            let (subject, text, html) = purpose.content(code);
            assert!(subject.starts_with("FarSail 遥舟｜"));
            for body in [&text, &html] {
                assert!(body.contains(code));
                assert!(body.contains(&format!("{minutes} 分钟")));
                assert!(body.contains("仅可使用一次"));
                assert!(!body.contains("token"));
            }
            let message = lettre::Message::builder()
                .from("FarSail <sender@example.invalid>".parse().unwrap())
                .to("recipient@example.invalid".parse().unwrap())
                .subject(subject)
                .multipart(lettre::message::MultiPart::alternative_plain_html(
                    text, html,
                ))
                .unwrap();
            let mime = String::from_utf8(message.formatted()).unwrap();
            assert!(mime.contains("multipart/alternative"));
            assert!(mime.contains("text/plain"));
            assert!(mime.contains("text/html"));
            assert!(mime.contains("charset=utf-8"));
        }
    }

    #[test]
    fn html_content_escapes_inserted_code() {
        let (_, text, html) = MailPurpose::Verification.content("<script>&\"'");
        assert!(text.contains("<script>"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;&amp;&quot;&#39;"));
    }
}
