use farsail_client::{NativeClient, SecureStore, file_runtime::NativeFiles, files::CHUNK_SIZE};
use farsail_coordinator::{AppState, Mailer, router};
use farsail_core::RemotePermission;
use farsail_transport::{Channel, Config as TransportConfig, Frame};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
use uuid::Uuid;

// Synthetic fixture credentials stay in memory; this integration exercises
// NativeClient/HTTP/QUIC/file actors without accessing the user's keyring.
#[derive(Default)]
struct MemoryStore(Mutex<HashMap<String, Vec<u8>>>);
impl SecureStore for MemoryStore {
    fn read(&self, key: &str) -> farsail_client::Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn write(&self, key: &str, value: &[u8]) -> farsail_client::Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.to_vec());
        Ok(())
    }
    fn delete(&self, key: &str) -> farsail_client::Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    source: Arc<NativeClient>,
    target: Arc<NativeClient>,
    source_files: Arc<NativeFiles>,
    target_files: Arc<NativeFiles>,
    source_device: String,
    target_device: String,
    directory: tempfile::TempDir,
    handles: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.source_files.stop_all();
        self.target_files.stop_all();
        for handle in &self.handles {
            handle.abort();
        }
    }
}

async fn account(client: &NativeClient, mail: &Mailer, email: &str) {
    let credentials = json!({"email":email,"password":"correct horse battery staple"});
    client.call("register", credentials.clone()).await.unwrap();
    let token = mail.messages().last().unwrap().token.clone();
    client
        .call("verify", json!({"email":email,"token":token}))
        .await
        .unwrap();
    client.call("login", credentials).await.unwrap();
}

