//! FarSail native account and device client. Credentials never cross the UI bridge.
use ed25519_dalek::{Signer, SigningKey};
use farsail_core::RemotePermission;
use farsail_transport::{Authority, Claims, Config as TransportConfig, Session, Transport};
use iroh::EndpointAddr;
use rand::rngs::OsRng;
use reqwest::{Client as Http, Method};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, broadcast, watch};
use url::Url;
mod preferences;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("HTTP {0}: {1}")]
    Http(u16, String),
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("local credential store: {0}")]
    Store(String),
    #[error("sign in first")]
    SignedOut,
    #[error("bind this device first")]
    Unbound,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Only HTTPS is accepted outside the explicit loopback development boundary.
pub fn validate_base(input: &str) -> Result<String> {
    let u = Url::parse(input).map_err(|_| Error::Invalid("invalid server URL".into()))?;
    let loopback = match u.host() {
        Some(url::Host::Domain(x)) => x == "localhost",
        Some(url::Host::Ipv4(x)) => x.is_loopback(),
        Some(url::Host::Ipv6(x)) => x.is_loopback(),
        None => return Err(Error::Invalid("server host required".into())),
    };
    if !(u.scheme() == "https" || u.scheme() == "http" && loopback)
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
    {
        return Err(Error::Invalid(
            "use HTTPS, or HTTP on loopback only; no URL credentials, path or query".into(),
        ));
    }
    Ok(u.as_str().trim_end_matches('/').to_string())
}

pub trait SecureStore: Send + Sync {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &str, value: &[u8]) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Windows user scoped DPAPI. Separate encrypted blobs keep login, device token and signing key apart.
pub struct WindowsStore {
    directory: PathBuf,
}
impl WindowsStore {
    pub fn new(directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&directory).map_err(|e| Error::Store(e.to_string()))?;
        Ok(Self { directory })
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        if !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            return Err(Error::Store("invalid credential key".into()));
        }
        Ok(self.directory.join(format!("{key}.bin")))
    }
}
impl SecureStore for WindowsStore {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path(key)?;
        if !path.exists() {
            return Ok(None);
        }
        let encrypted = fs::read(path).map_err(|e| Error::Store(e.to_string()))?;
        Ok(Some(dpapi::unprotect(&encrypted)?))
    }
    fn write(&self, key: &str, value: &[u8]) -> Result<()> {
        let path = self.path(key)?;
        let encrypted = dpapi::protect(value)?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, encrypted).map_err(|e| Error::Store(e.to_string()))?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| Error::Store(e.to_string()))?;
        }
        fs::rename(&temp, path).map_err(|e| Error::Store(e.to_string()))
    }
    fn delete(&self, key: &str) -> Result<()> {
        match fs::remove_file(self.path(key)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Store(e.to_string())),
        }
    }
}

