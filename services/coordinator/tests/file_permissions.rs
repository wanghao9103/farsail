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
    bearer: &str,
    proof: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if !bearer.is_empty() {
        builder = builder.header("authorization", format!("Bearer {bearer}"));
    }
    if let Some(proof) = proof {
        builder = builder.header("x-farsail-device-token", proof);
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

async fn bind(app: &Router, user: &str, platform: &str, host: bool) -> (String, String) {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let key = SigningKey::from_bytes(&seed);
    let (status, challenge) = call(
        app,
        "POST",
        "/v1/devices/challenge",
        json!({"public_key":hex::encode(key.verifying_key().to_bytes())}),
        user,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{challenge}");
    let (status, device) = call(
        app,
        "POST",
        "/v1/devices/bind",
        json!({
            "challenge_id":challenge["challenge_id"],
            "signature":hex::encode(key.sign(challenge["message"].as_str().unwrap().as_bytes()).to_bytes()),
            "name":format!("{platform} integration fixture"), "platform":platform,
            "can_host":host, "can_files":false
        }),
        user,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{device}");
    (
        device["id"].as_str().unwrap().to_owned(),
        device["device_token"].as_str().unwrap().to_owned(),
    )
}

async fn heartbeat(app: &Router, token: &str) -> i64 {
    let (status, value) = call(app, "POST", "/v1/devices/heartbeat", json!({}), token, None).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["generation"].as_i64().unwrap()
}

async fn request(
    app: &Router,
    user: &str,
    source: &str,
    target: &str,
    proof: &str,
    permission: &str,
) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/v1/remote/request",
        json!({"source_device_id":source,"target_device_id":target,"permission":permission}),
        user,
        Some(proof),
    )
    .await
}

async fn approve(app: &Router, session: &Value, target: &str) -> Value {
    let (status, grant) = call(
        app,
        "POST",
        &format!("/v1/remote/{}/decide", session["id"].as_str().unwrap()),
        json!({"approve":true}),
        target,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    grant
}

async fn inspect(app: &Router, grant: &Value, proof: &str) -> (StatusCode, Value) {
    call(
        app,
        "GET",
        &format!("/v1/remote/{}/grant", grant["session_id"].as_str().unwrap()),
        Value::Null,
        grant["grant_token"].as_str().unwrap(),
        Some(proof),
    )
    .await
}

#[tokio::test]
async fn postgres_files_are_separate_from_screen_capabilities_and_require_approval() {
    let url = std::env::var("FARSAIL_TEST_DATABASE_URL")
        .expect("set dedicated FARSAIL_TEST_DATABASE_URL");
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("fs_files_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(8)
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
    let credentials =
        json!({"email":"files@example.test","password":"correct horse battery staple"});
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/auth/register",
            credentials.clone(),
            "",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let verification = mailer.messages().last().unwrap().token.clone();
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/auth/verify",
            json!({"email":"files@example.test","token":verification}),
            "",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, login) = call(&app, "POST", "/v1/auth/login", credentials, "", None).await;
    assert_eq!(status, StatusCode::OK, "{login}");
    let user = login["access_token"].as_str().unwrap();
    let (windows, windows_token) = bind(&app, user, "windows", true).await;
    let (linux, linux_token) = bind(&app, user, "linux", false).await;
    let windows_generation = heartbeat(&app, &windows_token).await;
    let linux_generation = heartbeat(&app, &linux_token).await;

    // An already-bound Ubuntu device can enable Files without rebinding or
    // acquiring any screen/control authority. Stale endpoints cannot enable it.
    assert_eq!(
        request(&app, user, &windows, &linux, &windows_token, "files")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for (input, expected) in [
        (
            json!({"generation":linux_generation}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"generation":linux_generation-1,"can_files":true}),
            StatusCode::CONFLICT,
        ),
        (
            json!({"generation":linux_generation,"can_host":true,"can_files":true}),
            StatusCode::CONFLICT,
        ),
    ] {
        assert_eq!(
            call(
                &app,
                "POST",
                "/v1/devices/capability",
                input,
                &linux_token,
                None
            )
            .await
            .0,
            expected
        );
    }
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":linux_generation,"can_files":true}),
        &linux_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(capability["can_host"], false);
    assert_eq!(capability["can_files"], true);
    for permission in ["view", "control"] {
        assert_eq!(
            request(&app, user, &windows, &linux, &windows_token, permission)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                &app,
                "POST",
                "/v1/invitations",
                json!({"target_device_id":linux,"permission":permission}),
                user,
                None
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/invitations",
            json!({"target_device_id":linux,"permission":"files"}),
            user,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, session) = request(&app, user, &windows, &linux, &windows_token, "files").await;
    assert_eq!(status, StatusCode::OK, "{session}");
    assert_eq!(session["state"], "pending");
    for (path, bearer) in [
        ("/v1/remote/pending", linux_token.as_str()),
        ("/v1/remote/sessions", user),
    ] {
        let (status, listed) = call(&app, "GET", path, Value::Null, bearer, None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        let item = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == session["id"])
            .unwrap();
        assert_eq!(item["permission"], "files");
    }
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/v1/remote/{}/decide", session["id"].as_str().unwrap()),
            json!({"approve":true}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let linux_grant = approve(&app, &session, &linux_token).await;
    assert_eq!(linux_grant["permission"], "files");
    let (status, verified) = inspect(&app, &linux_grant, &windows_token).await;
    assert_eq!(status, StatusCode::OK, "{verified}");
    assert_eq!(verified["permission"], "files");

    // The two sharing switches preserve each other, and switching screen
    // sharing off must leave an independently approved file session usable.
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":windows_generation,"can_files":true}),
        &windows_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(capability["can_host"], true);
    let (status, view) = request(&app, user, &linux, &windows, &linux_token, "view").await;
    assert_eq!(status, StatusCode::OK, "{view}");
    let view_grant = approve(&app, &view, &windows_token).await;
    let (status, files) = request(&app, user, &linux, &windows, &linux_token, "files").await;
    assert_eq!(status, StatusCode::OK, "{files}");
    let windows_grant = approve(&app, &files, &windows_token).await;
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":windows_generation,"can_host":false}),
        &windows_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(capability["can_files"], true);
    assert_eq!(
        inspect(&app, &view_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::OK
    );
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":windows_generation,"can_host":true}),
        &windows_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(capability["can_files"], true);
    let (status, view) = request(&app, user, &linux, &windows, &linux_token, "view").await;
    assert_eq!(status, StatusCode::OK, "{view}");
    let view_grant = approve(&app, &view, &windows_token).await;
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":windows_generation,"can_files":false}),
        &windows_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(capability["can_host"], true);
    assert_eq!(
        inspect(&app, &view_grant, &linux_token).await.0,
        StatusCode::OK
    );
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, _) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":windows_generation,"can_files":true}),
        &windows_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, files) = request(&app, user, &linux, &windows, &linux_token, "files").await;
    assert_eq!(status, StatusCode::OK, "{files}");
    let windows_grant = approve(&app, &files, &windows_token).await;

    // Turning Files off revokes both existing grants and pending approvals.
    let (status, pending) = request(&app, user, &windows, &linux, &windows_token, "files").await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let (status, capability) = call(
        &app,
        "POST",
        "/v1/devices/capability",
        json!({"generation":linux_generation,"can_files":false}),
        &linux_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{capability}");
    assert_eq!(
        inspect(&app, &linux_grant, &windows_token).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/v1/remote/{}/decide", pending["id"].as_str().unwrap()),
            json!({"approve":true}),
            &linux_token,
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&app, user, &windows, &linux, &windows_token, "files")
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Expired leases, disabled devices and logout retain the old fail-closed
    // behavior for file capability updates and otherwise valid file grants.
    let windows_id = Uuid::parse_str(&windows).unwrap();
    // Simulate capability loss between approval and issuance/renewal. The
    // grant checks must enforce the current capability independently of the
    // revocation normally performed by the capability transaction.
    sqlx::query("UPDATE devices SET can_files=false WHERE id=$1")
        .bind(windows_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        inspect(&app, &view_grant, &linux_token).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!(
                "/v1/remote/{}/renew",
                windows_grant["session_id"].as_str().unwrap()
            ),
            json!({}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE devices SET can_files=true WHERE id=$1")
        .bind(windows_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(windows_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/devices/capability",
            json!({"generation":windows_generation,"can_files":false}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/devices/heartbeat",
            json!({"generation":windows_generation}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE devices SET enabled=false WHERE id=$1")
        .bind(windows_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/devices/capability",
            json!({"generation":windows_generation,"can_files":true}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE devices SET enabled=true WHERE id=$1")
        .bind(windows_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "POST", "/v1/auth/logout", json!({}), user, None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/v1/devices/capability",
            json!({"generation":windows_generation,"can_files":true}),
            &windows_token,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        inspect(&app, &windows_grant, &linux_token).await.0,
        StatusCode::FORBIDDEN
    );

    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