impl Fixture {
    async fn new() -> Option<Self> {
        let Ok(url) = std::env::var("FARSAIL_TEST_DATABASE_URL") else {
            eprintln!("skipping native Files integration: FARSAIL_TEST_DATABASE_URL not set");
            return None;
        };
        let admin = PgPool::connect(&url).await.unwrap();
        let schema = format!("fs_native_files_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let search_path = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(12)
            .after_connect(move |connection, _| {
                let statement = format!("SET search_path TO {search_path}");
                Box::pin(async move {
                    sqlx::query(&statement).execute(connection).await?;
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
        let source = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        let target = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        source.set_server(&base).await.unwrap();
        target.set_server(&base).await.unwrap();
        account(&source, &mail, "files-source@example.test").await;
        account(&target, &mail, "files-target@example.test").await;
        let source_device = source
            .call("bind", json!({"name":"File source"}))
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let target_device = target
            .call("bind", json!({"name":"File target"}))
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        source
            .start_transport(TransportConfig::default())
            .await
            .unwrap();
        target
            .start_transport(TransportConfig::default())
            .await
            .unwrap();
        assert!(!source.files_enabled());
        assert!(!target.files_enabled());
        let capability = target.set_files_capability(true).await.unwrap();
        assert_eq!(capability["can_host"], false);
        assert_eq!(capability["can_files"], true);
        #[cfg(target_os = "linux")]
        assert_eq!(capability["platform"], "linux");
        let source_files = NativeFiles::new(source.clone());
        let target_files = NativeFiles::new(target.clone());
        let handles = vec![
            server,
            tokio::spawn(source_files.clone().listen()),
            tokio::spawn(target_files.clone().listen()),
        ];
        let directory = tempfile::tempdir().unwrap();
        for folder in ["send", "source-save", "target-save"] {
            std::fs::create_dir(directory.path().join(folder)).unwrap();
        }
        Some(Self {
            admin,
            pool,
            schema,
            source,
            target,
            source_files,
            target_files,
            source_device,
            target_device,
            directory,
            handles,
        })
    }
    fn path(&self, folder: &str, name: &str) -> PathBuf {
        self.directory.path().join(folder).join(name)
    }
    async fn connect_files(&self) -> String {
        let session = self.approve_files().await;
        self.dial_files(&session).await;
        session
    }
    async fn approve_files(&self) -> String {
        self.source.call("heartbeat", Value::Null).await.unwrap();
        self.target.call("heartbeat", Value::Null).await.unwrap();
        let invitation = self
            .target
            .call(
                "invite",
                json!({"target_device_id":self.target_device,"permission":"files"}),
            )
            .await
            .unwrap();
        let request = self.source.call("request", json!({"source_device_id":self.source_device,"target_device_id":self.target_device,"permission":"files","invitation_code":invitation["code"]})).await.unwrap();
        assert_eq!(request["state"], "pending");
        let session = request["id"].as_str().unwrap().to_owned();
        self.target.approve_same_account_pending().await.unwrap();
        assert_eq!(
            self.source
                .call("remote_status", json!({"id":session}))
                .await
                .unwrap()["state"],
            "pending"
        );
        let pending = self.target.call("pending", Value::Null).await.unwrap();
        assert!(
            pending
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == session && item["permission"] == "files")
        );
        self.target
            .call("decide", json!({"id":session,"approve":true}))
            .await
            .unwrap();
        session
    }
    async fn dial_files(&self, session: &str) {
        assert!(
            self.source
                .connect_transport(session, RemotePermission::View)
                .await
                .is_err(),
            "a Files approval cannot establish a screen session"
        );
        let connection = self
            .source
            .connect_transport(session, RemotePermission::Files)
            .await
            .unwrap();
        assert_eq!(connection["state"], "direct");
        wait_connected(&self.source_files, session).await;
        wait_connected(&self.target_files, session).await;
    }
    async fn finish(&self) {
        self.source_files.stop_all();
        self.target_files.stop_all();
        self.source.stop_transport().await;
        self.target.stop_transport().await;
        for handle in &self.handles {
            handle.abort();
        }
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}

async fn wait_connected(files: &NativeFiles, session: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if files
                .status(Some(session))
                .is_ok_and(|status| status["state"] == "connected")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("native file actor received the session event");
}
fn row(files: &NativeFiles, session: &str, transfer: &str) -> Option<Value> {
    files.status(Some(session)).ok()?["transfers"]
        .as_array()?
        .iter()
        .find(|item| item["id"] == transfer)
        .cloned()
}
async fn wait_row(
    files: &NativeFiles,
    session: &str,
    transfer: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(item) = row(files, session, transfer) {
                if predicate(&item) {
                    return item;
                }
                assert_ne!(
                    item["state"], "failed",
                    "unexpected file actor failure: {item}"
                );
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "file state wait timed out: {:?}",
            files.status(Some(session))
        )
    })
}
async fn offer(
    sender: &NativeFiles,
    receiver: &NativeFiles,
    session: &str,
    path: PathBuf,
) -> String {
    sender.send_path(session, path).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = receiver.status(Some(session)).unwrap();
            if let Some(item) = status["transfers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["direction"] == "receive" && item["state"] == "offered")
            {
                return item["id"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("peer received the file offer")
}
fn no_partials(directory: &Path) -> bool {
    std::fs::read_dir(directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".farsail-part-")
    })
}
async fn wait_clean(directory: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !no_partials(directory) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("partial file removed after stopping transfer");
}
async fn complete(
    sender: &NativeFiles,
    receiver: &NativeFiles,
    session: &str,
    path: PathBuf,
    destination: PathBuf,
    expected: &[u8],
) {
    let transfer = offer(sender, receiver, session, path).await;
    save(sender, receiver, session, &transfer, destination, expected).await;
}
async fn save(
    sender: &NativeFiles,
    receiver: &NativeFiles,
    session: &str,
    transfer: &str,
    destination: PathBuf,
    expected: &[u8],
) {
    let incoming = receiver.incoming_offer(session, transfer).unwrap();
    assert_eq!(incoming.size, expected.len() as u64);
    assert!(
        incoming.sha256 == hex::encode(Sha256::digest(expected)),
        "native save consent uses the authenticated offer hash"
    );
    assert!(!destination.exists(), "offers cannot create a destination");
    assert!(
        no_partials(destination.parent().unwrap()),
        "offers cannot create partial files"
    );
    receiver
        .accept_path(session, transfer, destination.clone())
        .await
        .unwrap();
    for files in [sender, receiver] {
        let item = wait_row(files, session, transfer, |item| {
            item["state"] == "completed"
        })
        .await;
        assert_eq!(item["transferred"], expected.len() as u64);
    }
    assert_eq!(std::fs::read(destination).unwrap(), expected);
}

#[tokio::test]
async fn native_files_actor_transfers_multiple_zero_and_bidirectional_files_with_consent() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let session = fixture.connect_files().await;
    let channel = fixture
        .source
        .transport_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(channel.permission(), RemotePermission::Files);
    for forbidden in [Channel::Media, Channel::Control] {
        assert!(matches!(
            channel
                .send(Frame {
                    channel: forbidden,
                    bytes: vec![1]
                })
                .await,
            Err(farsail_transport::Error::Denied)
        ));
    }
    let content: Vec<u8> = (0..CHUNK_SIZE * 5 + 113)
        .map(|offset| (offset % 251) as u8)
        .collect();
    let source = fixture.path("send", "native-payload.bin");
    std::fs::write(&source, &content).unwrap();
    let first = offer(
        &fixture.source_files,
        &fixture.target_files,
        &session,
        source.clone(),
    )
    .await;
    let session_id = Uuid::parse_str(&session).unwrap();
    let before: (Vec<u8>, String) =
        sqlx::query_as("SELECT grant_hash,updated_at::text FROM remote_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    // Native transport is a production dependency here: its five-second poll
    // renews on the third tick. Keep an offer waiting for explicit local save
    // across that real grant rotation on a Linux target with can_host=false.
    tokio::time::sleep(Duration::from_secs(12)).await;
    let after: (Vec<u8>, String) =
        sqlx::query_as("SELECT grant_hash,updated_at::text FROM remote_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert!(
        before.0 != after.0,
        "Files grant hash rotated while awaiting local save"
    );
    assert!(
        before.1 != after.1,
        "Files grant renewal advanced its server timestamp"
    );
    for files in [&fixture.source_files, &fixture.target_files] {
        assert_eq!(files.status(Some(&session)).unwrap()["state"], "connected");
    }
    assert_eq!(
        fixture.target.call("devices", Value::Null).await.unwrap()[0]["can_host"],
        false
    );
    save(
        &fixture.source_files,
        &fixture.target_files,
        &session,
        &first,
        fixture.path("target-save", "locally-chosen.bin"),
        &content,
    )
    .await;
    assert!(!fixture.path("target-save", "native-payload.bin").exists());
    let empty = fixture.path("send", "empty.bin");
    std::fs::write(&empty, []).unwrap();
    complete(
        &fixture.source_files,
        &fixture.target_files,
        &session,
        empty,
        fixture.path("target-save", "empty-saved.bin"),
        &[],
    )
    .await;
    let reverse = fixture.path("send", "reverse.bin");
    std::fs::write(&reverse, b"return over the authenticated Files session").unwrap();
    complete(
        &fixture.target_files,
        &fixture.source_files,
        &session,
        reverse,
        fixture.path("source-save", "returned.bin"),
        b"return over the authenticated Files session",
    )
    .await;

    let declined = offer(
        &fixture.source_files,
        &fixture.target_files,
        &session,
        source,
    )
    .await;
    fixture
        .target_files
        .reject(&session, &declined)
        .await
        .unwrap();
    for files in [&fixture.source_files, &fixture.target_files] {
        wait_row(files, &session, &declined, |item| {
            item["state"] == "rejected"
        })
        .await;
    }
    assert!(no_partials(&fixture.path("target-save", "")));
    fixture.finish().await;
}

async fn partial_transfer(fixture: &Fixture, session: &str, name: &str) -> (String, PathBuf) {
    let source = fixture.path("send", name);
    let content = vec![37; 16 * 1024 * 1024];
    std::fs::write(&source, &content).unwrap();
    let transfer = offer(
        &fixture.source_files,
        &fixture.target_files,
        session,
        source,
    )
    .await;
    let destination = fixture.path("target-save", name);
    fixture
        .target_files
        .accept_path(session, &transfer, destination.clone())
        .await
        .unwrap();
    wait_row(&fixture.target_files, session, &transfer, |item| {
        item["state"] == "transferring"
            && item["transferred"].as_u64().unwrap() >= CHUNK_SIZE as u64
    })
    .await;
    assert!(!destination.exists());
    (transfer, destination)
}

#[tokio::test]
async fn native_files_cancellation_removes_partial_and_allows_next_file() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let session = fixture.connect_files().await;
    for cancel_source in [true, false] {
        let name = if cancel_source {
            "sender-cancelled.bin"
        } else {
            "receiver-cancelled.bin"
        };
        let (transfer, destination) = partial_transfer(&fixture, &session, name).await;
        let canceller = if cancel_source {
            &fixture.source_files
        } else {
            &fixture.target_files
        };
        canceller.cancel(&session, &transfer).await.unwrap();
        for files in [&fixture.source_files, &fixture.target_files] {
            wait_row(files, &session, &transfer, |item| {
                item["state"] == "cancelled"
            })
            .await;
            assert_eq!(files.status(Some(&session)).unwrap()["state"], "connected");
        }
        wait_clean(destination.parent().unwrap()).await;
        assert!(!destination.exists());
    }
    let next = fixture.path("send", "after-cancel.txt");
    std::fs::write(&next, b"fresh transfer after cancel").unwrap();
    complete(
        &fixture.source_files,
        &fixture.target_files,
        &session,
        next,
        fixture.path("target-save", "after-cancel.txt"),
        b"fresh transfer after cancel",
    )
    .await;
    fixture.finish().await;
}

#[tokio::test]
async fn native_files_receive_off_removes_partial_and_requires_fresh_approval() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let session = fixture.connect_files().await;
    let (transfer, destination) = partial_transfer(&fixture, &session, "receive-off.bin").await;
    fixture.target.set_files_capability(false).await.unwrap();
    wait_row(&fixture.target_files, &session, &transfer, |item| {
        matches!(item["state"].as_str(), Some("cancelled" | "failed"))
    })
    .await;
    wait_clean(destination.parent().unwrap()).await;
    assert!(!destination.exists());
    assert!(!fixture.target.files_enabled());
    assert_eq!(
        fixture
            .source
            .call("remote_status", json!({"id":session}))
            .await
            .unwrap()["state"],
        "revoked"
    );
    fixture.target.set_files_capability(true).await.unwrap();
    assert!(
        fixture
            .source
            .connect_transport(&session, RemotePermission::Files)
            .await
            .is_err()
    );
    let fresh = fixture.connect_files().await;
    assert_ne!(fresh, session);
    let source = fixture.path("send", "fresh.txt");
    std::fs::write(&source, b"new consent required").unwrap();
    complete(
        &fixture.source_files,
        &fixture.target_files,
        &fresh,
        source,
        fixture.path("target-save", "fresh.txt"),
        b"new consent required",
    )
    .await;
    fixture.finish().await;
}

#[tokio::test]
async fn native_files_receive_off_revokes_approved_but_not_connected_grant() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let approved = fixture.approve_files().await;
    assert_eq!(
        fixture
            .source
            .call("remote_status", json!({"id":approved}))
            .await
            .unwrap()["state"],
        "approved"
    );
    assert!(
        fixture
            .source
            .transport_session(&approved)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .target
            .transport_session(&approved)
            .await
            .unwrap()
            .is_none()
    );
    fixture.target.set_files_capability(false).await.unwrap();
    fixture.target.set_files_capability(true).await.unwrap();
    assert_eq!(
        fixture
            .source
            .call("remote_status", json!({"id":approved}))
            .await
            .unwrap()["state"],
        "revoked"
    );
    assert!(
        fixture
            .source
            .connect_transport(&approved, RemotePermission::Files)
            .await
            .is_err(),
        "re-enabling reception cannot revive a grant approved before it was disabled"
    );
    let fresh = fixture.connect_files().await;
    assert_ne!(fresh, approved);
    fixture.finish().await;
}

#[tokio::test]
async fn native_files_logout_on_either_end_removes_partial_without_publishing_destination() {
    for logout_source in [false, true] {
        let Some(fixture) = Fixture::new().await else {
            return;
        };
        let session = fixture.connect_files().await;
        let (transfer, destination) = partial_transfer(&fixture, &session, "logout.bin").await;
        let logout_client = if logout_source {
            &fixture.source
        } else {
            &fixture.target
        };
        logout_client.call("logout", Value::Null).await.unwrap();
        wait_row(&fixture.target_files, &session, &transfer, |item| {
            matches!(item["state"].as_str(), Some("cancelled" | "failed"))
        })
        .await;
        wait_clean(destination.parent().unwrap()).await;
        assert!(!destination.exists());
        assert!(!logout_client.files_enabled());
        assert_eq!(logout_client.public_state().await["signedIn"], false);
        assert!(
            fixture
                .target_files
                .accept_path(&session, &transfer, destination)
                .await
                .is_err()
        );
        // The other account can still inspect the revoked request; no former
        // approval may authorize further writes after either participant exits.
        let observer = if logout_source {
            &fixture.target
        } else {
            &fixture.source
        };
        assert_eq!(
            observer
                .call("remote_status", json!({"id":session}))
                .await
                .unwrap()["state"],
            "revoked"
        );
        fixture.finish().await;
    }
}