#[cfg(windows)]
mod dpapi {
    use super::{Error, Result};
    use std::ffi::c_void;
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *mut Blob,
            description: *const u16,
            entropy: *mut Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *mut Blob,
            description: *mut *mut u16,
            entropy: *mut Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(ptr: *mut c_void) -> *mut c_void;
    }
    fn convert(value: &[u8], encrypt: bool) -> Result<Vec<u8>> {
        let mut input = Blob {
            size: value.len() as u32,
            data: value.as_ptr() as *mut u8,
        };
        let mut output = Blob {
            size: 0,
            data: std::ptr::null_mut(),
        };
        // SAFETY: input is valid for the call; Windows allocates output which is copied and freed.
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &mut input,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    1,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &mut input,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    1,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err(Error::Store(std::io::Error::last_os_error().to_string()));
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.data, output.size as usize).to_vec() };
        unsafe {
            LocalFree(output.data.cast());
        }
        Ok(bytes)
    }
    pub fn protect(x: &[u8]) -> Result<Vec<u8>> {
        convert(x, true)
    }
    pub fn unprotect(x: &[u8]) -> Result<Vec<u8>> {
        convert(x, false)
    }
}
#[cfg(not(windows))]
mod dpapi {
    use super::{Error, Result};
    pub fn protect(_: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Store("Windows DPAPI required".into()))
    }
    pub fn unprotect(_: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Store("Windows DPAPI required".into()))
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Login {
    access_token: String,
    refresh_token: String,
    session_id: String,
    access_expires_in: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct DeviceCredential {
    id: String,
    device_token: String,
    owner_id: String,
    session_id: String,
}
struct State {
    base: String,
    login: Option<Login>,
    expiry: Instant,
    owner: Option<String>,
    device: Option<DeviceCredential>,
    generation: Option<i64>,
    grant: std::collections::HashMap<String, String>,
}
struct DeviceSnapshot {
    base: String,
    token: String,
    generation: i64,
}
impl DeviceSnapshot {
    fn from_state(s: &State) -> Result<Self> {
        Ok(Self {
            base: s.base.clone(),
            token: s
                .device
                .as_ref()
                .ok_or(Error::Unbound)?
                .device_token
                .clone(),
            generation: s
                .generation
                .ok_or_else(|| Error::Invalid("heartbeat required".into()))?,
        })
    }
    fn current(&self, s: &State) -> bool {
        s.base == self.base
            && s.generation == Some(self.generation)
            && s.device
                .as_ref()
                .is_some_and(|d| d.device_token == self.token)
    }
}
pub struct NativeClient {
    http: Http,
    store: Arc<dyn SecureStore>,
    state: Mutex<State>,
    transport: Mutex<Option<Transport>>,
    transport_sessions: Mutex<std::collections::HashMap<String, Session>>,
    transport_epoch: Mutex<u64>,
    // Serialize old/new endpoint uploads so a delayed old address cannot win
    // after the replacement endpoint publishes in the same heartbeat generation.
    transport_publication: Mutex<()>,
    signing_out: AtomicBool,
    cancel_tx: watch::Sender<u64>,
    host_state: std::sync::atomic::AtomicU64,
    auto_approve: std::sync::atomic::AtomicU64,
    session_tx: broadcast::Sender<(Session, bool)>,
    capability_lock: Mutex<()>,
    preferences: std::sync::Mutex<preferences::Preferences>,
}

impl NativeClient {
    pub fn new(store: Arc<dyn SecureStore>) -> Result<Self> {
        let base = store
            .read("server")?
            .and_then(|x| String::from_utf8(x).ok())
            .unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let base = validate_base(&base)?;
        let login = read_json::<Login>(&*store, "login")?;
        let device = read_json::<DeviceCredential>(&*store, "device-token")?;
        let (cancel_tx, _) = watch::channel(0u64);
        let (session_tx, _) = broadcast::channel(16);
        let state = State {
            base,
            login,
            expiry: Instant::now(),
            owner: None,
            device,
            generation: None,
            grant: Default::default(),
        };
        let preferences = preferences::Preferences::load(&*store, &state);
        Ok(Self {
            http: Http::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            store,
            state: Mutex::new(state),
            transport: Mutex::new(None),
            transport_sessions: Mutex::new(Default::default()),
            transport_epoch: Mutex::new(0),
            transport_publication: Mutex::new(()),
            signing_out: AtomicBool::new(false),
            cancel_tx,
            host_state: std::sync::atomic::AtomicU64::new(0),
            auto_approve: std::sync::atomic::AtomicU64::new(0),
            session_tx,
            capability_lock: Mutex::new(()),
            preferences: std::sync::Mutex::new(preferences),
        })
    }
    pub async fn public_state(&self) -> Value {
        let s = self.state.lock().await;
        let preferences = self.preferences.lock().unwrap().public(&s);
        let mut result = json!({"server": s.base, "signedIn": s.login.is_some(), "deviceId": s.device.as_ref().map(|x| &x.id), "sharing":self.hosting_enabled(), "remoteWatch":self.auto_approve.load(Ordering::SeqCst) & 1 == 1, "sharePreferences":preferences});
        drop(s);
        result["transportRunning"] = json!(self.transport_running().await);
        result
    }
    pub fn subscribe_sessions(&self) -> broadcast::Receiver<(Session, bool)> {
        self.session_tx.subscribe()
    }
    pub fn hosting_enabled(&self) -> bool {
        self.host_state.load(Ordering::SeqCst) & 1 == 1
    }
    pub fn disable_host_local(&self) {
        self.set_remote_watch(false).ok();
        let _ = self
            .host_state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                Some(state.wrapping_add(2) & !1)
            });
    }
    /// Runtime-only switch. Explicit user choices use the separate preference methods.
    pub fn set_remote_watch(&self, enabled: bool) -> Result<()> {
        let host_epoch = self.host_state.load(Ordering::SeqCst);
        if enabled && host_epoch & 1 == 0 {
            return Err(Error::Invalid("请先开启本机共享".into()));
        }
        self.auto_approve
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                Some(n.wrapping_add(2) & !1 | u64::from(enabled))
            })
            .ok();
        if enabled && self.host_state.load(Ordering::SeqCst) != host_epoch {
            self.set_remote_watch(false)?;
            return Err(Error::Invalid("共享已停止，远程值守未开启".into()));
        }
        Ok(())
    }
    pub async fn approve_same_account_pending(&self) -> Result<()> {
        let epoch = self.auto_approve.load(Ordering::SeqCst);
        let host_epoch = self.host_state.load(Ordering::SeqCst);
        if epoch & 1 == 0 || host_epoch & 1 == 0 {
            return Ok(());
        }
        let mut s = self.state.lock().await;
        let owner = s.owner.clone().ok_or(Error::Unbound)?;
        if s.device.as_ref().is_none_or(|d| d.owner_id != owner) {
            return Err(Error::Unbound);
        }
        let rows = self
            .device(&mut s, Method::GET, "/v1/remote/pending", None)
            .await?;
        for row in rows.as_array().into_iter().flatten() {
            if row["requester_id"].as_str() != Some(&owner)
                || !matches!(row["permission"].as_str(), Some("view" | "control"))
            {
                continue;
            }
            if self.auto_approve.load(Ordering::SeqCst) != epoch
                || self.host_state.load(Ordering::SeqCst) != host_epoch
            {
                break;
            }
            let id = uuid(row["id"].as_str().ok_or(Error::Unbound)?)?;
            let result = self
                .device(
                    &mut s,
                    Method::POST,
                    &format!("/v1/remote/{id}/decide"),
                    Some(json!({"approve":true})),
                )
                .await;
            if self.auto_approve.load(Ordering::SeqCst) != epoch
                || self.host_state.load(Ordering::SeqCst) != host_epoch
            {
                // A late approval cannot survive opt-out, stop, logout or identity change.
                let _ = self
                    .device(
                        &mut s,
                        Method::POST,
                        &format!("/v1/remote/{id}/revoke"),
                        None,
                    )
                    .await;
                break;
            }
            if let Ok(v) = result
                && let Some(token) = v["grant_token"].as_str()
            {
                s.grant.insert(id, token.into());
            }
        }
        Ok(())
    }
    pub async fn revoke_host_approvals(&self) -> Result<()> {
        let mut s = self.state.lock().await;
        let ids: Vec<_> = s.grant.drain().map(|(id, _)| id).collect();
        let mut failed = None;
        for id in ids {
            if let Err(e) = self
                .device(
                    &mut s,
                    Method::POST,
                    &format!("/v1/remote/{id}/revoke"),
                    None,
                )
                .await
            {
                failed = Some(e);
            }
        }
        // The local grants are already removed, even if the coordinator is unreachable.
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    pub async fn transport_running(&self) -> bool {
        self.transport.lock().await.is_some()
    }
    pub async fn set_host_capability(&self, enabled: bool) -> Result<Value> {
        self.set_host_capability_inner(enabled, None).await
    }
    async fn set_host_capability_for_preference(&self, revision: u64) -> Result<Value> {
        self.set_host_capability_inner(true, Some(revision)).await
    }
    async fn set_host_capability_inner(
        &self,
        enabled: bool,
        preference: Option<u64>,
    ) -> Result<Value> {
        if !enabled {
            self.disable_host_local();
        }
        let started = self.host_state.load(Ordering::SeqCst);
        let _guard = self.capability_lock.lock().await;
        if enabled
            && (self.host_state.load(Ordering::SeqCst) != started
                || preference.is_some_and(|r| !self.preference_current(r)))
        {
            return Err(Error::Invalid("sharing start cancelled".into()));
        }
        let identity = DeviceSnapshot::from_state(&*self.state.lock().await)?;
        let generation = identity.generation;
        let result = self
            .transport_request_as(
                &identity,
                Method::POST,
                "/v1/devices/capability",
                None,
                Some(json!({"generation":generation,"can_host":enabled})),
            )
            .await;
        if enabled && result.is_err() {
            self.disable_host_local();
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.transport_request_as(
                    &identity,
                    Method::POST,
                    "/v1/devices/capability",
                    None,
                    Some(json!({"generation":generation,"can_host":false})),
                ),
            )
            .await;
        }
        if result.is_ok() && enabled {
            let still_current = self.host_state.load(Ordering::SeqCst) == started
                && preference.is_none_or(|r| self.preference_current(r))
                && !self.signing_out.load(Ordering::SeqCst)
                && self.transport_running().await
                && identity.current(&*self.state.lock().await);
            if !still_current {
                let _ = self
                    .transport_request_as(
                        &identity,
                        Method::POST,
                        "/v1/devices/capability",
                        None,
                        Some(json!({"generation":generation,"can_host":false})),
                    )
                    .await;
                return Err(Error::Invalid("sharing start cancelled".into()));
            }
            if self
                .host_state
                .compare_exchange(started, started | 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                let _ = self
                    .transport_request_as(
                        &identity,
                        Method::POST,
                        "/v1/devices/capability",
                        None,
                        Some(json!({"generation":generation,"can_host":false})),
                    )
                    .await;
                return Err(Error::Invalid("sharing start cancelled".into()));
            }
        }
        result
    }
    pub async fn clear_stale_host_capability(&self) -> Result<()> {
        let _guard = self.capability_lock.lock().await;
        if self.hosting_enabled() {
            return Ok(());
        }
        let generation = self.state.lock().await.generation.ok_or(Error::Unbound)?;
        self.transport_request(
            Method::POST,
            "/v1/devices/capability",
            None,
            Some(json!({"generation":generation,"can_host":false})),
        )
        .await?;
        Ok(())
    }
    pub async fn set_server(&self, base: &str) -> Result<Value> {
        let base = validate_base(base)?;
        let mut s = self.state.lock().await;
        if s.login.is_some() {
            return Err(Error::Invalid("sign out before changing server".into()));
        }
        if s.base != base {
            self.forget_sharing_preferences()?;
        }
        self.store.write("server", base.as_bytes())?;
        s.base = base;
        Ok(json!({"server":s.base}))
    }
    async fn raw(
        &self,
        s: &State,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        device_header: Option<&str>,
    ) -> Result<Value> {
        if self.signing_out.load(Ordering::SeqCst) {
            return Err(Error::Invalid("request cancelled by sign-out".into()));
        }
        let mut cancel = self.cancel_tx.subscribe();
        let mut r = self.http.request(method, format!("{}{}", s.base, path));
        if let Some(x) = bearer {
            r = r.bearer_auth(x);
        }
        if let Some(x) = device_header {
            r = r.header("X-Farsail-Device-Token", x);
        }
        if let Some(x) = body {
            r = r.json(&x);
        }
        let mut response = tokio::select! {
            result = r.send() => result?,
            _ = cancel.changed() => return Err(Error::Invalid("request cancelled by sign-out".into())),
        };
        let status = response.status();
        const MAX_RESPONSE: usize = 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(Error::Invalid("server response too large".into()));
        }
        let mut bytes = Vec::new();
        loop {
            let next = tokio::select! {
                result = response.chunk() => result?,
                _ = cancel.changed() => return Err(Error::Invalid("request cancelled by sign-out".into())),
            };
            let Some(chunk) = next else {
                break;
            };
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Err(Error::Invalid("server response too large".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let message = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| status.canonical_reason().unwrap_or("request failed").into());
            return Err(Error::Http(status.as_u16(), message));
        }
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("invalid server JSON".into()))
        }
    }
    async fn access(&self, s: &mut State) -> Result<String> {
        let login = s.login.clone().ok_or(Error::SignedOut)?;
        if Instant::now() + Duration::from_secs(30) < s.expiry {
            return Ok(login.access_token);
        }
        let v = self
            .raw(
                s,
                Method::POST,
                "/v1/auth/refresh",
                Some(json!({"refresh_token":login.refresh_token})),
                None,
                None,
            )
            .await;
        match v {
            Ok(v) => {
                let next: Login = serde_json::from_value(v)
                    .map_err(|_| Error::Invalid("invalid refresh response".into()))?;
                self.store
                    .write("login", &serde_json::to_vec(&next).unwrap())?;
                s.expiry = Instant::now() + Duration::from_secs(next.access_expires_in);
                s.login = Some(next.clone());
                Ok(next.access_token)
            }
            Err(e) => {
                if matches!(e, Error::Http(400 | 401 | 403, _)) {
                    self.clear_auth(s)?;
                }
                Err(e)
            }
        }
    }
    fn clear_auth(&self, s: &mut State) -> Result<()> {
        let preferences = self.forget_sharing_preferences();
        if let Ok(mut sessions) = self.transport_sessions.try_lock() {
            for session in sessions.values() {
                session.close();
            }
            sessions.clear();
        }
        s.login = None;
        s.owner = None;
        s.device = None;
        s.generation = None;
        s.grant.clear();
        self.store.delete("login")?;
        self.store.delete("device-token")?;
        preferences
    }
    async fn user(
        &self,
        s: &mut State,
        method: Method,
        path: &str,
        body: Option<Value>,
        source: bool,
    ) -> Result<Value> {
        let access = self.access(s).await?;
        let device = if source {
            Some(
                s.device
                    .as_ref()
                    .ok_or(Error::Unbound)?
                    .device_token
                    .as_str(),
            )
        } else {
            None
        };
        let result = self
            .raw(s, method.clone(), path, body.clone(), Some(&access), device)
            .await;
        if matches!(&result, Err(Error::Http(401, _))) {
            s.expiry = Instant::now();
            let access = self.access(s).await?;
            let device = if source {
                Some(
                    s.device
                        .as_ref()
                        .ok_or(Error::Unbound)?
                        .device_token
                        .as_str(),
                )
            } else {
                None
            };
            return self.raw(s, method, path, body, Some(&access), device).await;
        }
        result
    }
    async fn device(
        &self,
        s: &mut State,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let token = s
            .device
            .as_ref()
            .ok_or(Error::Unbound)?
            .device_token
            .clone();
        let result = self.raw(s, method, path, body, Some(&token), None).await;
        if matches!(&result, Err(Error::Http(401, _))) {
            let preference = self.forget_sharing_preferences();
            s.device = None;
            s.generation = None;
            self.store.delete("device-token")?;
            preference?;
        }
        result
    }
    pub async fn call(&self, op: &str, args: Value) -> Result<Value> {
        if op == "devices" {
            return self.list_devices().await;
        }
        if op == "logout" {
            let preferences = self.forget_sharing_preferences();
            self.signing_out.store(true, Ordering::SeqCst);
            self.cancel_tx.send_modify(|v| *v = v.wrapping_add(1));
            self.stop_transport().await;
            let mut s = self.state.lock().await;
            let base = s.base.clone();
            let access = s.login.as_ref().map(|x| x.access_token.clone());
            let local = self.clear_auth(&mut s);
            drop(s);
            self.signing_out.store(false, Ordering::SeqCst);
            local?;
            preferences?;
            let access = access.ok_or(Error::SignedOut)?;
            let remote = tokio::time::timeout(
                Duration::from_secs(2),
                self.http
                    .post(format!("{base}/v1/auth/logout"))
                    .bearer_auth(access)
                    .send(),
            )
            .await;
            return match remote {
                Ok(Ok(r)) if r.status().is_success() => Ok(Value::Null),
                _ => Err(Error::Invalid(
                    "local sign-out complete; server revocation unconfirmed".into(),
                )),
            };
        }
        if matches!(op, "login" | "bind" | "unbind_device" | "password") {
            self.cancel_sharing_restore();
            self.stop_transport().await;
        }
        if op == "revoke_remote"
            && let Some(id) = args.get("id").and_then(Value::as_str)
        {
            let _ = self.close_transport_session(id).await;
        }
        let mut s = self.state.lock().await;
        let string = |key: &str| -> Result<String> {
            args.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| Error::Invalid(format!("{key} required")))
        };
        match op {
            "register" => {
                self.raw(
                    &s,
                    Method::POST,
                    "/v1/auth/register",
                    Some(args),
                    None,
                    None,
                )
                .await
            }
            "verify" => {
                self.raw(&s, Method::POST, "/v1/auth/verify", Some(args), None, None)
                    .await
            }
            "resend" => {
                self.raw(
                    &s,
                    Method::POST,
                    "/v1/auth/verify/resend",
                    Some(args),
                    None,
                    None,
                )
                .await
            }
            "recover_request" => {
                self.raw(
                    &s,
                    Method::POST,
                    "/v1/auth/recovery/request",
                    Some(args),
                    None,
                    None,
                )
                .await
            }
            "recover_complete" => {
                self.raw(
                    &s,
                    Method::POST,
                    "/v1/auth/recovery/complete",
                    Some(args),
                    None,
                    None,
                )
                .await
            }
            "login" => {
                if s.login.is_some() {
                    return Err(Error::Invalid("sign out before switching account".into()));
                }
                self.clear_auth(&mut s)?;
                let v = self
                    .raw(&s, Method::POST, "/v1/auth/login", Some(args), None, None)
                    .await?;
                let login: Login = serde_json::from_value(v)
                    .map_err(|_| Error::Invalid("invalid login response".into()))?;
                self.store
                    .write("login", &serde_json::to_vec(&login).unwrap())?;
                s.expiry = Instant::now() + Duration::from_secs(login.access_expires_in);
                s.login = Some(login);
                let me = self
                    .user(&mut s, Method::GET, "/v1/me", None, false)
                    .await?;
                s.owner = me.get("id").and_then(Value::as_str).map(str::to_owned);
                Ok(me)
            }
            "resume" | "me" => {
                let me = self
                    .user(&mut s, Method::GET, "/v1/me", None, false)
                    .await?;
                s.owner = me.get("id").and_then(Value::as_str).map(str::to_owned);
                if s.device.as_ref().is_some_and(|d| {
                    Some(&d.owner_id) != s.owner.as_ref()
                        || s.login
                            .as_ref()
                            .is_some_and(|l| d.session_id != l.session_id)
                }) {
                    self.forget_sharing_preferences()?;
                    s.device = None;
                    s.generation = None;
                    self.store.delete("device-token")?;
                }
                Ok(me)
            }
            "password" => {
                let v = self
                    .user(&mut s, Method::POST, "/v1/auth/password", Some(args), false)
                    .await?;
                self.clear_auth(&mut s)?;
                Ok(v)
            }
            "bind" => {
                let me = self
                    .user(&mut s, Method::GET, "/v1/me", None, false)
                    .await?;
                let owner = me["id"]
                    .as_str()
                    .ok_or_else(|| Error::Invalid("missing account id".into()))?
                    .to_owned();
                let key_name = format!(
                    "identity-{}-{}",
                    hex::encode(Sha256::digest(s.base.as_bytes())),
                    owner
                );
                let signing = match self.store.read(&key_name)? {
                    Some(bytes) => SigningKey::from_bytes(
                        &bytes
                            .try_into()
                            .map_err(|_| Error::Store("invalid signing key".into()))?,
                    ),
                    None => {
                        let key = SigningKey::generate(&mut OsRng);
                        self.store.write(&key_name, &key.to_bytes())?;
                        key
                    }
                };
                let challenge = self
                    .user(
                        &mut s,
                        Method::POST,
                        "/v1/devices/challenge",
                        Some(json!({"public_key":hex::encode(signing.verifying_key().to_bytes())})),
                        false,
                    )
                    .await?;
                let message = challenge["message"]
                    .as_str()
                    .ok_or_else(|| Error::Invalid("missing challenge message".into()))?;
                let bind = self.user(&mut s, Method::POST, "/v1/devices/bind", Some(json!({"challenge_id":challenge["challenge_id"], "signature":hex::encode(signing.sign(message.as_bytes()).to_bytes()), "name":string("name")?, "platform":"windows", "can_host":false, "can_files":false})), false).await?;
                let credential = DeviceCredential {
                    id: bind["id"]
                        .as_str()
                        .ok_or_else(|| Error::Invalid("missing device id".into()))?
                        .into(),
                    device_token: bind["device_token"]
                        .as_str()
                        .ok_or_else(|| Error::Invalid("missing device credential".into()))?
                        .into(),
                    owner_id: owner,
                    session_id: s.login.as_ref().unwrap().session_id.clone(),
                };
                self.forget_sharing_preferences()?;
                self.store
                    .write("device-token", &serde_json::to_vec(&credential).unwrap())?;
                s.device = Some(credential);
                s.generation = None;
                Ok(json!({"id":s.device.as_ref().unwrap().id}))
            }
            "heartbeat" => {
                let generation = s.generation;
                let v = self
                    .device(
                        &mut s,
                        Method::POST,
                        "/v1/devices/heartbeat",
                        Some(json!({"generation":generation})),
                    )
                    .await?;
                s.generation = v["generation"].as_i64();
                Ok(v)
            }
            "rename_device" => {
                self.user(
                    &mut s,
                    Method::PATCH,
                    &format!("/v1/devices/{}", uuid(&string("id")?)?),
                    Some(json!({"name":string("name")?})),
                    false,
                )
                .await
            }
            "unbind_device" => {
                let id = string("id")?;
                if s.device.as_ref().is_some_and(|d| d.id == id) {
                    self.forget_sharing_preferences()?;
                }
                let v = self
                    .user(
                        &mut s,
                        Method::DELETE,
                        &format!("/v1/devices/{}", uuid(&id)?),
                        None,
                        false,
                    )
                    .await?;
                if s.device.as_ref().is_some_and(|d| d.id == id) {
                    s.device = None;
                    s.generation = None;
                    self.store.delete("device-token")?;
                }
                Ok(v)
            }
            "sessions" => {
                self.user(&mut s, Method::GET, "/v1/auth/sessions", None, false)
                    .await
            }
            "revoke_session" => {
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/auth/sessions/{}/revoke", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "remote_sessions" => {
                self.user(&mut s, Method::GET, "/v1/remote/sessions", None, false)
                    .await
            }
            "remote_status" => {
                self.user(
                    &mut s,
                    Method::GET,
                    &format!("/v1/remote/{}", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "pending" => {
                self.device(&mut s, Method::GET, "/v1/remote/pending", None)
                    .await
            }
            "invite" => {
                self.user(&mut s, Method::POST, "/v1/invitations", Some(args), false)
                    .await
            }
            "revoke_invite" => {
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/invitations/{}/revoke", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "request" => {
                self.user(&mut s, Method::POST, "/v1/remote/request", Some(args), true)
                    .await
            }
            "revoke_remote" => {
                s.grant.remove(&uuid(&string("id")?)?);
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/remote/{}/revoke", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "decide" => {
                let id = uuid(&string("id")?)?;
                let v = self
                    .device(
                        &mut s,
                        Method::POST,
                        &format!("/v1/remote/{id}/decide"),
                        Some(json!({"approve":args["approve"]})),
                    )
                    .await?;
                if let Some(token) = v.get("grant_token").and_then(Value::as_str) {
                    s.grant.insert(id, token.into());
                }
                Ok(json!({"approved":v.is_object()}))
            }
            "admin_users" => {
                page_user(
                    self,
                    &mut s,
                    "/v1/admin/users",
                    args.get("after").and_then(Value::as_str),
                )
                .await
            }
            "admin_user_enabled" => {
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/admin/users/{}/enabled", uuid(&string("id")?)?),
                    Some(json!({"enabled":args["enabled"]})),
                    false,
                )
                .await
            }
            "admin_device_revoke" => {
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/admin/devices/{}/revoke", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "admin_device_enabled" => {
                self.user(
                    &mut s,
                    Method::POST,
                    &format!("/v1/admin/devices/{}/enabled", uuid(&string("id")?)?),
                    Some(json!({"enabled":true})),
                    false,
                )
                .await
            }
            "admin_registration" => {
                self.user(&mut s, Method::GET, "/v1/admin/registration", None, false)
                    .await
            }
            "admin_set_registration" => {
                self.user(
                    &mut s,
                    Method::PUT,
                    "/v1/admin/registration",
                    Some(args),
                    false,
                )
                .await
            }
            "admin_invite" => {
                self.user(
                    &mut s,
                    Method::POST,
                    "/v1/admin/signup-invitations",
                    Some(args),
                    false,
                )
                .await
            }
            "admin_invite_revoke" => {
                self.user(
                    &mut s,
                    Method::DELETE,
                    &format!("/v1/admin/signup-invitations/{}", uuid(&string("id")?)?),
                    None,
                    false,
                )
                .await
            }
            "admin_sessions" => {
                self.user(&mut s, Method::GET, "/v1/admin/sessions", None, false)
                    .await
            }
            "admin_audit" => {
                self.user(&mut s, Method::GET, "/v1/admin/audit", None, false)
                    .await
            }
            _ => Err(Error::Invalid("unknown action".into())),
        }
    }
    async fn list_devices(&self) -> Result<Value> {
        let mut all = Vec::new();
        let mut after: Option<String> = None;
        let mut identity: Option<(String, String)> = None;
        loop {
            let mut s = self.state.lock().await;
            let login = s.login.as_ref().ok_or(Error::SignedOut)?;
            let current = (s.base.clone(), login.session_id.clone());
            if identity.as_ref().is_some_and(|x| x != &current) {
                return Err(Error::Invalid("account changed during device list".into()));
            }
            identity = Some(current);
            let path = format!(
                "/v1/devices?limit=100{}",
                after
                    .as_ref()
                    .map(|x| format!("&after={x}"))
                    .unwrap_or_default()
            );
            let page = self.user(&mut s, Method::GET, &path, None, false).await?;
            let rows = page
                .as_array()
                .ok_or_else(|| Error::Invalid("invalid device list".into()))?;
            if rows.is_empty() {
                break;
            }
            if rows.len() > 100 || all.len() > 10_000 {
                return Err(Error::Invalid("device list exceeds limit".into()));
            }
            let mut next = after.clone();
            for row in rows {
                let id = uuid(
                    row["id"]
                        .as_str()
                        .ok_or_else(|| Error::Invalid("missing device cursor".into()))?,
                )?;
                if next.as_ref().is_some_and(|x| id <= *x) {
                    return Err(Error::Invalid("device cursor did not advance".into()));
                }
                next = Some(id);
            }
            all.extend(rows.iter().cloned());
            after = next;
            if rows.len() < 100 {
                break;
            }
        }
        Ok(Value::Array(all))
    }
}
impl NativeClient {
    async fn transport_request(
        &self,
        method: Method,
        path: &str,
        grant: Option<&str>,
        body: Option<Value>,
    ) -> Result<Value> {
        let identity = DeviceSnapshot::from_state(&*self.state.lock().await)?;
        self.transport_request_as(&identity, method, path, grant, body)
            .await
    }
    // A delayed capability response must be compensated against its original identity,
    // even after sign-out clears the current device or the server has changed.
    async fn transport_request_as(
        &self,
        identity: &DeviceSnapshot,
        method: Method,
        path: &str,
        grant: Option<&str>,
        body: Option<Value>,
    ) -> Result<Value> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", identity.base))
            .bearer_auth(grant.unwrap_or(&identity.token))
            .header("X-Farsail-Generation", identity.generation.to_string());
        if grant.is_some() {
            request = request.header("X-Farsail-Device-Token", &identity.token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let start = Instant::now();
        let mut response = request.send().await?;
        let status = response.status();
        let mut bytes = Vec::new();
        const MAX: usize = 4096;
        if response.content_length().is_some_and(|n| n > MAX as u64) {
            return Err(Error::Invalid("transport response too large".into()));
        }
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX {
                return Err(Error::Invalid("transport response too large".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(Error::Http(
                status.as_u16(),
                status.canonical_reason().unwrap_or("request failed").into(),
            ));
        }
        let mut value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|_| Error::Invalid("invalid transport response".into()))?
        };
        // The coordinator supplies remaining TTL. Account for all local HTTP time,
        // including body receipt, before any transport lease uses that number.
        if let Some(ttl) = value.get_mut("expires_in")
            && let Some(n) = ttl.as_i64()
        {
            *ttl = json!(n.saturating_sub(start.elapsed().as_secs() as i64 + 1));
        }
        Ok(value)
    }

    pub async fn start_transport(self: &Arc<Self>, config: TransportConfig) -> Result<Value> {
        let (name, generation) = {
            let s = self.state.lock().await;
            let d = s.device.as_ref().ok_or(Error::Unbound)?;
            let generation = s
                .generation
                .ok_or_else(|| Error::Invalid("heartbeat required".into()))?;
            (
                format!(
                    "identity-{}-{}",
                    hex::encode(Sha256::digest(s.base.as_bytes())),
                    d.owner_id
                ),
                generation,
            )
        };
        let secret: [u8; 32] = self
            .store
            .read(&name)?
            .ok_or_else(|| Error::Store("device identity missing".into()))?
            .try_into()
            .map_err(|_| Error::Store("invalid device identity".into()))?;
        let epoch = self.stop_transport().await;
        let relay = config.relay.is_some();
        let transport = Transport::bind(secret, config)
            .await
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if relay {
            tokio::time::timeout(Duration::from_secs(10), transport.wait_online())
                .await
                .map_err(|_| Error::Invalid("relay did not become reachable".into()))?;
        }
        if let Err(error) = self
            .publish_transport_address(epoch, generation, &transport)
            .await
        {
            transport.close().await;
            return Err(error);
        }
        let active = {
            let current = self.transport_epoch.lock().await;
            if *current != epoch || self.signing_out.load(Ordering::SeqCst) {
                false
            } else {
                *self.transport.lock().await = Some(transport.clone());
                true
            }
        };
        if !active {
            transport.close().await;
            return Err(Error::Invalid("transport start cancelled".into()));
        }
        let endpoint_id = transport.id().to_string();
        let owner = self.clone();
        let watched = transport.clone();
        tokio::spawn(async move {
            use iroh::Watcher;
            let mut addresses = watched.watch_addr();
            let mut retry = tokio::time::interval(Duration::from_secs(30));
            retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            retry.tick().await;
            loop {
                tokio::select! {
                    _ = watched.wait_closed() => break,
                    changed = addresses.updated() => {
                        if changed.is_err() { break; }
                        // Coalesce address bursts and publish the current snapshot.
                        tokio::select! {
                            _ = watched.wait_closed() => break,
                            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                        }
                    }
                    _ = retry.tick() => {
                        let needs_direct = owner.transport_sessions.lock().await.values()
                            .any(|s| s.is_open_now() && s.path().0 == "relay");
                        if needs_direct { let _ = watched.refresh_discovery().await; }
                    }
                }
                if *owner.transport_epoch.lock().await != epoch {
                    break;
                }
                let generation = owner.state.lock().await.generation;
                if let Some(generation) = generation {
                    // Finish an in-flight upload under the publication lock.
                    // Cancelling its HTTP future does not cancel a server write.
                    let _ = owner
                        .publish_transport_address(epoch, generation, &watched)
                        .await;
                }
            }
        });
        let owner = self.clone();
        tokio::spawn(async move {
            let permits = Arc::new(tokio::sync::Semaphore::new(8));
            loop {
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    break;
                };
                match transport.next_incoming().await {
                    Ok(incoming) => {
                        let owner = owner.clone();
                        let transport = transport.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Ok(session) =
                                transport.accept_incoming(incoming, owner.clone()).await
                            {
                                owner.register_transport_session(epoch, session).await;
                            }
                        });
                    }
                    Err(farsail_transport::Error::Closed) => break,
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        });
        Ok(json!({"state":"connecting","endpoint_id":endpoint_id}))
    }

