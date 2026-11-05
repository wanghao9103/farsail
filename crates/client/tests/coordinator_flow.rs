#[cfg(windows)]
use farsail_client::WindowsStore;
use farsail_client::{NativeClient, SecureStore};
use farsail_coordinator::{AppState, Mailer, router};
use farsail_core::RemotePermission;
use farsail_media::{FrameMeta, JpegFrame};
use farsail_transport::{Channel, Config as TransportConfig, Frame, Session};
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
    client
        .call("verify", json!({"email":email,"token":token}))
        .await
        .unwrap();
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
async fn approved_view_pair(
    source: &Arc<NativeClient>,
    target: &Arc<NativeClient>,
    source_device: &str,
    target_device: &str,
) -> (Session, Session) {
    approved_screen_pair(
        source,
        target,
        source_device,
        target_device,
        RemotePermission::View,
    )
    .await
}
async fn approved_screen_pair(
    source: &Arc<NativeClient>,
    target: &Arc<NativeClient>,
    source_device: &str,
    target_device: &str,
    permission: RemotePermission,
) -> (Session, Session) {
    let invite = target
        .call(
            "invite",
            json!({"target_device_id":target_device,"permission":permission}),
        )
        .await
        .unwrap();
    let request = source.call("request", json!({"source_device_id":source_device,"target_device_id":target_device,"permission":permission,"invitation_code":invite["code"]})).await.unwrap();
    let id = request["id"].as_str().unwrap();
    target
        .call("decide", json!({"id":id,"approve":true}))
        .await
        .unwrap();
    source.connect_transport(id, permission).await.unwrap();
    let viewer = source.transport_session(id).await.unwrap().unwrap();
    let host = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(host) = target.transport_session(id).await.unwrap() {
                break host;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("second host accepted its independent connection");
    (viewer, host)
}
async fn exchange_media(viewer: &Session, host: &Session, payload: &[u8]) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        host.send(Frame {
            channel: Channel::Media,
            bytes: payload.to_vec(),
        })
        .await
        .unwrap();
        assert_eq!(viewer.receive().await.unwrap().bytes, payload);
        viewer
            .send(Frame {
                channel: Channel::Media,
                bytes: b"independent ACK".to_vec(),
            })
            .await
            .unwrap();
        assert_eq!(host.receive().await.unwrap().bytes, b"independent ACK");
    })
    .await
    .expect("each live peer keeps exchanging media and ACKs");
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
    // Binding publishes the online lease before returning, including controller-only Linux.
    let initial = a.call("devices", Value::Null).await.unwrap();
    assert_eq!(initial[0]["online"], true);
    #[cfg(not(windows))]
    {
        // The target simulates a Windows host; the source retains its real Linux platform.
        sqlx::query("UPDATE devices SET platform='windows' WHERE id=$1")
            .bind(Uuid::parse_str(&dev_b).unwrap())
            .execute(&pool)
            .await
            .unwrap();
    }
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
    assert_eq!(own[0]["platform"], std::env::consts::OS);
    #[cfg(target_os = "linux")]
    {
        assert!(a.set_host_capability(true).await.is_err());
        a.set_host_capability(false).await.unwrap();
    }
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
    // One desktop controls two independent clients against the real HTTP
    // coordinator and QUIC endpoints, including grant rotation and peer restart.
    let second_dir = tempfile::tempdir().unwrap();
    let second = Arc::new(client(second_dir.path().to_path_buf(), &base));
    second.set_server(&base).await.unwrap();
    account(&second, &mail, "native-second-host@example.test").await;
    let second_device = second
        .call("bind", json!({"name":"Second independent host"}))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    #[cfg(not(windows))]
    sqlx::query("UPDATE devices SET platform='windows' WHERE id=$1")
        .bind(Uuid::parse_str(&second_device).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    second
        .start_transport(TransportConfig::default())
        .await
        .unwrap();
    second.set_host_capability(true).await.unwrap();
    let (mut second_source, mut second_target) =
        approved_view_pair(&a, &second, &dev_a, &second_device).await;
    let first_connection = source.stable_id();
    assert_ne!(first_connection, second_source.stable_id());
    // Production lease cadence (transport is a dependency, without cfg(test)).
    // Cross several 30s grant lifetimes with real HTTP token rotation and heartbeats.
    let first_expiry =
        a.call("remote_status", json!({"id":id})).await.unwrap()["grant_expires_at"].clone();
    for iteration in 0..7 {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        a.call("heartbeat", Value::Null).await.unwrap();
        b.call("heartbeat", Value::Null).await.unwrap();
        second.call("heartbeat", Value::Null).await.unwrap();
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
        tokio::join!(
            exchange_media(&source, &target, b"first host lease alive"),
            exchange_media(&second_source, &second_target, b"second host lease alive")
        );
        if iteration == 2 {
            let old_connection = second_source.stable_id();
            second.stop_transport().await;
            assert!(!second_target.is_open().await);
            let (reconnected, ()) = tokio::join!(
                async {
                    second
                        .start_transport(TransportConfig::default())
                        .await
                        .unwrap();
                    second.set_host_capability(true).await.unwrap();
                    approved_view_pair(&a, &second, &dev_a, &second_device).await
                },
                async {
                    for _ in 0..6 {
                        exchange_media(&source, &target, b"first host survives other peer restart")
                            .await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            );
            (second_source, second_target) = reconnected;
            assert_ne!(second_source.stable_id(), old_connection);
        }
        assert_eq!(source.stable_id(), first_connection);
        assert!(second_source.is_open().await && second_target.is_open().await);
    }
    assert_ne!(
        a.call("remote_status", json!({"id":id})).await.unwrap()["grant_expires_at"],
        first_expiry
    );
    second.stop_transport().await;
    exchange_media(
        &source,
        &target,
        b"first host survives second host shutdown",
    )
    .await;
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
        3 // Controller, original host, and the independent second host.
    );
    assert!(
        !b.call("admin_audit", Value::Null)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    #[cfg(not(windows))]
    for entry in std::fs::read_dir(a_dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("binding-")
        {
            std::fs::remove_file(path).unwrap(); // Simulate an older client's token-only profile.
        }
    }
    a.call("logout", Value::Null).await.unwrap();
    assert!(a.call("me", Value::Null).await.is_err());
    assert!(a.call("heartbeat", Value::Null).await.is_err());
    // A fresh login renews the device credential without asking to add this computer again.
    let login = json!({"email":"native-a@example.test","password":"correct horse battery staple"});
    drop(a);
    let a = client(a_dir.path().to_path_buf(), &base);
    a.call("login", login.clone()).await.unwrap();
    assert_eq!(a.public_state().await["deviceId"], dev_a);
    assert!(a.public_state().await["deviceRestoreError"].is_null());
    let devices = a.call("devices", Value::Null).await.unwrap();
    let restored = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == dev_a)
        .unwrap();
    assert_eq!(restored["online"], true);
    assert_eq!(restored["name"], "Source Windows");
    assert_eq!(devices.as_array().unwrap().len(), 1);
    assert_eq!(a.public_state().await["sharing"], false);
    a.call("logout", Value::Null).await.unwrap();
    // The remembered binding is scoped to both coordinator and account.
    a.call(
        "login",
        json!({"email":"native-b@example.test","password":"correct horse battery staple"}),
    )
    .await
    .unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    a.call("logout", Value::Null).await.unwrap();
    a.set_server(&base.replace("127.0.0.1", "localhost"))
        .await
        .unwrap();
    a.call("login", login.clone()).await.unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    a.call("logout", Value::Null).await.unwrap();
    a.set_server(&base).await.unwrap();
    // Administrator disable cannot be undone by automatic recovery.
    sqlx::query("UPDATE devices SET enabled=false WHERE id=$1")
        .bind(Uuid::parse_str(&dev_a).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    a.call("login", login.clone()).await.unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    assert!(a.public_state().await["deviceRestoreError"].is_string());
    sqlx::query("UPDATE devices SET enabled=true WHERE id=$1")
        .bind(Uuid::parse_str(&dev_a).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    a.call("resume", Value::Null).await.unwrap();
    assert_eq!(a.public_state().await["deviceId"], dev_a);
    assert!(a.public_state().await["deviceRestoreError"].is_null());
    // Devices removed remotely are not silently re-created by the saved local key.
    a.call("logout", Value::Null).await.unwrap();
    sqlx::query("UPDATE devices SET bound=false WHERE id=$1")
        .bind(Uuid::parse_str(&dev_a).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    a.call("login", login.clone()).await.unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    assert!(a.public_state().await["deviceRestoreError"].is_null());
    sqlx::query("UPDATE devices SET bound=true WHERE id=$1")
        .bind(Uuid::parse_str(&dev_a).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    // Re-enabling the row doesn't restore the discarded local hint.
    a.call("resume", Value::Null).await.unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    a.call("bind", json!({"name":"Source Windows"}))
        .await
        .unwrap();
    // Explicit unbind also removes the recovery hint; relogin must not re-add it.
    a.call("unbind_device", json!({"id":dev_a})).await.unwrap();
    a.call("logout", Value::Null).await.unwrap();
    a.call("login", login).await.unwrap();
    assert!(a.public_state().await["deviceId"].is_null());
    server.abort();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}

// Real HTTP coordinator + encrypted QUIC. Android platform rows and media/input
// payloads are synthetic; no phone capture or host OS input is injected.
async fn android_protocol_fanout(count: usize, same_owner: bool) {
    let Ok(url) = std::env::var("FARSAIL_TEST_DATABASE_URL") else {
        eprintln!("skipping fanout integration: FARSAIL_TEST_DATABASE_URL not set");
        return;
    };
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("fs_fanout_{}_{}", count, Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(20)
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
        allowed_relays: Arc::new(vec![]),
    });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let source_dir = tempfile::tempdir().unwrap();
    let source = Arc::new(client(source_dir.path().to_path_buf(), &base));
    source.set_server(&base).await.unwrap();
    account(&source, &mail, "fanout-source@example.test").await;
    let source_device = source
        .call("bind", json!({"name":"One synthetic desktop controller"}))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    source
        .start_transport(TransportConfig::default())
        .await
        .unwrap();
    let mut directories = Vec::new();
    let mut hosts = Vec::new();
    let mut devices = Vec::new();
    let mut pairs = Vec::new();
    for index in 0..count {
        let directory = tempfile::tempdir().unwrap();
        let host = Arc::new(client(directory.path().to_path_buf(), &base));
        host.set_server(&base).await.unwrap();
        if same_owner {
            host.call("login", json!({"email":"fanout-source@example.test","password":"correct horse battery staple"})).await.unwrap();
        } else {
            account(&host, &mail, &format!("fanout-phone-{index}@example.test")).await;
        }
        let device = host
            .call("bind", json!({"name":format!("Synthetic Android {index}")}))
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        sqlx::query("UPDATE devices SET platform='android' WHERE id=$1")
            .bind(Uuid::parse_str(&device).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        host.start_transport(TransportConfig::default())
            .await
            .unwrap();
        host.set_host_capability(true).await.unwrap();
        let pair = approved_screen_pair(
            &source,
            &host,
            &source_device,
            &device,
            RemotePermission::Control,
        )
        .await;
        directories.push(directory);
        hosts.push(host);
        devices.push(device);
        pairs.push(pair);
    }
    if same_owner {
        assert_eq!(
            source
                .call("devices", Value::Null)
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            count + 1,
            "one account must retain the desktop and every phone"
        );
    }
    let original: Vec<_> = pairs.iter().map(|(viewer, _)| viewer.stable_id()).collect();
    assert_eq!(
        original
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        count
    );
    let initial_expiry = source
        .call("remote_status", json!({"id":pairs[0].0.id()}))
        .await
        .unwrap()["grant_expires_at"]
        .clone();
    let started = std::time::Instant::now();
    let mut ticks = 0u64;
    let mut frames = 0u64;
    for round in 0..8 {
        source.call("heartbeat", Value::Null).await.unwrap();
        for host in &hosts {
            host.call("heartbeat", Value::Null).await.unwrap();
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let mut tasks = Vec::new();
            for (index, (viewer, host)) in pairs.iter().enumerate() {
                assert!(
                    viewer.is_open().await && host.is_open().await,
                    "{count} peers: closed at round {round}, peer {index}: {} / {}",
                    viewer.end_reason(),
                    host.end_reason()
                );
                let viewer = viewer.clone();
                let host = host.clone();
                tasks.push(tokio::spawn(async move {
                    let label = format!("fanout-{count}-peer-{index}-tick-{ticks}");
                    let mut media = vec![index as u8; 64 * 1024];
                    media[..label.len()].copy_from_slice(label.as_bytes());
                    exchange_media(&viewer, &host, &media).await;
                    let mut input = b"FSI1".to_vec();
                    input.extend_from_slice(&(ticks + 1).to_be_bytes());
                    input.extend_from_slice(
                        &serde_json::to_vec(&json!({"kind":"key","vk":65+index,"down":true}))
                            .unwrap(),
                    );
                    viewer
                        .send(Frame {
                            channel: Channel::Control,
                            bytes: input.clone(),
                        })
                        .await
                        .unwrap();
                    let received =
                        tokio::time::timeout(std::time::Duration::from_secs(5), host.receive())
                            .await
                            .unwrap()
                            .unwrap();
                    assert_eq!(received.channel, Channel::Control);
                    assert_eq!(received.bytes, input, "input crossed phone sessions");
                }));
            }
            for task in tasks {
                task.await.unwrap();
            }
            ticks += 1;
            frames += count as u64;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if round == 3 {
            // One peer restarts while every other connection keeps exchanging data.
            let old = pairs[count - 1].0.stable_id();
            hosts[count - 1].stop_transport().await;
            assert!(!pairs[count - 1].1.is_open().await);
            let (replacement, ()) = tokio::join!(
                async {
                    hosts[count - 1]
                        .start_transport(TransportConfig::default())
                        .await
                        .unwrap();
                    hosts[count - 1].set_host_capability(true).await.unwrap();
                    approved_screen_pair(
                        &source,
                        &hosts[count - 1],
                        &source_device,
                        &devices[count - 1],
                        RemotePermission::Control,
                    )
                    .await
                },
                async {
                    for _ in 0..6 {
                        for (viewer, host) in &pairs[..count - 1] {
                            exchange_media(viewer, host, b"unaffected phone during peer restart")
                                .await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            );
            assert_ne!(replacement.0.stable_id(), old);
            pairs[count - 1] = replacement;
        }
        for index in 0..count - 1 {
            assert_eq!(
                pairs[index].0.stable_id(),
                original[index],
                "unaffected phone reconnected"
            );
        }
    }
    assert_ne!(
        source
            .call("remote_status", json!({"id":pairs[0].0.id()}))
            .await
            .unwrap()["grant_expires_at"],
        initial_expiry
    );
    hosts[count - 1].stop_transport().await;
    for (viewer, host) in &pairs[..count - 1] {
        exchange_media(viewer, host, b"other phones survive final peer close").await;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let report = json!({"verification":"passed","controllers":1,"syntheticAndroidPeers":count,"sameAccount":same_owner,"realCoordinator":true,"realQuic":true,"elapsedSeconds":elapsed,"frames":frames,"payloadBytesPerFrame":65536,"mediaBytes":frames*65536,"controlPackets":frames,"restartedPeers":1,"survivorReconnects":0,"leaseRotated":true,"hostOsInputInjected":false,"physicalPhones":false});
    if let Ok(directory) = std::env::var("FARSAIL_FANOUT_REPORT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            PathBuf::from(directory).join(format!(
                "fanout-{count}{}.json",
                if same_owner { "-same-account" } else { "" }
            )),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    println!("{report}");
    source.stop_transport().await;
    for host in hosts {
        host.stop_transport().await;
    }
    drop(directories);
    server.abort();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn four_android_protocol_fanout() {
    android_protocol_fanout(4, false).await;
}

#[tokio::test]
async fn five_android_protocol_fanout() {
    android_protocol_fanout(5, false).await;
}

#[tokio::test]
async fn four_android_same_account_fanout() {
    android_protocol_fanout(4, true).await;
}
#[tokio::test]
async fn five_android_same_account_fanout() {
    android_protocol_fanout(5, true).await;
}
