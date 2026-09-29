#[cfg(windows)]
use farsail_client::WindowsStore;
use farsail_client::{NativeClient, SecureStore};
use farsail_coordinator::{AppState, Mailer, router};
use farsail_core::RemotePermission;
use farsail_media::{FrameMeta, JpegFrame};
use farsail_transport::{Channel, Config as TransportConfig, Frame};
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
    #[cfg(windows)]
    let store: Arc<dyn SecureStore> = Arc::new(WindowsStore::new(dir).unwrap());
    #[cfg(not(windows))]
    let store: Arc<dyn SecureStore> = Arc::new(TestStore(dir));
    let client = NativeClient::new(store).unwrap();
    // Set server through a runtime call below; this helper is synchronous to keep setup small.
    assert!(!base.is_empty());
    client
}
#[cfg(not(windows))]
struct TestStore(PathBuf);
#[cfg(not(windows))]
impl SecureStore for TestStore {
    fn read(&self, key: &str) -> farsail_client::Result<Option<Vec<u8>>> {
        match std::fs::read(self.0.join(key)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(farsail_client::Error::Store(e.to_string())),
        }
    }
    fn write(&self, key: &str, value: &[u8]) -> farsail_client::Result<()> {
        std::fs::write(self.0.join(key), value)
            .map_err(|e| farsail_client::Error::Store(e.to_string()))
    }
    fn delete(&self, key: &str) -> farsail_client::Result<()> {
        match std::fs::remove_file(self.0.join(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(farsail_client::Error::Store(e.to_string())),
        }
    }
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
        allowed_relays: std::sync::Arc::new(vec![]),
    });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a = Arc::new(client(a_dir.path().to_path_buf(), &base));
    a.set_server(&base).await.unwrap();
    let b = Arc::new(client(b_dir.path().to_path_buf(), &base));
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
    let a = Arc::new(client(a_dir.path().to_path_buf(), &base));
    assert_eq!(
        a.call("resume", Value::Null).await.unwrap()["id"],
        me_a["id"]
    );
    assert_eq!(a.public_state().await["deviceId"], dev_a);
    a.call("heartbeat", Value::Null).await.unwrap();
    a.start_transport(TransportConfig::default()).await.unwrap();
    let own = a.call("devices", Value::Null).await.unwrap();
    assert_eq!(own.as_array().unwrap().len(), 1);
    assert_eq!(own[0]["can_host"], false);
    assert_eq!(own[0]["id"], dev_a);
    b.start_transport(TransportConfig::default()).await.unwrap();
    b.set_host_capability(true).await.unwrap();
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
    // Remote watch never grants another account's invitation automatically.
    assert_eq!(b.public_state().await["remoteWatch"], false);
    b.set_remote_watch(true).unwrap();
    b.approve_same_account_pending().await.unwrap();
    assert_eq!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["state"],
        "pending"
    );
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
    // The active host capability and endpoint preserve the fresh approval.
    assert_eq!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["state"],
        "approved"
    );
    let path = a
        .connect_transport(id, RemotePermission::View)
        .await
        .unwrap();
    assert_eq!(path["state"], "direct");
    let source = a.transport_session(id).await.unwrap().unwrap();
    let mut target = None;
    for _ in 0..50 {
        target = b.transport_session(id).await.unwrap();
        if target.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let target = target.expect("target accepted authenticated iroh connection");
    source
        .send(Frame {
            channel: Channel::Media,
            bytes: b"real HTTP and QUIC".to_vec(),
        })
        .await
        .unwrap();
    assert_eq!(target.receive().await.unwrap().bytes, b"real HTTP and QUIC");
    target
        .send(Frame {
            channel: Channel::Media,
            bytes: b"ack".to_vec(),
        })
        .await
        .unwrap();
    assert_eq!(source.receive().await.unwrap().bytes, b"ack");
    // Production lease cadence (transport is a dependency, without cfg(test)).
    // Cross several 30s grant lifetimes with real HTTP token rotation and heartbeats.
    let first_expiry =
        a.call("remote_status", json!({"id":id})).await.unwrap()["grant_expires_at"].clone();
    for _ in 0..7 {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        a.call("heartbeat", Value::Null).await.unwrap();
        b.call("heartbeat", Value::Null).await.unwrap();
        assert!(
            source.is_open().await,
            "source survived lease rotation: {}",
            source.end_reason()
        );
        assert!(
            target.is_open().await,
            "target survived lease rotation: {}",
            target.end_reason()
        );
        target
            .send(Frame {
                channel: Channel::Media,
                bytes: b"lease alive".to_vec(),
            })
            .await
            .unwrap();
        assert_eq!(source.receive().await.unwrap().bytes, b"lease alive");
    }
    assert_ne!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["grant_expires_at"],
        first_expiry
    );
    // Same-account consent is checked by the host against authoritative requester identity.
    let c_dir = tempfile::tempdir().unwrap();
    let c = Arc::new(client(c_dir.path().to_path_buf(), &base));
    c.set_server(&base).await.unwrap();
    c.call(
        "login",
        json!({"email":"native-b@example.test","password":"correct horse battery staple"}),
    )
    .await
    .unwrap();
    let dev_c = c
        .call("bind", json!({"name":"Same account source"}))
        .await
        .unwrap()["id"]
        .clone();
    c.call("heartbeat", Value::Null).await.unwrap();
    let same = c
        .call(
            "request",
            json!({"source_device_id":dev_c,"target_device_id":dev_b,"permission":"control"}),
        )
        .await
        .unwrap();
    b.set_remote_watch(false).unwrap();
    b.approve_same_account_pending().await.unwrap();
    assert_eq!(
        c.call("remote_status", json!({"id":same["id"]}))
            .await
            .unwrap()["state"],
        "pending"
    );
    b.set_remote_watch(true).unwrap();
    b.approve_same_account_pending().await.unwrap();
    assert_eq!(
        c.call("remote_status", json!({"id":same["id"]}))
            .await
            .unwrap()["state"],
        "approved"
    );
    c.call("revoke_remote", json!({"id":same["id"]}))
        .await
        .unwrap();
    b.approve_same_account_pending().await.unwrap();
    assert_eq!(
        c.call("remote_status", json!({"id":same["id"]}))
            .await
            .unwrap()["state"],
        "revoked"
    );
    c.stop_transport().await;
    let synthetic = JpegFrame::encode_rgb(
        FrameMeta {
            monitor: 1,
            layout: 1,
            sequence: 1,
            captured_ms: 0,
            width: 32,
            height: 16,
            origin_x: -40,
            origin_y: 0,
        },
        &vec![80; 32 * 16 * 3],
        55,
    )
    .unwrap();
    target
        .send(Frame {
            channel: Channel::Media,
            bytes: synthetic.to_wire().unwrap(),
        })
        .await
        .unwrap();
    let decoded = JpegFrame::from_wire(&source.receive().await.unwrap().bytes).unwrap();
    assert_eq!(decoded.meta.origin_x, -40);
    assert_eq!(decoded.decode_rgb().unwrap().len(), 32 * 16 * 3);
    #[cfg(windows)]
    if std::env::var_os("FARSAIL_REAL_CAPTURE").is_some() {
        let monitor = farsail_windows::displays().unwrap().remove(0);
        let mut capture = farsail_windows::Capture::new(monitor.id).unwrap();
        let captured = loop {
            if let Some(frame) = capture.next_frame(2, 2).unwrap() {
                break frame;
            }
        };
        target
            .send(Frame {
                channel: Channel::Media,
                bytes: captured.to_wire().unwrap(),
            })
            .await
            .unwrap();
        let remote = JpegFrame::from_wire(&source.receive().await.unwrap().bytes).unwrap();
        assert_eq!(remote.meta.monitor, monitor.id);
        assert_eq!(
            remote.decode_rgb().unwrap().len(),
            (remote.meta.width * remote.meta.height * 3) as usize
        );
    }
    assert!(
        source
            .send(Frame {
                channel: Channel::Control,
                bytes: vec![1]
            })
            .await
            .is_err()
    );
    b.stop_transport().await;
    assert!(!target.is_open().await);
    b.start_transport(TransportConfig::default()).await.unwrap();
    b.set_host_capability(true).await.unwrap();
    assert!(
        a.connect_transport(id, RemotePermission::View)
            .await
            .is_err(),
        "old approval cannot reconnect after endpoint restart"
    );
    let invite2 = b
        .call(
            "invite",
            json!({"target_device_id":dev_b,"permission":"view"}),
        )
        .await
        .unwrap();
    let request2 = a.call("request",json!({"source_device_id":dev_a,"target_device_id":dev_b,"permission":"view","invitation_code":invite2["code"]})).await.unwrap();
    let id2 = request2["id"].as_str().unwrap();
    b.call("decide", json!({"id":id2,"approve":true}))
        .await
        .unwrap();
    a.connect_transport(id2, RemotePermission::View)
        .await
        .unwrap();
    let source2 = a.transport_session(id2).await.unwrap().unwrap();
    let mut target2 = None;
    for _ in 0..50 {
        target2 = b.transport_session(id2).await.unwrap();
        if target2.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let target2 = target2.expect("fresh approval accepted");
    source2
        .send(Frame {
            channel: Channel::Media,
            bytes: b"fresh approval".to_vec(),
        })
        .await
        .unwrap();
    assert_eq!(target2.receive().await.unwrap().bytes, b"fresh approval");
    a.call("revoke_remote", json!({"id":id2})).await.unwrap();
    assert!(!source2.is_open().await);
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    assert!(!target2.is_open().await);
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
