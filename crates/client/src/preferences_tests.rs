use super::*;
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use std::sync::atomic::{AtomicU16, AtomicUsize};

const OWNER: &str = "00000000-0000-0000-0000-000000000001";
const DEVICE: &str = "00000000-0000-0000-0000-000000000002";
const SESSION: &str = "00000000-0000-0000-0000-000000000003";

#[derive(Default)]
struct Store(
    std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    AtomicBool,
);
impl SecureStore for Store {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn write(&self, key: &str, value: &[u8]) -> Result<()> {
        if key == KEY && self.1.load(Ordering::SeqCst) {
            return Err(Error::Store("synthetic write failure".into()));
        }
        self.0.lock().unwrap().insert(key.into(), value.to_vec());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
#[tokio::test]
async fn failed_save_does_not_enable_and_failed_opt_out_removes_old_permission() {
    let store = Arc::new(Store::default());
    let f = Fixture::new(store.clone()).await;
    let c = f.remember().await;
    store.1.store(true, Ordering::SeqCst);
    assert!(c.stop_watch_preference().is_err());
    assert_eq!(c.public_state().await["remoteWatch"], false);
    assert!(f.store.read(KEY).unwrap().is_none());
    store.1.store(false, Ordering::SeqCst);
    c.stop_sharing_preference().unwrap();
    c.disable_remote_access().await.unwrap();
    store.1.store(true, Ordering::SeqCst);
    assert!(c.enable_sharing_preference(|| Ok(true)).await.is_err());
    assert!(!c.hosting_enabled());
    assert!(!c.files_enabled());
    assert!(!f.advertised.load(Ordering::SeqCst));
    assert!(!f.files_advertised.load(Ordering::SeqCst));
    c.stop_transport().await;
    f.client()
        .restore_sharing(|| panic!("save failed, no opt-in"))
        .await
        .unwrap();
}
#[tokio::test]
async fn shutdown_before_startup_attempt_keeps_intent_but_never_activates() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let first = f.remember().await;
    first.stop_transport().await;
    let next = f.client();
    next.cancel_sharing_restore();
    next.restore_sharing(|| panic!("already closing"))
        .await
        .unwrap();
    assert!(!next.hosting_enabled());
    assert_eq!(
        next.public_state().await["sharePreferences"]["sharing"],
        true
    );
    let fresh = f.client();
    fresh.restore_sharing(|| Ok(true)).await.unwrap();
    fresh.stop_transport().await;
}
struct Fixture {
    store: Arc<dyn SecureStore>,
    server: tokio::task::JoinHandle<()>,
    advertised: Arc<AtomicBool>,
    files_advertised: Arc<AtomicBool>,
    capability_status: Arc<AtomicU16>,
    enables: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    seen: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    me_status: Arc<AtomicU16>,
    heartbeat_status: Arc<AtomicU16>,
    address_status: Arc<AtomicU16>,
    owner: Arc<std::sync::Mutex<String>>,
}
#[tokio::test]
async fn failed_identity_mutation_keeps_same_identity_intent() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let c = f.remember().await;
    // The fixture rejects these mutations. Cancellation stops activity, but no identity changed.
    assert!(
        c.call(
            "password",
            json!({"current_password":"synthetic","new_password":"synthetic"})
        )
        .await
        .is_err()
    );
    assert_eq!(c.public_state().await["sharePreferences"]["watch"], true);
    assert!(c.call("bind", json!({"name":"synthetic"})).await.is_err());
    assert_eq!(c.public_state().await["sharePreferences"]["sharing"], true);
    assert!(
        c.call("login", json!({"email":"synthetic","password":"synthetic"}))
            .await
            .is_err()
    );
    assert_eq!(c.public_state().await["sharePreferences"]["watch"], true);
    let next = f.client();
    next.restore_sharing(|| Ok(true)).await.unwrap();
    next.stop_transport().await;
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new(store: Arc<dyn SecureStore>) -> Self {
        let advertised = Arc::new(AtomicBool::new(false));
        let files_advertised = Arc::new(AtomicBool::new(false));
        let capability_status = Arc::new(AtomicU16::new(200));
        let enables = Arc::new(AtomicUsize::new(0));
        let hold = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let me_status = Arc::new(AtomicU16::new(200));
        let heartbeat_status = Arc::new(AtomicU16::new(200));
        let address_status = Arc::new(AtomicU16::new(200));
        let owner = Arc::new(std::sync::Mutex::new(OWNER.to_string()));
        let app=Router::new()
            .route("/v1/auth/refresh",post(||async {Json(json!({"access_token":"synthetic-access","refresh_token":"synthetic-refresh","session_id":SESSION,"access_expires_in":300}))}))
            .route("/v1/auth/logout",post(||async {Json(Value::Null)}))
            .route("/v1/me",get({let status=me_status.clone();let owner=owner.clone();move ||{let status=status.clone();let owner=owner.clone();async move {(StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap(),Json(json!({"id":owner.lock().unwrap().clone()})))}}}))
            .route("/v1/devices/heartbeat",post({let status=heartbeat_status.clone();move ||{let status=status.clone();async move {(StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap(),Json(json!({"generation":1})))}}}))
            .route("/v1/devices/endpoint-address",post({let status=address_status.clone();move ||{let status=status.clone();async move {(StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap(),Json(Value::Null))}}}))
            .route("/v1/devices/capability",post({let advertised=advertised.clone();let files_advertised=files_advertised.clone();let status=capability_status.clone();let enables=enables.clone();let hold=hold.clone();let seen=seen.clone();let release=release.clone();move |Json(body):Json<Value>| {
                let advertised=advertised.clone();let files_advertised=files_advertised.clone();let status=status.clone();let enables=enables.clone();let hold=hold.clone();let seen=seen.clone();let release=release.clone();async move {
                    let enabled=body["can_host"]==true || body["can_files"]==true;
                    if enabled { enables.fetch_add(1,Ordering::SeqCst); if hold.load(Ordering::SeqCst) {seen.notify_one();release.notified().await;} }
                    if let Some(value) = body["can_host"].as_bool() { advertised.store(value,Ordering::SeqCst); }
                    if let Some(value) = body["can_files"].as_bool() { files_advertised.store(value,Ordering::SeqCst); }
                    // Failure after mutation models a lost/failed successful-write response.
                    (StatusCode::from_u16(if enabled {status.load(Ordering::SeqCst)} else {200}).unwrap(),Json(Value::Null))
                }
            }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        store.write("server", base.as_bytes()).unwrap();
        store
            .write(
                "login",
                &serde_json::to_vec(&Login {
                    access_token: "synthetic-access".into(),
                    refresh_token: "synthetic-refresh".into(),
                    session_id: SESSION.into(),
                    access_expires_in: 300,
                })
                .unwrap(),
            )
            .unwrap();
        store
            .write(
                "device-token",
                &serde_json::to_vec(&DeviceCredential {
                    id: DEVICE.into(),
                    owner_id: OWNER.into(),
                    session_id: SESSION.into(),
                    device_token: "synthetic-device".into(),
                })
                .unwrap(),
            )
            .unwrap();
        store
            .write(
                &format!(
                    "identity-{}-{}",
                    hex::encode(Sha256::digest(base.as_bytes())),
                    OWNER
                ),
                &[77; 32],
            )
            .unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            store,
            server,
            advertised,
            files_advertised,
            capability_status,
            enables,
            hold,
            seen,
            release,
            me_status,
            heartbeat_status,
            address_status,
            owner,
        }
    }
    fn client(&self) -> Arc<NativeClient> {
        Arc::new(NativeClient::new(self.store.clone()).unwrap())
    }
    async fn remember(&self) -> Arc<NativeClient> {
        let c = self.client();
        c.call("resume", Value::Null).await.unwrap();
        c.call("heartbeat", Value::Null).await.unwrap();
        c.start_transport(TransportConfig::default()).await.unwrap();
        c.enable_sharing_preference(|| Ok(true)).await.unwrap();
        c.enable_watch_preference().await.unwrap();
        c
    }
}

async fn restart_roundtrip(store: Arc<dyn SecureStore>) {
    let f = Fixture::new(store).await;
    let first = f.remember().await;
    first.cancel_sharing_restore();
    first.stop_transport().await;
    assert_eq!(
        first.public_state().await["sharePreferences"]["watch"],
        true
    );
    drop(first);
    let second = f.client();
    assert!(!second.hosting_enabled());
    assert_eq!(
        second.public_state().await["sharePreferences"]["restore"],
        "pending"
    );
    let desktop = AtomicBool::new(false);
    second
        .restore_sharing(|| {
            desktop.store(true, Ordering::SeqCst);
            Ok(true)
        })
        .await
        .unwrap();
    assert!(desktop.load(Ordering::SeqCst));
    assert_eq!(second.public_state().await["remoteWatch"], true);
    second
        .restore_sharing(|| panic!("one startup attempt only"))
        .await
        .unwrap();
    second.stop_watch_preference().unwrap();
    second.stop_transport().await;
    let third = f.client();
    third.restore_sharing(|| Ok(true)).await.unwrap();
    assert!(third.hosting_enabled());
    assert!(third.files_enabled());
    assert_eq!(third.public_state().await["remoteWatch"], false);
    third.stop_sharing_preference().unwrap();
    third.disable_remote_access().await.unwrap();
    third.stop_transport().await;
    let fourth = f.client();
    fourth
        .restore_sharing(|| panic!("explicit opt-out must survive restart"))
        .await
        .unwrap();
    assert!(!fourth.hosting_enabled());
    assert!(!fourth.files_enabled());
    assert_eq!(
        fourth.public_state().await["sharePreferences"]["sharing"],
        false
    );
    assert!(!f.advertised.load(Ordering::SeqCst));
    assert!(!f.files_advertised.load(Ordering::SeqCst));
}
#[tokio::test]
async fn recreate_restores_only_successful_same_identity_choices_and_manual_off() {
    restart_roundtrip(Arc::new(Store::default())).await;
}
#[tokio::test]
async fn unified_access_restores_files_on_both_platforms_from_existing_v1_intent() {
    for screen in [true, false] {
        let f = Fixture::new(Arc::new(Store::default())).await;
        let first = f.client();
        first.call("resume", Value::Null).await.unwrap();
        first.call("heartbeat", Value::Null).await.unwrap();
        first
            .start_transport(TransportConfig::default())
            .await
            .unwrap();
        first
            .enable_sharing_preference(|| Ok(screen))
            .await
            .unwrap();
        assert_eq!(first.hosting_enabled(), screen);
        assert!(first.files_enabled());
        assert_eq!(f.advertised.load(Ordering::SeqCst), screen);
        assert!(f.files_advertised.load(Ordering::SeqCst));
        assert_eq!(first.public_state().await["remoteAccessEnabled"], true);
        let saved: Value = serde_json::from_slice(&f.store.read(KEY).unwrap().unwrap()).unwrap();
        assert_eq!(saved["version"], 1);
        assert_eq!(saved["sharing"], true);
        assert!(
            saved.get("files").is_none(),
            "existing v1 intent needs no extra file opt-in"
        );
        first.stop_transport().await;
        let restored = f.client();
        restored.restore_sharing(|| Ok(screen)).await.unwrap();
        assert_eq!(restored.hosting_enabled(), screen);
        assert!(restored.files_enabled());
        assert_eq!(
            restored.public_state().await["sharePreferences"]["restore"],
            "ready"
        );
        if !screen {
            assert!(restored.enable_watch_preference().await.is_err());
        }
        {
            let mut state = restored.state.lock().await;
            for (id, permission, permission_epoch) in [
                (
                    "screen-approved",
                    RemotePermission::Control,
                    restored.host_state.load(Ordering::SeqCst),
                ),
                (
                    "files-approved",
                    RemotePermission::Files,
                    restored.files_state.load(Ordering::SeqCst),
                ),
            ] {
                state.grant.insert(
                    id.into(),
                    LocalGrant {
                        token: "synthetic".into(),
                        permission,
                        permission_epoch,
                    },
                );
            }
        }
        restored.stop_sharing_preference().unwrap();
        assert!(!restored.hosting_enabled());
        assert!(!restored.files_enabled());
        restored.disable_remote_access().await.unwrap();
        assert!(
            restored.state.lock().await.grant.is_empty(),
            "closing remote access also revokes unused file and screen approvals"
        );
        assert!(!f.advertised.load(Ordering::SeqCst));
        assert!(!f.files_advertised.load(Ordering::SeqCst));
        restored.stop_transport().await;
        f.client()
            .restore_sharing(|| panic!("opt-out persists for both platforms"))
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn failed_unified_advertisement_rolls_back_both_capabilities() {
    for screen in [true, false] {
        for status in [422, 503] {
            let f = Fixture::new(Arc::new(Store::default())).await;
            let c = f.client();
            c.call("resume", Value::Null).await.unwrap();
            c.call("heartbeat", Value::Null).await.unwrap();
            c.start_transport(TransportConfig::default()).await.unwrap();
            f.capability_status.store(status, Ordering::SeqCst);
            assert!(c.enable_sharing_preference(|| Ok(screen)).await.is_err());
            assert!(!c.hosting_enabled());
            assert!(!c.files_enabled());
            assert!(!f.advertised.load(Ordering::SeqCst));
            assert!(!f.files_advertised.load(Ordering::SeqCst));
            assert_eq!(c.public_state().await["sharePreferences"]["sharing"], false);
            c.stop_transport().await;
        }
    }
}
#[tokio::test]
async fn delayed_unified_enable_cannot_survive_transport_stop_on_either_platform() {
    for screen in [true, false] {
        let f = Fixture::new(Arc::new(Store::default())).await;
        let c = f.client();
        c.call("resume", Value::Null).await.unwrap();
        c.call("heartbeat", Value::Null).await.unwrap();
        c.start_transport(TransportConfig::default()).await.unwrap();
        f.hold.store(true, Ordering::SeqCst);
        let enable = tokio::spawn({
            let c = c.clone();
            async move { c.enable_sharing_preference(|| Ok(screen)).await }
        });
        f.seen.notified().await;
        c.stop_transport().await;
        f.release.notify_one();
        assert!(enable.await.unwrap().is_err());
        assert!(!c.hosting_enabled());
        assert!(!c.files_enabled());
        assert!(!f.advertised.load(Ordering::SeqCst));
        assert!(!f.files_advertised.load(Ordering::SeqCst));
        assert_eq!(c.public_state().await["sharePreferences"]["sharing"], false);
    }
}
#[tokio::test]
async fn cancelled_activation_cannot_roll_back_a_newer_explicit_enable() {
    for screen in [true, false] {
        let f = Fixture::new(Arc::new(Store::default())).await;
        let c = f.client();
        c.call("resume", Value::Null).await.unwrap();
        c.call("heartbeat", Value::Null).await.unwrap();
        c.start_transport(TransportConfig::default()).await.unwrap();
        f.hold.store(true, Ordering::SeqCst);
        let old = tokio::spawn({
            let c = c.clone();
            async move { c.enable_sharing_preference(|| Ok(screen)).await }
        });
        f.seen.notified().await;
        c.stop_sharing_preference().unwrap();
        let newer = tokio::spawn({
            let c = c.clone();
            async move { c.enable_sharing_preference(|| Ok(screen)).await }
        });
        tokio::task::yield_now().await;
        assert!(
            !newer.is_finished(),
            "new activation waits through old rollback"
        );
        f.hold.store(false, Ordering::SeqCst);
        f.release.notify_one();
        assert!(old.await.unwrap().is_err());
        newer.await.unwrap().unwrap();
        assert_eq!(c.hosting_enabled(), screen);
        assert!(c.files_enabled());
        assert_eq!(f.advertised.load(Ordering::SeqCst), screen);
        assert!(f.files_advertised.load(Ordering::SeqCst));
        assert_eq!(c.public_state().await["sharePreferences"]["sharing"], true);
        c.stop_transport().await;
    }
}
#[tokio::test]
async fn failed_restore_rechecks_revision_after_waiting_to_roll_back() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let first = f.remember().await;
    first.stop_transport().await;
    let c = f.client();
    let capability = c.capability_lock.lock().await;
    let preflight = Arc::new(tokio::sync::Notify::new());
    let restore = tokio::spawn({
        let c = c.clone();
        let preflight = preflight.clone();
        async move {
            c.restore_sharing(|| {
                preflight.notify_one();
                Err(Error::Invalid("synthetic desktop failure".into()))
            })
            .await
        }
    });
    preflight.notified().await;
    let old_revision = c.preferences.lock().unwrap().revision;
    let newer = tokio::spawn({
        let c = c.clone();
        async move { c.enable_sharing_preference(|| Ok(true)).await }
    });
    while c.preferences.lock().unwrap().revision == old_revision {
        tokio::task::yield_now().await;
    }
    drop(capability);
    assert!(restore.await.unwrap().is_err());
    newer.await.unwrap().unwrap();
    assert!(c.hosting_enabled());
    assert!(c.files_enabled());
    assert!(f.advertised.load(Ordering::SeqCst));
    assert!(f.files_advertised.load(Ordering::SeqCst));
    assert_eq!(
        c.public_state().await["sharePreferences"]["restore"],
        "ready"
    );
    c.stop_transport().await;
}
#[cfg(windows)]
#[tokio::test]
async fn real_dpapi_temporary_profile_restart_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(WindowsStore::new(dir.path().to_path_buf()).unwrap());
    restart_roundtrip(store).await;
    let blob = std::fs::read(dir.path().join("sharing-preferences.bin")).unwrap();
    assert!(
        !blob.windows(7).any(|s| s == b"sharing"),
        "record is encrypted"
    );
}
#[tokio::test]
async fn fresh_corrupt_unknown_schema_and_wrong_identity_all_start_off() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    f.client()
        .restore_sharing(|| panic!("fresh install is off"))
        .await
        .unwrap();
    let saved = f.remember().await;
    saved.stop_transport().await;
    let valid = f.store.read(KEY).unwrap().unwrap();
    let baseline: Value = serde_json::from_slice(&valid).unwrap();
    let mut candidates = vec![b"{broken".to_vec(), vec![b'x'; 4097]];
    for path in [
        "version", "server", "owner", "device", "session", "watch", "extra",
    ] {
        let mut value = baseline.clone();
        match path {
            "version" => value["version"] = json!(2),
            "watch" => value["sharing"] = json!(false),
            "extra" => value["extra"] = json!(true),
            "server" => value["scope"][path] = json!("https://wrong.invalid"),
            _ => value["scope"][path] = json!("00000000-0000-0000-0000-000000000099"),
        }
        candidates.push(serde_json::to_vec(&value).unwrap());
    }
    for bytes in candidates {
        f.store.write(KEY, &bytes).unwrap();
        let c = f.client();
        c.restore_sharing(|| panic!("invalid or foreign identity must not restore"))
            .await
            .unwrap();
        assert_eq!(c.public_state().await["sharePreferences"]["sharing"], false);
    }
    f.store.write(KEY, &valid).unwrap();
    *f.owner.lock().unwrap() = "00000000-0000-0000-0000-000000000099".into();
    let changed = f.client();
    assert!(
        changed
            .restore_sharing(|| panic!("wrong validated owner"))
            .await
            .is_err()
    );
    assert!(f.store.read(KEY).unwrap().is_none());
    assert!(!changed.hosting_enabled());
}
#[tokio::test]
async fn delayed_restore_and_manual_enable_cannot_overwrite_sharing_opt_out() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let first = f.remember().await;
    first.stop_transport().await;
    let c = f.client();
    f.hold.store(true, Ordering::SeqCst);
    let restore = tokio::spawn({
        let c = c.clone();
        async move { c.restore_sharing(|| Ok(true)).await }
    });
    f.seen.notified().await;
    c.stop_sharing_preference().unwrap();
    f.release.notify_one();
    assert!(restore.await.unwrap().is_err());
    assert!(!c.hosting_enabled());
    assert!(!c.files_enabled());
    assert!(!f.advertised.load(Ordering::SeqCst));
    assert!(!f.files_advertised.load(Ordering::SeqCst));
    let enabled = tokio::spawn({
        let c = c.clone();
        async move { c.enable_sharing_preference(|| Ok(true)).await }
    });
    f.seen.notified().await;
    c.stop_sharing_preference().unwrap();
    f.release.notify_one();
    assert!(enabled.await.unwrap().is_err());
    assert!(!c.hosting_enabled());
    assert!(!c.files_enabled());
    c.stop_transport().await;
    f.client()
        .restore_sharing(|| panic!("off persists after late responses"))
        .await
        .unwrap();
}
#[tokio::test]
async fn watch_opt_out_during_restore_preserves_sharing_and_rejects_late_watch_enable() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let first = f.remember().await;
    first.stop_transport().await;
    let c = f.client();
    f.hold.store(true, Ordering::SeqCst);
    let restore = tokio::spawn({
        let c = c.clone();
        async move { c.restore_sharing(|| Ok(true)).await }
    });
    f.seen.notified().await;
    c.stop_watch_preference().unwrap();
    f.release.notify_one();
    restore.await.unwrap().unwrap();
    f.hold.store(false, Ordering::SeqCst);
    assert!(c.hosting_enabled());
    assert!(c.files_enabled());
    assert_eq!(c.public_state().await["remoteWatch"], false);
    let state = c.state.lock().await;
    let watch = tokio::spawn({
        let c = c.clone();
        async move { c.enable_watch_preference().await }
    });
    tokio::task::yield_now().await;
    c.stop_watch_preference().unwrap();
    drop(state);
    assert!(watch.await.unwrap().is_err());
    c.stop_transport().await;
    let next = f.client();
    next.restore_sharing(|| Ok(true)).await.unwrap();
    assert!(next.hosting_enabled());
    assert!(next.files_enabled());
    assert_eq!(next.public_state().await["remoteWatch"], false);
    next.stop_transport().await;
}
#[tokio::test]
async fn logout_or_operational_shutdown_cancels_slow_restore_with_different_persistence() {
    for logout in [false, true] {
        let f = Fixture::new(Arc::new(Store::default())).await;
        let first = f.remember().await;
        first.stop_transport().await;
        let c = f.client();
        f.hold.store(true, Ordering::SeqCst);
        let restore = tokio::spawn({
            let c = c.clone();
            async move { c.restore_sharing(|| Ok(true)).await }
        });
        f.seen.notified().await;
        let mut exit = if logout {
            Some(tokio::spawn({
                let c = c.clone();
                async move { c.call("logout", Value::Null).await }
            }))
        } else {
            c.cancel_sharing_restore();
            None
        };
        if logout {
            tokio::time::timeout(Duration::from_secs(2), async {
                while f.store.read(KEY).unwrap().is_some() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            exit.take().unwrap().await.unwrap().unwrap();
            // The old capability response arrives after both logout and a server switch.
            // Compensation must use its original request identity, not current settings.
            c.set_server("https://another.invalid").await.unwrap();
        }
        f.release.notify_one();
        assert!(restore.await.unwrap().is_err());
        if let Some(exit) = exit {
            exit.await.unwrap().unwrap();
        } else {
            c.stop_transport().await;
        }
        assert!(!c.hosting_enabled());
        assert!(!c.files_enabled());
        assert!(!f.advertised.load(Ordering::SeqCst));
        assert!(!f.files_advertised.load(Ordering::SeqCst));
        assert_eq!(f.store.read(KEY).unwrap().is_some(), !logout);
    }
}
#[tokio::test]
async fn login_heartbeat_transport_and_desktop_fail_closed_without_losing_valid_intent() {
    let f = Fixture::new(Arc::new(Store::default())).await;
    let first = f.remember().await;
    first.stop_transport().await;
    for stage in 0..4 {
        f.me_status
            .store(if stage == 0 { 503 } else { 200 }, Ordering::SeqCst);
        f.heartbeat_status
            .store(if stage == 1 { 503 } else { 200 }, Ordering::SeqCst);
        f.address_status
            .store(if stage == 2 { 503 } else { 200 }, Ordering::SeqCst);
        let c = f.client();
        let count = f.enables.load(Ordering::SeqCst);
        assert!(
            c.restore_sharing(|| Err(Error::Invalid("synthetic unavailable desktop".into())))
                .await
                .is_err()
        );
        assert!(!c.hosting_enabled());
        assert!(!c.files_enabled());
        let state = c.public_state().await;
        assert_eq!(state["remoteWatch"], false);
        assert_eq!(state["sharePreferences"]["sharing"], true);
        assert_eq!(state["sharePreferences"]["restore"], "failed");
        assert_eq!(f.enables.load(Ordering::SeqCst), count);
        c.restore_sharing(|| panic!("no failure retry loop"))
            .await
            .unwrap();
        c.stop_transport().await;
    }
}
