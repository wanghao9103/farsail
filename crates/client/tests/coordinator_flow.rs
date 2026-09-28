use farsail_client::{NativeClient, WindowsStore};
use farsail_coordinator::{AppState, Mailer, router};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

async fn account(client: &NativeClient, mail: &Mailer, email: &str) -> Value {
    client
        .call(
            "register",
            json!({"email":email,"password":"correct horse battery staple"}),
        )
        .await
        .unwrap();
    let token = mail.messages().last().unwrap().token.clone();
    client.call("verify", json!({"token":token})).await.unwrap();
    client
        .call(
            "login",
            json!({"email":email,"password":"correct horse battery staple"}),
        )
        .await
        .unwrap()
}
fn client(dir: PathBuf, base: &str) -> NativeClient {
    let client = NativeClient::new(Arc::new(WindowsStore::new(dir).unwrap())).unwrap();
    // Set server through a runtime call below; this helper is synchronous to keep setup small.
    assert!(!base.is_empty());
    client
}
#[tokio::test]
async fn native_client_against_real_coordinator() {
    let Ok(url) = std::env::var("FARSAIL_TEST_DATABASE_URL") else {
        eprintln!("skipping integration: FARSAIL_TEST_DATABASE_URL not set");
        return;
    };
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("fs_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(12)
        .after_connect(move |conn, _| {
            let sql = format!("SET search_path TO {search_path}");
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    let migrations =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../services/coordinator/migrations");
    sqlx::migrate::Migrator::new(migrations.as_path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    let mail = Mailer::memory();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = router(AppState {
        pool: pool.clone(),
        mailer: mail.clone(),
    });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a = client(a_dir.path().to_path_buf(), &base);
    a.set_server(&base).await.unwrap();
    let b = client(b_dir.path().to_path_buf(), &base);
    b.set_server(&base).await.unwrap();
    let me_a = account(&a, &mail, "native-a@example.test").await;
    let me_b = account(&b, &mail, "native-b@example.test").await;
    assert!(
        a.call(
            "login",
            json!({"email":"native-b@example.test","password":"correct horse battery staple"})
        )
        .await
        .is_err()
    );
    assert_eq!(a.call("me", Value::Null).await.unwrap()["id"], me_a["id"]);
    let dev_a = a
        .call("bind", json!({"name":"Source Windows"}))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let dev_b = b
        .call("bind", json!({"name":"Target Windows"}))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    a.call("heartbeat", Value::Null).await.unwrap();
    b.call("heartbeat", Value::Null).await.unwrap();
    drop(a);
    let a = client(a_dir.path().to_path_buf(), &base);
    assert_eq!(
        a.call("resume", Value::Null).await.unwrap()["id"],
        me_a["id"]
    );
    assert_eq!(a.public_state().await["deviceId"], dev_a);
    a.call("heartbeat", Value::Null).await.unwrap();
    let own = a.call("devices", Value::Null).await.unwrap();
    assert_eq!(own.as_array().unwrap().len(), 1);
    assert_eq!(own[0]["can_host"], false);
    assert_eq!(own[0]["id"], dev_a);
    // Simulate a later WI-004 host capability in the isolated schema so the current UI authorization path is exercised.
    sqlx::query("UPDATE devices SET can_host=true WHERE id=$1")
        .bind(Uuid::parse_str(&dev_b).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let invite = b
        .call(
            "invite",
            json!({"target_device_id":dev_b,"permission":"view"}),
        )
        .await
        .unwrap();
    let request = a.call("request",json!({"source_device_id":dev_a,"target_device_id":dev_b,"permission":"view","invitation_code":invite["code"]})).await.unwrap();
    assert_eq!(request["state"], "pending");
    let id = request["id"].as_str().unwrap();
    assert_eq!(
        b.call("pending", Value::Null)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let decision = b
        .call("decide", json!({"id":id,"approve":true}))
        .await
        .unwrap();
    assert_eq!(decision["approved"], true);
    assert!(decision.get("grant_token").is_none());
    assert_eq!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["state"],
        "approved"
    );
    a.call("revoke_remote", json!({"id":id})).await.unwrap();
    assert_eq!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["state"],
        "revoked"
    );
    let b_id = Uuid::parse_str(me_b["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(b_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(b.call("me", Value::Null).await.unwrap()["role"], "admin");
    assert_eq!(
        b.call("admin_registration", Value::Null).await.unwrap()["value"],
        "open"
    );
    b.call("admin_set_registration", json!({"value":"invite_only"}))
        .await
        .unwrap();
    assert_eq!(
        b.call("admin_users", Value::Null)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        !b.call("admin_audit", Value::Null)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    a.call("logout", Value::Null).await.unwrap();
    assert!(a.call("me", Value::Null).await.is_err());
    assert!(a.call("heartbeat", Value::Null).await.is_err());
    server.abort();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
