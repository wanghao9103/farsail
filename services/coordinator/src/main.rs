use farsail_coordinator::{AppState, Mailer, router};
use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials};
use std::{
    env,
    net::{IpAddr, SocketAddr},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let database_url = env::var("FARSAIL_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("FARSAIL_DATABASE_URL is required"))?;
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|x| x == "bootstrap-admin") {
        let email = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("usage: farsail-coordinator bootstrap-admin EMAIL"))?;
        let pool = sqlx::PgPool::connect(&database_url).await?;
        let mut tx = pool.begin().await?;
        let r: Option<(uuid::Uuid,)> = sqlx::query_as(
            "UPDATE users SET role='admin' WHERE email=$1 AND verified AND enabled RETURNING id",
        )
        .bind(email.trim().to_ascii_lowercase())
        .fetch_optional(&mut *tx)
        .await?;
        let (id,) = r.ok_or_else(|| anyhow::anyhow!("verified enabled account not found"))?;
        sqlx::query("INSERT INTO audit_events(actor_id,action,object_id,result) VALUES(NULL,'bootstrap_admin',$1,'ok')")
            .bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        println!("admin role assigned");
        return Ok(());
    }
    let bind: SocketAddr = env::var("FARSAIL_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()?;
    anyhow::ensure!(
        matches!(bind.ip(),IpAddr::V4(ip) if ip.is_loopback())
            || matches!(bind.ip(),IpAddr::V6(ip) if ip.is_loopback()),
        "coordinator must bind loopback; terminate HTTPS in a reverse proxy"
    );
    let mode = env::var("FARSAIL_MAIL_MODE").unwrap_or_else(|_| "smtp-local".into());
    tracing::info!(mail_mode = %mode, "mailer configuration selected");
    let mailer = match mode.as_str() {
        "memory" if env::var("FARSAIL_DEV_MEMORY_MAIL").as_deref() == Ok("1") => Mailer::memory(),
        "smtp-local" => {
            let host = env::var("FARSAIL_SMTP_HOST").unwrap_or_else(|_| "127.0.0.1".into());
            anyhow::ensure!(
                host == "127.0.0.1" || host == "localhost" || host == "::1",
                "smtp-local must use loopback"
            );
            let port: u16 = env::var("FARSAIL_SMTP_PORT")
                .unwrap_or_else(|_| "1025".into())
                .parse()?;
            let from = env::var("FARSAIL_MAIL_FROM")
                .unwrap_or_else(|_| "FarSail <noreply@localhost>".into());
            Mailer::Smtp(
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                    .port(port)
                    .build(),
                from,
            )
        }
        "smtp-tls" => {
            let host = env::var("FARSAIL_SMTP_HOST")?;
            let username = env::var("FARSAIL_SMTP_USER")?;
            let password = env::var("FARSAIL_SMTP_PASSWORD")?;
            let from = env::var("FARSAIL_MAIL_FROM")?;
            let tls = env::var("FARSAIL_SMTP_TLS").unwrap_or_else(|_| "implicit".into());
            let builder = match tls.as_str() {
                "implicit" => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?,
                "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?,
                _ => anyhow::bail!("FARSAIL_SMTP_TLS must be implicit or starttls"),
            };
            let port: u16 = env::var("FARSAIL_SMTP_PORT")
                .unwrap_or_else(|_| if tls == "starttls" { "587" } else { "465" }.into())
                .parse()?;
            Mailer::Smtp(
                builder
                    .port(port)
                    .credentials(Credentials::new(username, password))
                    .build(),
                from,
            )
        }
        _ => anyhow::bail!("invalid mail mode; memory requires FARSAIL_DEV_MEMORY_MAIL=1"),
    };
    let state = AppState::connect(&database_url, mailer).await?;
    let internal = env::var("FARSAIL_RELAY_ACCESS_TOKEN")
        .ok()
        .map(|secret| farsail_coordinator::relay_access::router(state.pool.clone(), &secret))
        .transpose()?;
    let mut app = router(state);
    if let Some(internal) = internal {
        app = app.merge(internal);
    }
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind,"coordinator listening");
    axum::serve(listener, app).await?;
    Ok(())
}
