use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use ed25519_dalek::{Signer, SigningKey};
use farsail_coordinator::{AppState, Mailer, router};
use http_body_util::BodyExt;
use rand::RngCore;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    user: Option<&str>,
    device: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = user {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    if let Some(t) = device {
        builder = builder.header("x-farsail-device-token", t);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}
async fn post(
    app: &Router,
    path: &str,
    body: Value,
    user: Option<&str>,
    device: Option<&str>,
) -> (StatusCode, Value) {
    call(app, "POST", path, body, user, device).await
}
async fn transport_call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    bearer: &str,
    device: Option<&str>,
    generation: i64,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {bearer}"))
        .header("x-farsail-generation", generation.to_string());
    if let Some(t) = device {
        builder = builder.header("x-farsail-device-token", t);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn register(app: &Router, mailer: &Mailer, email: &str) -> (Uuid, String) {
    let (s, v) = post(
        app,
        "/v1/auth/register",
        json!({"email":email,"password":"correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let id = Uuid::parse_str(v["id"].as_str().unwrap()).unwrap();
    let token = mailer.messages().last().unwrap().token.clone();
    let (s, _) = post(app, "/v1/auth/verify", json!({"token":token}), None, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = post(
        app,
        "/v1/auth/login",
        json!({"email":email,"password":"correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    (id, v["access_token"].as_str().unwrap().to_owned())
}
fn signing_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    SigningKey::from_bytes(&bytes)
}
async fn bind(
    app: &Router,
    user: &str,
    key: &SigningKey,
    name: &str,
    host: bool,
) -> (Uuid, String) {
    let pubkey = hex::encode(key.verifying_key().to_bytes());
    let (s, c) = post(
        app,
        "/v1/devices/challenge",
        json!({"public_key":pubkey}),
        Some(user),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let signature = hex::encode(
        key.sign(c["message"].as_str().unwrap().as_bytes())
            .to_bytes(),
    );
    let input = json!({"challenge_id":c["challenge_id"],"signature":signature,"name":name,"platform":"windows","can_host":host,"can_files":true});
    let (s, b) = post(app, "/v1/devices/bind", input.clone(), Some(user), None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (replay, _) = post(app, "/v1/devices/bind", input, Some(user), None).await;
    assert_eq!(replay, StatusCode::BAD_REQUEST);
    (
        Uuid::parse_str(b["id"].as_str().unwrap()).unwrap(),
        b["device_token"].as_str().unwrap().to_owned(),
    )
}
async fn online(app: &Router, token: &str) -> i64 {
    let (s, v) = post(app, "/v1/devices/heartbeat", json!({}), Some(token), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["generation"].as_i64().unwrap()
}

#[tokio::test]
async fn postgres_identity_device_and_grant_lifecycle() {
    let url = std::env::var("FARSAIL_TEST_DATABASE_URL")
        .expect("set dedicated FARSAIL_TEST_DATABASE_URL");
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
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let mailer = Mailer::memory();
    let app = router(AppState {
        pool: pool.clone(),
        mailer: mailer.clone(),
        allowed_relays: std::sync::Arc::new(vec![]),
    });

    let (_alice_id, alice) = register(&app, &mailer, "alice@example.test").await;
    let (bob_id, bob) = register(&app, &mailer, "bob@example.test").await;
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(bob_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_charlie_id, charlie) = register(&app, &mailer, "charlie@example.test").await;
    let (s, me) = call(&app, "GET", "/v1/me", Value::Null, Some(&alice), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["email"], "alice@example.test");
    let alice_key = signing_key();
    let bob_key = signing_key();
    let (alice_device, alice_device_token) = bind(&app, &alice, &alice_key, "Alice", false).await;
    let (bob_device, bob_device_token) = bind(&app, &bob, &bob_key, "Bob", true).await;

    // Internal admission is independent of application grants and fails closed.
    let secret = "disposable-integration-relay-secret-32bytes";
    let admission = farsail_coordinator::relay_access::router(pool.clone(), secret).unwrap();
    async fn admit(app: &Router, secret: Option<&str>, key: &str) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri("/internal/relay-access")
            .header("x-iroh-nodeid", key);
        if let Some(secret) = secret {
            request = request.header("authorization", format!("Bearer {secret}"));
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    let public_key = hex::encode(alice_key.verifying_key().to_bytes());
    assert_eq!(
        admit(&admission, None, &public_key).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        admit(&admission, Some("wrong"), &public_key).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        admit(&admission, Some(secret), "invalid").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        admit(&admission, Some(secret), &"00".repeat(32)).await,
        (StatusCode::OK, json!(false))
    );
    assert_eq!(
        admit(&admission, Some(secret), &public_key).await,
        (StatusCode::OK, json!(true))
    );
    for (table, field, id, bad, good) in [
        ("devices", "enabled", alice_device, "false", "true"),
        ("devices", "bound", alice_device, "false", "true"),
        ("users", "enabled", _alice_id, "false", "true"),
        ("users", "verified", _alice_id, "false", "true"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {field}={bad} WHERE id=$1"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            admit(&admission, Some(secret), &public_key).await.1,
            json!(false)
        );
        sqlx::query(&format!("UPDATE {table} SET {field}={good} WHERE id=$1"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE auth_sessions SET revoked_at=now() WHERE id=(SELECT credential_session_id FROM devices WHERE id=$1)")
        .bind(alice_device).execute(&pool).await.unwrap();
    assert_eq!(
        admit(&admission, Some(secret), &public_key).await.1,
        json!(false)
    );
    sqlx::query("UPDATE auth_sessions SET revoked_at=NULL WHERE id=(SELECT credential_session_id FROM devices WHERE id=$1)")
        .bind(alice_device).execute(&pool).await.unwrap();

    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/devices/{bob_device}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, v) = call(
        &app,
        "GET",
        "/v1/devices?limit=100",
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 1);
    let (s, _) = call(
        &app,
        "GET",
        "/v1/devices",
        Value::Null,
        Some(&alice_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(&app, "/v1/devices/heartbeat", json!({}), Some(&alice), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let first = online(&app, &alice_device_token).await;
    let second = online(&app, &alice_device_token).await;
    assert!(second > first);
    let (s, _) = post(
        &app,
        "/v1/devices/heartbeat",
        json!({"generation":first}),
        Some(&alice_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let bob_generation = online(&app, &bob_device_token).await;

    // Two valid challenges for one key race through the PostgreSQL unique constraint.
    let racing = signing_key();
    let key_hex = hex::encode(racing.verifying_key().to_bytes());
    let (_, c1) = post(
        &app,
        "/v1/devices/challenge",
        json!({"public_key":key_hex}),
        Some(&alice),
        None,
    )
    .await;
    let (_, c2) = post(
        &app,
        "/v1/devices/challenge",
        json!({"public_key":key_hex}),
        Some(&alice),
        None,
    )
    .await;
    let b1 = json!({"challenge_id":c1["challenge_id"],"signature":hex::encode(racing.sign(c1["message"].as_str().unwrap().as_bytes()).to_bytes()),"name":"race","platform":"windows","can_host":false,"can_files":false});
    let b2 = json!({"challenge_id":c2["challenge_id"],"signature":hex::encode(racing.sign(c2["message"].as_str().unwrap().as_bytes()).to_bytes()),"name":"race","platform":"windows","can_host":false,"can_files":false});
    let (r1, r2) = tokio::join!(
        post(&app, "/v1/devices/bind", b1, Some(&alice), None),
        post(&app, "/v1/devices/bind", b2, Some(&alice), None)
    );
    assert_eq!(r1.0, StatusCode::OK, "{:?}", r1.1);
    assert_eq!(r2.0, StatusCode::OK, "{:?}", r2.1);
    assert_eq!(r1.1["id"], r2.1["id"]);
    let count: (i64,) =
        sqlx::query_as("SELECT count(*) FROM devices WHERE public_key=$1 AND bound")
            .bind(racing.verifying_key().to_bytes().as_slice())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);
    let (s, foreign_challenge) = post(
        &app,
        "/v1/devices/challenge",
        json!({"public_key":hex::encode(bob_key.verifying_key().to_bytes())}),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s,_) = post(&app,"/v1/devices/bind",json!({"challenge_id":foreign_challenge["challenge_id"],"signature":hex::encode(bob_key.sign(foreign_challenge["message"].as_str().unwrap().as_bytes()).to_bytes()),"name":"stolen","platform":"windows","can_host":true,"can_files":true}),Some(&alice),None).await;
    assert_eq!(s, StatusCode::CONFLICT); // Even proof cannot silently overwrite an existing owner.

    let (_, invite) = post(
        &app,
        "/v1/invitations",
        json!({"target_device_id":bob_device,"permission":"view"}),
        Some(&bob),
        None,
    )
    .await;
    let (s,request)=post(&app,"/v1/remote/request",json!({"source_device_id":alice_device,"target_device_id":bob_device,"permission":"view","invitation_code":invite["code"]}),Some(&alice),Some(&alice_device_token)).await;
    assert_eq!(s, StatusCode::OK, "{request}");
    let remote_id = request["id"].as_str().unwrap();
    let (s, v) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["state"], "pending");
    let (_, own_sessions) = call(
        &app,
        "GET",
        "/v1/remote/sessions",
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert!(
        own_sessions
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == remote_id)
    );
    let (_, foreign_sessions) = call(
        &app,
        "GET",
        "/v1/remote/sessions",
        Value::Null,
        Some(&charlie),
        None,
    )
    .await;
    assert!(foreign_sessions.as_array().unwrap().is_empty());
    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}"),
        Value::Null,
        Some(&charlie),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s,_) = post(&app,"/v1/remote/request",json!({"source_device_id":alice_device,"target_device_id":bob_device,"permission":"view","invitation_code":invite["code"]}),Some(&alice),Some(&alice_device_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN); // One-use invitation.
    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/grant"),
        Value::Null,
        Some("bogus"),
        Some(&alice_device_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, audit) = call(
        &app,
        "GET",
        "/v1/admin/audit",
        Value::Null,
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(!audit.as_array().unwrap().is_empty());
    let (s, _) = call(
        &app,
        "GET",
        "/v1/admin/audit",
        Value::Null,
        Some(&charlie),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = post(
        &app,
        &format!("/v1/remote/{remote_id}/decide"),
        json!({"approve":true}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED); // User or admin tokens cannot approve as a device.
    let (s, grant) = post(
        &app,
        &format!("/v1/remote/{remote_id}/decide"),
        json!({"approve":true}),
        Some(&bob_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{grant}");
    assert_eq!(grant["permission"], "view");
    let (s, v) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["state"], "approved");
    assert!(v.get("grant_token").is_none());
    let grant_token = grant["grant_token"].as_str().unwrap();
    let (s, g) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/grant"),
        Value::Null,
        Some(grant_token),
        Some(&alice_device_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["permission"], "view");
    let alice_addr =
        iroh::EndpointAddr::new(iroh::SecretKey::from_bytes(&alice_key.to_bytes()).public())
            .with_ip_addr("127.0.0.1:43001".parse().unwrap());
    let bob_addr =
        iroh::EndpointAddr::new(iroh::SecretKey::from_bytes(&bob_key.to_bytes()).public())
            .with_ip_addr("127.0.0.1:43002".parse().unwrap());
    let (s, _) = transport_call(
        &app,
        "POST",
        "/v1/devices/endpoint-address",
        json!({"generation":first,"endpoint_addr":alice_addr}),
        &alice_device_token,
        None,
        first,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT); // Stale process cannot publish.
    for (token, generation, addr) in [
        (&alice_device_token, second, &alice_addr),
        (&bob_device_token, bob_generation, &bob_addr),
    ] {
        let (s, v) = transport_call(
            &app,
            "POST",
            "/v1/devices/endpoint-address",
            json!({"generation":generation,"endpoint_addr":addr}),
            token,
            None,
            generation,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    // A delayed old publication must not overwrite a newer generation at conflict update.
    let stale = sqlx::query("INSERT INTO device_endpoint_addresses(device_id,generation,endpoint_addr) VALUES($1,$2,$3) ON CONFLICT(device_id) DO UPDATE SET generation=EXCLUDED.generation,endpoint_addr=EXCLUDED.endpoint_addr WHERE device_endpoint_addresses.generation<=EXCLUDED.generation")
        .bind(alice_device).bind(first).bind(serde_json::to_string(&alice_addr).unwrap()).execute(&pool).await.unwrap();
    assert_eq!(stale.rows_affected(), 0);
    let (stored_generation,): (i64,) =
        sqlx::query_as("SELECT generation FROM device_endpoint_addresses WHERE device_id=$1")
            .bind(alice_device)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_generation, second);
    let (s, v) = transport_call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/peer-address"),
        Value::Null,
        &alice_device_token,
        None,
        second,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["endpoint_addr"], serde_json::to_value(&bob_addr).unwrap());
    let (s, _) = transport_call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/peer-address"),
        Value::Null,
        &alice_device_token,
        None,
        first,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, g) = transport_call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/transport-grant"),
        Value::Null,
        grant_token,
        Some(&alice_device_token),
        second,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert!(g["expires_in"].as_i64().unwrap() > 0);
    let (s, _) = transport_call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/transport-grant"),
        Value::Null,
        grant_token,
        Some(&alice_device_token),
        first,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let malicious =
        iroh::EndpointAddr::new(iroh::SecretKey::from_bytes(&bob_key.to_bytes()).public())
            .with_relay_url("https://user:secret@evil.example/?token=x".parse().unwrap());
    let (s, _) = transport_call(
        &app,
        "POST",
        "/v1/devices/endpoint-address",
        json!({"generation":bob_generation,"endpoint_addr":malicious}),
        &bob_device_token,
        None,
        bob_generation,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/grant"),
        Value::Null,
        Some(grant_token),
        r2.1["device_token"].as_str(),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN); // Third device cannot use the grant.
    let (s, _) = post(
        &app,
        &format!("/v1/remote/{remote_id}/revoke"),
        json!({}),
        Some(&bob_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(v["state"], "revoked");
    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/remote/{remote_id}/grant"),
        Value::Null,
        Some(grant_token),
        Some(&alice_device_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (_, invite2) = post(
        &app,
        "/v1/invitations",
        json!({"target_device_id":bob_device,"permission":"view"}),
        Some(&bob),
        None,
    )
    .await;
    let (_,request2)=post(&app,"/v1/remote/request",json!({"source_device_id":alice_device,"target_device_id":bob_device,"permission":"view","invitation_code":invite2["code"]}),Some(&alice),Some(&alice_device_token)).await;
    let id2 = request2["id"].as_str().unwrap();
    let (_, grant2) = post(
        &app,
        &format!("/v1/remote/{id2}/decide"),
        json!({"approve":true}),
        Some(&bob_device_token),
        None,
    )
    .await;
    sqlx::query("UPDATE remote_sessions SET grant_until=now()-interval '1 second' WHERE id=$1")
        .bind(Uuid::parse_str(id2).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (s, _) = call(
        &app,
        "GET",
        &format!("/v1/remote/{id2}/grant"),
        Value::Null,
        Some(grant2["grant_token"].as_str().unwrap()),
        Some(&alice_device_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = post(
        &app,
        &format!("/v1/remote/{id2}/renew"),
        json!({}),
        Some(&bob_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, v) = call(
        &app,
        "GET",
        &format!("/v1/remote/{id2}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(v["state"], "expired");

    let (_, invite3) = post(
        &app,
        "/v1/invitations",
        json!({"target_device_id":bob_device,"permission":"control"}),
        Some(&bob),
        None,
    )
    .await;
    let (s, request3) = post(&app,"/v1/remote/request",json!({"source_device_id":alice_device,"target_device_id":bob_device,"permission":"control","invitation_code":invite3["code"]}),Some(&alice),Some(&alice_device_token)).await;
    assert_eq!(s, StatusCode::OK);
    let id3 = request3["id"].as_str().unwrap();
    let (s, _) = post(
        &app,
        &format!("/v1/remote/{id3}/decide"),
        json!({"approve":false}),
        Some(&bob_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = call(
        &app,
        "GET",
        &format!("/v1/remote/{id3}"),
        Value::Null,
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(v["state"], "denied");

    let (_, login) = post(
        &app,
        "/v1/auth/login",
        json!({"email":"alice@example.test","password":"correct horse battery staple"}),
        None,
        None,
    )
    .await;
    let old_refresh = login["refresh_token"].as_str().unwrap();
    let (s, rotated) = post(
        &app,
        "/v1/auth/refresh",
        json!({"refresh_token":old_refresh}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{rotated}");
    let (s, _) = post(
        &app,
        "/v1/auth/refresh",
        json!({"refresh_token":old_refresh}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(
        &app,
        "/v1/auth/refresh",
        json!({"refresh_token":rotated["refresh_token"]}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Admin powers do not grant a remote approval; device disable survives rebind attempts.
    let (s, _) = call(
        &app,
        "PUT",
        "/v1/admin/registration",
        json!({"value":"invite_only"}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = post(
        &app,
        "/v1/auth/register",
        json!({"email":"invited@example.test","password":"correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, signup) = post(
        &app,
        "/v1/admin/signup-invitations",
        json!({"email":"invited@example.test"}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s,_) = post(&app,"/v1/auth/register",json!({"email":"invited@example.test","password":"correct horse battery staple","invite_code":signup["code"]}),None,None).await;
    assert_eq!(s, StatusCode::OK);
    let (s,_) = post(&app,"/v1/auth/register",json!({"email":"invited@example.test","password":"correct horse battery staple","invite_code":signup["code"]}),None,None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = post(
        &app,
        &format!("/v1/admin/users/{bob_id}/enabled"),
        json!({"enabled":false}),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let racing_id = Uuid::parse_str(r1.1["id"].as_str().unwrap()).unwrap();
    let (s, _) = post(
        &app,
        &format!("/v1/admin/devices/{racing_id}/revoke"),
        json!({}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, v) = call(&app, "GET", "/v1/devices", Value::Null, Some(&alice), None).await;
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == r1.1["id"] && d["enabled"] == false)
    );
    let (_, c) = post(
        &app,
        "/v1/devices/challenge",
        json!({"public_key":key_hex}),
        Some(&alice),
        None,
    )
    .await;
    let (s,_) = post(&app,"/v1/devices/bind",json!({"challenge_id":c["challenge_id"],"signature":hex::encode(racing.sign(c["message"].as_str().unwrap().as_bytes()).to_bytes()),"name":"retry","platform":"windows","can_host":false,"can_files":false}),Some(&alice),None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = post(
        &app,
        &format!("/v1/admin/devices/{racing_id}/enabled"),
        json!({"enabled":true}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_id, _new_device_token) = bind(&app, &alice, &racing, "reapproved", false).await;

    let (s, _) = post(
        &app,
        "/v1/auth/recovery/request",
        json!({"email":"alice@example.test"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let reset1 = mailer.messages().last().unwrap().token.clone();
    let (s, _) = post(
        &app,
        "/v1/auth/recovery/request",
        json!({"email":"alice@example.test"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let reset2 = mailer.messages().last().unwrap().token.clone();
    let (s, _) = post(
        &app,
        "/v1/auth/recovery/complete",
        json!({"token":reset1,"password":"new correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = post(
        &app,
        "/v1/auth/recovery/complete",
        json!({"token":reset2,"password":"third correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "GET", "/v1/me", Value::Null, Some(&alice), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(
        &app,
        "/v1/devices/heartbeat",
        json!({}),
        Some(&alice_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED); // Recovery revoked credential's issuing login.
    let (s, v) = post(
        &app,
        "/v1/auth/login",
        json!({"email":"alice@example.test","password":"new correct horse battery staple"}),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let new_alice = v["access_token"].as_str().unwrap();
    let alice_id = Uuid::parse_str(me["id"].as_str().unwrap()).unwrap();
    let (s, _) = post(
        &app,
        &format!("/v1/admin/users/{alice_id}/enabled"),
        json!({"enabled":false}),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "GET", "/v1/me", Value::Null, Some(new_alice), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(
        &app,
        "/v1/devices/heartbeat",
        json!({}),
        Some(&alice_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = post(&app, "/v1/auth/logout", json!({}), Some(&bob), None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = post(
        &app,
        "/v1/devices/heartbeat",
        json!({}),
        Some(&bob_device_token),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