    pub async fn refresh_transport_address(&self) -> Result<()> {
        let (epoch, transport) = {
            let epoch = self.transport_epoch.lock().await;
            (*epoch, self.transport.lock().await.clone())
        };
        if let Some(transport) = transport {
            let generation = self.state.lock().await.generation.ok_or(Error::Unbound)?;
            self.publish_transport_address(epoch, generation, &transport)
                .await?;
        }
        Ok(())
    }
    async fn publish_transport_address(
        &self,
        epoch: u64,
        generation: i64,
        transport: &Transport,
    ) -> Result<()> {
        let _publication = self.transport_publication.lock().await;
        if *self.transport_epoch.lock().await != epoch
            || self.state.lock().await.generation != Some(generation)
            || self.signing_out.load(Ordering::SeqCst)
        {
            return Err(Error::Invalid("address publication cancelled".into()));
        }
        self.transport_request(
            Method::POST,
            "/v1/devices/endpoint-address",
            None,
            Some(json!({"generation":generation,"endpoint_addr":transport.addr()})),
        )
        .await?;
        Ok(())
    }
    pub async fn transport_discovery_state(&self, path: &str) -> &'static str {
        let transport = self.transport.lock().await.clone();
        match transport {
            Some(transport) => transport.discovery_state(path).await,
            None => "unavailable",
        }
    }

    pub async fn connect_transport(
        self: &Arc<Self>,
        id: &str,
        permission: RemotePermission,
    ) -> Result<Value> {
        let id = uuid(id)?;
        let (epoch, transport) = {
            let epoch = self.transport_epoch.lock().await;
            (
                *epoch,
                self.transport
                    .lock()
                    .await
                    .clone()
                    .ok_or_else(|| Error::Invalid("start transport first".into()))?,
            )
        };
        let _ = transport.refresh_discovery().await;
        // New discoveries are published by the watcher. Keep discovery and
        // address uploads out of the authorization handshake's critical path.
        let peer = self
            .transport_request(
                Method::GET,
                &format!("/v1/remote/{id}/peer-address"),
                None,
                None,
            )
            .await?;
        let addr: EndpointAddr = serde_json::from_value(peer["endpoint_addr"].clone())
            .map_err(|_| Error::Invalid("invalid peer address".into()))?;
        if *self.transport_epoch.lock().await != epoch {
            return Err(Error::Invalid("transport connection cancelled".into()));
        }
        let session = transport
            .connect(addr, &id, permission, self.clone())
            .await
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let (path, rtt) = session.path();
        if !self.register_transport_session(epoch, session).await {
            return Err(Error::Invalid(
                "transport connection cancelled or limit reached".into(),
            ));
        }
        Ok(json!({"id":id,"state":path,"rtt_ms":rtt}))
    }

    pub async fn transport_status(&self, id: &str) -> Result<Value> {
        let id = uuid(id)?;
        let session = self.transport_sessions.lock().await.get(&id).cloned();
        if let Some(s) = session.as_ref()
            && s.is_open().await
        {
            let (path, rtt) = s.path();
            let discovery = self.transport_discovery_state(path).await;
            return Ok(
                json!({"id":id,"state":path,"rtt_ms":rtt,"permission":s.permission(),"discovery":discovery}),
            );
        }
        let mut sessions = self.transport_sessions.lock().await;
        if let Some(old) = session
            && sessions
                .get(&id)
                .is_some_and(|s| s.stable_id() == old.stable_id())
        {
            sessions.remove(&id);
            drop(sessions);
            self.invalidate_transport_grant(&id).await;
            return Ok(json!({"id":id,"state":"closed","rtt_ms":null}));
        }
        Ok(json!({"id":id,"state":"closed","rtt_ms":null}))
    }
    pub async fn transport_session(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .transport_sessions
            .lock()
            .await
            .get(&uuid(id)?)
            .cloned())
    }
    pub async fn close_transport_session(&self, id: &str) -> Result<()> {
        let id = uuid(id)?;
        if let Some(s) = self.transport_sessions.lock().await.remove(&id) {
            s.close();
            self.invalidate_transport_grant(&id).await;
        }
        Ok(())
    }
    async fn register_transport_session(self: &Arc<Self>, epoch: u64, session: Session) -> bool {
        let local = self.transport.lock().await.as_ref().map(Transport::id);
        let Some(local) = local else {
            session.close();
            return false;
        };
        let is_host = session.is_host(local);
        if is_host && !self.hosting_enabled() {
            session.close();
            return false;
        }
        let current = self.transport_epoch.lock().await;
        if *current != epoch || self.signing_out.load(Ordering::SeqCst) {
            let id = session.id().to_owned();
            session.close();
            drop(current);
            self.invalidate_transport_grant(&id).await;
            return false;
        }
        let mut sessions = self.transport_sessions.lock().await;
        // Expired entries are removed on close by the task below; cap live sessions.
        if sessions.len() >= 16 || sessions.contains_key(session.id()) {
            let id = session.id().to_owned();
            session.close();
            drop(sessions);
            drop(current);
            self.invalidate_transport_grant(&id).await;
            return false;
        }
        let id = session.id().to_owned();
        let connection_id = session.stable_id();
        sessions.insert(id.clone(), session.clone());
        drop(sessions);
        drop(current);
        let _ = self.session_tx.send((session.clone(), is_host));
        let owner = self.clone();
        tokio::spawn(async move {
            session.wait_closed().await;
            let mut sessions = owner.transport_sessions.lock().await;
            if sessions
                .get(&id)
                .is_some_and(|s| s.stable_id() == connection_id)
            {
                sessions.remove(&id);
                drop(sessions);
                owner.invalidate_transport_grant(&id).await;
            }
        });
        true
    }
    pub async fn stop_transport(&self) -> u64 {
        let was_host = self.hosting_enabled();
        self.disable_host_local();
        let mut epoch = self.transport_epoch.lock().await;
        *epoch = epoch.wrapping_add(1);
        let next = *epoch;
        let sessions = std::mem::take(&mut *self.transport_sessions.lock().await);
        let transport = self.transport.lock().await.take();
        drop(epoch);
        let mut established = Vec::new();
        for (id, session) in sessions {
            session.close();
            established.push(id);
        }
        for id in established {
            self.invalidate_transport_grant(&id).await;
        }
        if let Some(transport) = transport {
            tokio::spawn(async move {
                transport.close().await;
            });
        }
        if was_host {
            let _ =
                tokio::time::timeout(Duration::from_secs(2), self.set_host_capability(false)).await;
        }
        next
    }
    async fn invalidate_transport_grant(&self, id: &str) {
        let (base, credential) = {
            let mut state = self.state.lock().await;
            let target = state.grant.remove(id).is_some();
            let bearer = if target {
                state.device.as_ref().map(|d| d.device_token.clone())
            } else {
                state.login.as_ref().map(|l| l.access_token.clone())
            };
            (state.base.clone(), bearer)
        };
        if let Some(credential) = credential {
            let http = self.http.clone();
            let path = format!("{base}/v1/remote/{id}/revoke");
            tokio::spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    http.post(path).bearer_auth(credential).send(),
                )
                .await;
            });
        }
    }
}

