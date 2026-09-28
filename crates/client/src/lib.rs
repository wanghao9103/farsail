//! FarSail native account and device client. Credentials never cross the UI bridge.
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use reqwest::{Client as Http, Method};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use url::Url;

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
pub struct NativeClient {
    http: Http,
    store: Arc<dyn SecureStore>,
    state: Mutex<State>,
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
        Ok(Self {
            http: Http::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            store,
            state: Mutex::new(State {
                base,
                login,
                expiry: Instant::now(),
                owner: None,
                device,
                generation: None,
                grant: Default::default(),
            }),
        })
    }
    pub async fn public_state(&self) -> Value {
        let s = self.state.lock().await;
        json!({"server": s.base, "signedIn": s.login.is_some(), "deviceId": s.device.as_ref().map(|x| &x.id)})
    }
    pub async fn set_server(&self, base: &str) -> Result<Value> {
        let base = validate_base(base)?;
        let mut s = self.state.lock().await;
        if s.login.is_some() {
            return Err(Error::Invalid("sign out before changing server".into()));
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
        let mut response = r.send().await?;
        let status = response.status();
        const MAX_RESPONSE: usize = 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(Error::Invalid("server response too large".into()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
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
        s.login = None;
        s.owner = None;
        s.device = None;
        s.generation = None;
        s.grant.clear();
        self.store.delete("login")?;
        self.store.delete("device-token")
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
            s.device = None;
            s.generation = None;
            self.store.delete("device-token")?;
        }
        result
    }
    pub async fn call(&self, op: &str, args: Value) -> Result<Value> {
        if op == "devices" {
            return self.list_devices().await;
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
                    s.device = None;
                    s.generation = None;
                    self.store.delete("device-token")?;
                }
                Ok(me)
            }
            "logout" => {
                let result = self
                    .user(&mut s, Method::POST, "/v1/auth/logout", None, false)
                    .await;
                self.clear_auth(&mut s)?;
                result.map_err(|e| {
                    Error::Invalid(format!(
                        "local sign-out complete; server revocation unconfirmed: {e}"
                    ))
                })
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
    /// Consumed by the future authenticated transport; never registered as a Tauri command.
    pub async fn take_grant_for_transport(&self, remote_id: &str) -> Result<Option<String>> {
        let id = uuid(remote_id)?;
        Ok(self.state.lock().await.grant.remove(&id))
    }
    /// Target-side lease renewal for the future transport signaling path.
    pub async fn renew_grant_for_transport(&self, remote_id: &str) -> Result<String> {
        let id = uuid(remote_id)?;
        let mut s = self.state.lock().await;
        let v = self
            .device(
                &mut s,
                Method::POST,
                &format!("/v1/remote/{id}/renew"),
                None,
            )
            .await?;
        let token = v["grant_token"]
            .as_str()
            .ok_or_else(|| Error::Invalid("missing grant credential".into()))?
            .to_owned();
        s.grant.insert(id, token.clone());
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
    #[test]
    fn server_policy() {
        assert!(validate_base("http://127.0.0.1:8787").is_ok());
        assert!(validate_base("http://[::1]:8787").is_ok());
        assert!(validate_base("https://203.0.113.10:443").is_ok());
        assert!(validate_base("http://203.0.113.10").is_err());
        assert!(validate_base("https://user:pass@example.com").is_err());
        assert!(validate_base("https://example.com/path").is_err());
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