impl Authority for NativeClient {
    async fn abandon(&self, id: &str) {
        self.invalidate_transport_grant(id).await;
    }
    async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
        let checked_at = Instant::now();
        let v = self
            .transport_request(
                Method::GET,
                &format!("/v1/remote/{id}/transport-grant"),
                Some(token),
                None,
            )
            .await
            .map_err(|_| farsail_transport::Error::Denied)?;
        let mut claims: Claims =
            serde_json::from_value(v).map_err(|_| farsail_transport::Error::Denied)?;
        claims.checked_at = checked_at;
        Ok(claims)
    }
    async fn issue(&self, id: &str, renew: bool) -> farsail_transport::Result<String> {
        if !self.hosting_enabled() {
            return Err(farsail_transport::Error::Denied);
        }
        if !renew {
            if let Some(token) = self.state.lock().await.grant.get(id).cloned() {
                return Ok(token);
            }
            return Err(farsail_transport::Error::Denied);
        }
        let epoch = *self.transport_epoch.lock().await;
        let v = self
            .transport_request(
                Method::POST,
                &format!("/v1/remote/{id}/transport-renew"),
                None,
                None,
            )
            .await
            .map_err(|_| farsail_transport::Error::Denied)?;
        let token = v["grant_token"]
            .as_str()
            .ok_or(farsail_transport::Error::Denied)?
            .to_owned();
        let current = self.transport_epoch.lock().await;
        if *current != epoch {
            return Err(farsail_transport::Error::Closed);
        }
        let mut state = self.state.lock().await;
        if !state.grant.contains_key(id) {
            return Err(farsail_transport::Error::Closed);
        }
        state.grant.insert(id.to_owned(), token.clone());
        Ok(token)
    }
}

async fn page_user(
    client: &NativeClient,
    s: &mut State,
    path: &str,
    after: Option<&str>,
) -> Result<Value> {
    let path = match after {
        Some(x) => format!("{path}?limit=100&after={}", uuid(x)?),
        None => format!("{path}?limit=100"),
    };
    client.user(s, Method::GET, &path, None, false).await
}
fn uuid(s: &str) -> Result<String> {
    uuid::Uuid::parse_str(s)
        .map(|x| x.to_string())
        .map_err(|_| Error::Invalid("invalid ID".into()))
}
fn read_json<T: DeserializeOwned>(store: &dyn SecureStore, key: &str) -> Result<Option<T>> {
    store
        .read(key)?
        .map(|x| serde_json::from_slice(&x).map_err(|e| Error::Store(e.to_string())))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct MemoryStore(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);
    impl SecureStore for MemoryStore {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn write(&self, key: &str, value: &[u8]) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<()> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }
    #[test]
    fn server_policy() {
        assert!(validate_base("http://127.0.0.1:8787").is_ok());
        assert!(validate_base("http://[::1]:8787").is_ok());
        assert!(validate_base("https://203.0.113.10:443").is_ok());
        assert!(validate_base("http://203.0.113.10").is_err());
        assert!(validate_base("https://user:pass@example.com").is_err());
        assert!(validate_base("https://example.com/path").is_err());
    }
    #[tokio::test]
    async fn late_remote_watch_approval_is_revoked_after_opt_out() {
        use axum::{
            Json, Router,
            routing::{get, post},
        };
        let seen = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let revoked = Arc::new(AtomicBool::new(false));
        let id = "00000000-0000-0000-0000-000000000001";
        let app = Router::new()
            .route(
                "/v1/remote/pending",
                get(move || async move {
                    Json(json!([{"id":id,"requester_id":"owner","permission":"control"}]))
                }),
            )
            .route(
                &format!("/v1/remote/{id}/decide"),
                post({
                    let seen = seen.clone();
                    let release = release.clone();
                    move || {
                        let seen = seen.clone();
                        let release = release.clone();
                        async move {
                            seen.notify_one();
                            release.notified().await;
                            Json(json!({"grant_token":"synthetic"}))
                        }
                    }
                }),
            )
            .route(
                &format!("/v1/remote/{id}/revoke"),
                post({
                    let revoked = revoked.clone();
                    move || {
                        let revoked = revoked.clone();
                        async move {
                            revoked.store(true, Ordering::SeqCst);
                            Json(Value::Null)
                        }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        client
            .set_server(&format!("http://{}", listener.local_addr().unwrap()))
            .await
            .unwrap();
        assert!(client.set_remote_watch(true).is_err());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        {
            let mut s = client.state.lock().await;
            s.owner = Some("owner".into());
            s.device = Some(DeviceCredential {
                id: "target".into(),
                owner_id: "owner".into(),
                device_token: "synthetic".into(),
                session_id: "synthetic".into(),
            });
        }
        client.host_state.store(1, Ordering::SeqCst);
        client.set_remote_watch(true).unwrap();
        let approval = tokio::spawn({
            let client = client.clone();
            async move { client.approve_same_account_pending().await }
        });
        seen.notified().await;
        client.set_remote_watch(false).unwrap();
        release.notify_one();
        approval.await.unwrap().unwrap();
        assert!(revoked.load(Ordering::SeqCst));
        assert!(client.state.lock().await.grant.is_empty());
        client.set_remote_watch(true).unwrap();
        client.disable_host_local();
        assert_eq!(client.public_state().await["remoteWatch"], false);
        server.abort();
    }
    #[tokio::test]
    async fn delayed_address_upload_cannot_overwrite_replacement_or_new_generation() {
        use axum::{Json, Router, routing::post};
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let uploads = Arc::new(Mutex::new(Vec::<Value>::new()));
        let app = Router::new().route(
            "/v1/devices/endpoint-address",
            post({
                let entered = entered.clone();
                let release = release.clone();
                let uploads = uploads.clone();
                move |Json(body): Json<Value>| {
                    let entered = entered.clone();
                    let release = release.clone();
                    let uploads = uploads.clone();
                    async move {
                        if uploads.lock().await.is_empty() {
                            entered.notify_one();
                            release.notified().await;
                        }
                        uploads.lock().await.push(body);
                        Json(Value::Null)
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        client
            .set_server(&format!("http://{}", listener.local_addr().unwrap()))
            .await
            .unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        {
            let mut state = client.state.lock().await;
            state.device = Some(DeviceCredential {
                id: "device".into(),
                owner_id: "owner".into(),
                device_token: "synthetic".into(),
                session_id: "login".into(),
            });
            state.generation = Some(1);
            state
                .grant
                .insert("pending".into(), "synthetic-approved-grant".into());
        }
        let old = Transport::bind([99; 32], TransportConfig::default())
            .await
            .unwrap();
        *client.transport.lock().await = Some(old.clone());
        let upload = tokio::spawn({
            let client = client.clone();
            let old = old.clone();
            async move { client.publish_transport_address(0, 1, &old).await }
        });
        entered.notified().await;
        // A queued old snapshot is cancelled once its lifecycle changes.
        let queued_old = tokio::spawn({
            let client = client.clone();
            let old = old.clone();
            async move { client.publish_transport_address(0, 1, &old).await }
        });
        let epoch = client.stop_transport().await;
        let new = Transport::bind([99; 32], TransportConfig::default())
            .await
            .unwrap();
        let replacement = tokio::spawn({
            let client = client.clone();
            let new = new.clone();
            async move { client.publish_transport_address(epoch, 1, &new).await }
        });
        assert!(
            !replacement.is_finished(),
            "new upload waits for the old HTTP write"
        );
        release.notify_one();
        upload.await.unwrap().unwrap();
        assert!(queued_old.await.unwrap().is_err());
        replacement.await.unwrap().unwrap();
        let rows = uploads.lock().await;
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1]["endpoint_addr"],
            serde_json::to_value(new.addr()).unwrap()
        );
        assert_ne!(rows[0]["endpoint_addr"], rows[1]["endpoint_addr"]);
        drop(rows);
        client.state.lock().await.generation = Some(2);
        assert!(
            client
                .publish_transport_address(epoch, 1, &new)
                .await
                .is_err()
        );
        assert_eq!(
            uploads.lock().await.len(),
            2,
            "old generation never reaches HTTP"
        );
        assert!(
            client.state.lock().await.grant.contains_key("pending"),
            "unused approval survives discovery/lifecycle changes"
        );
        new.close().await;
        server.abort();
    }
    #[tokio::test]
    async fn delayed_transport_start_cannot_survive_stop() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut conn, _) = listener.accept().await.unwrap();
            let mut bytes = [0u8; 4096];
            let _ = conn.read(&mut bytes).await.unwrap();
            seen_tx.send(()).unwrap();
            release_rx.await.unwrap();
            conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let store = Arc::new(MemoryStore::default());
        let client = Arc::new(NativeClient::new(store.clone()).unwrap());
        client.set_server(&base).await.unwrap();
        let key = SigningKey::generate(&mut OsRng);
        let name = format!(
            "identity-{}-owner",
            hex::encode(Sha256::digest(base.as_bytes()))
        );
        store.write(&name, &key.to_bytes()).unwrap();
        {
            let mut s = client.state.lock().await;
            s.device = Some(DeviceCredential {
                id: "device".into(),
                device_token: "device-token".into(),
                owner_id: "owner".into(),
                session_id: "login".into(),
            });
            s.generation = Some(1);
        }
        let task = tokio::spawn({
            let client = client.clone();
            async move { client.start_transport(TransportConfig::default()).await }
        });
        seen_rx.await.unwrap();
        client.stop_transport().await;
        release_tx.send(()).unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(client.transport.lock().await.is_none());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn delayed_host_enable_cannot_survive_transport_stop() {
        use axum::{Json, Router, extract::State as AxumState, routing::post};
        use std::sync::atomic::AtomicUsize;
        struct Gate {
            entered: tokio::sync::Notify,
            release: tokio::sync::Notify,
            disabled: AtomicUsize,
        }
        async fn capability(
            AxumState(gate): AxumState<Arc<Gate>>,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            if body["can_host"] == true {
                gate.entered.notify_one();
                gate.release.notified().await;
            } else {
                gate.disabled.fetch_add(1, Ordering::SeqCst);
            }
            Json(json!({"can_host":body["can_host"]}))
        }
        let gate = Arc::new(Gate {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            disabled: AtomicUsize::new(0),
        });
        let app = Router::new()
            .route("/v1/devices/capability", post(capability))
            .with_state(gate.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        client.set_server(&base).await.unwrap();
        {
            let mut state = client.state.lock().await;
            state.device = Some(DeviceCredential {
                id: "device".into(),
                device_token: "token".into(),
                owner_id: "owner".into(),
                session_id: "session".into(),
            });
            state.generation = Some(1);
        }
        *client.transport.lock().await = Some(
            Transport::bind([33; 32], TransportConfig::default())
                .await
                .unwrap(),
        );
        let task = tokio::spawn({
            let client = client.clone();
            async move { client.set_host_capability(true).await }
        });
        gate.entered.notified().await;
        client.stop_transport().await;
        gate.release.notify_one();
        assert!(task.await.unwrap().is_err());
        assert!(!client.hosting_enabled());
        for _ in 0..50 {
            if gate.disabled.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            gate.disabled.load(Ordering::SeqCst) > 0,
            "late enable must be compensated on server"
        );
        server.abort();
    }
    struct TestAuthority {
        source: iroh::EndpointId,
        target: iroh::EndpointId,
    }
    impl Authority for TestAuthority {
        async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
            if token != "grant" {
                return Err(farsail_transport::Error::Denied);
            }
            Ok(Claims {
                session_id: id.into(),
                permission: RemotePermission::View,
                source_public_key: hex::encode(self.source.as_bytes()),
                target_public_key: hex::encode(self.target.as_bytes()),
                nonce: "ab".repeat(32),
                expires_in: 30,
                checked_at: Instant::now(),
            })
        }
        async fn issue(&self, _: &str, _: bool) -> farsail_transport::Result<String> {
            Ok("grant".into())
        }
    }
    #[tokio::test]
    async fn completed_dial_from_old_lifecycle_is_rejected() {
        let source = Transport::bind([11; 32], TransportConfig::default())
            .await
            .unwrap();
        let target = Transport::bind([12; 32], TransportConfig::default())
            .await
            .unwrap();
        let auth = Arc::new(TestAuthority {
            source: source.id(),
            target: target.id(),
        });
        let host = tokio::spawn({
            let target = target.clone();
            let auth = auth.clone();
            async move { target.accept(auth).await.unwrap() }
        });
        let session = source
            .connect(target.addr(), "old-session", RemotePermission::View, auth)
            .await
            .unwrap();
        let peer = host.await.unwrap();
        let owner = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        let old = *owner.transport_epoch.lock().await;
        owner.stop_transport().await;
        assert!(!owner.register_transport_session(old, session.clone()).await);
        assert!(!session.is_open().await);
        peer.close();
        source.close().await;
        target.close().await;
    }
    #[tokio::test]
    async fn slow_http_does_not_hold_local_sign_out() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut slow, _) = listener.accept().await.unwrap();
            let mut bytes = [0u8; 1024];
            let _ = slow.read(&mut bytes).await.unwrap();
            seen_tx.send(()).unwrap();
            let (mut logout, _) = listener.accept().await.unwrap();
            let _ = logout.read(&mut bytes).await.unwrap();
            logout
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let client = Arc::new(NativeClient::new(Arc::new(MemoryStore::default())).unwrap());
        client.set_server(&base).await.unwrap();
        {
            let mut s = client.state.lock().await;
            s.login = Some(Login {
                access_token: "test".into(),
                refresh_token: "refresh".into(),
                session_id: "login".into(),
                access_expires_in: 900,
            });
            s.expiry = Instant::now() + Duration::from_secs(900);
        }
        let pending = tokio::spawn({
            let client = client.clone();
            async move { client.call("me", Value::Null).await }
        });
        seen_rx.await.unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(3), client.call("logout", Value::Null)).await;
        assert!(result.unwrap().is_ok());
        assert!(!client.public_state().await["signedIn"].as_bool().unwrap());
        assert!(pending.await.unwrap().is_err());
        server.await.unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn credential_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = WindowsStore::new(dir.path().to_path_buf()).unwrap();
        store.write("login", b"sensitive").unwrap();
        assert_eq!(store.read("login").unwrap().unwrap(), b"sensitive");
        assert!(
            !fs::read(dir.path().join("login.bin"))
                .unwrap()
                .windows(9)
                .any(|x| x == b"sensitive")
        );
        store.write("login", b"replacement").unwrap();
        assert_eq!(store.read("login").unwrap().unwrap(), b"replacement");
        let path = dir.path().join("login.bin");
        let mut damaged = fs::read(&path).unwrap();
        damaged[0] ^= 0xff;
        fs::write(&path, damaged).unwrap();
        assert!(store.read("login").is_err());
        store.delete("login").unwrap();
        assert!(store.read("login").unwrap().is_none());
    }
    #[tokio::test]
    #[cfg(windows)]
    async fn redirect_is_not_followed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = destination.local_addr().unwrap();
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = source.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = source.accept().await.unwrap();
            let mut buf = [0; 1024];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{target}/steal\r\nContent-Length: 0\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(WindowsStore::new(dir.path().to_path_buf()).unwrap());
        let client = NativeClient::new(store).unwrap();
        client
            .set_server(&format!("http://{origin}"))
            .await
            .unwrap();
        assert!(matches!(
            client
                .call(
                    "register",
                    json!({"email":"a@b.test","password":"a long password"})
                )
                .await,
            Err(Error::Http(302, _))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), destination.accept())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    #[cfg(windows)]
    async fn offline_logout_clears_local_auth_and_reports_uncertainty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(WindowsStore::new(dir.path().to_path_buf()).unwrap());
        let client = NativeClient::new(store.clone()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        client
            .set_server(&format!("http://{}", listener.local_addr().unwrap()))
            .await
            .unwrap();
        drop(listener);
        let mut state = client.state.lock().await;
        state.login = Some(Login {
            access_token: "test".into(),
            refresh_token: "test".into(),
            session_id: "test".into(),
            access_expires_in: 900,
        });
        state.expiry = Instant::now() + Duration::from_secs(900);
        store
            .write(
                "login",
                &serde_json::to_vec(state.login.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
        drop(state);
        let error = client
            .call("logout", Value::Null)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("server revocation unconfirmed"));
        assert!(!client.public_state().await["signedIn"].as_bool().unwrap());
        assert!(store.read("login").unwrap().is_none());
    }
}
