//! Remember intent, never effective permission. Only native preflight may restore hosting.
use super::*;

const KEY: &str = "sharing-preferences";
#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    server: String,
    owner: String,
    device: String,
    session: String,
}
impl Scope {
    fn from_state(s: &State, verified: bool) -> Result<Self> {
        let login = s.login.as_ref().ok_or(Error::SignedOut)?;
        let device = s.device.as_ref().ok_or(Error::Unbound)?;
        if device.session_id != login.session_id
            || verified && s.owner.as_ref() != Some(&device.owner_id)
        {
            return Err(Error::Invalid("sharing identity is not verified".into()));
        }
        Ok(Self {
            server: s.base.clone(),
            owner: device.owner_id.clone(),
            device: device.id.clone(),
            session: login.session_id.clone(),
        })
    }
    fn valid(&self) -> bool {
        validate_base(&self.server).is_ok_and(|s| s == self.server)
            && [&self.owner, &self.device, &self.session]
                .iter()
                .all(|s| uuid::Uuid::parse_str(s).is_ok())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    scope: Scope,
    sharing: bool,
    watch: bool,
}
pub(super) struct Preferences {
    saved: Option<Saved>,
    revision: u64,
    watch_revision: u64,
    claimed: bool,
    status: &'static str,
}
impl Preferences {
    pub(super) fn load(store: &dyn SecureStore, state: &State) -> Self {
        // A damaged optional preference must not prevent starting the app/signing out.
        let saved = store
            .read(KEY)
            .ok()
            .flatten()
            .and_then(|bytes| {
                if bytes.len() > 4096 {
                    return None;
                }
                serde_json::from_slice::<Saved>(&bytes).ok()
            })
            .filter(|p| {
                p.version == 1
                    && p.scope.valid()
                    && (!p.watch || p.sharing)
                    && Scope::from_state(state, false).is_ok_and(|s| s == p.scope)
            });
        let status = if saved.as_ref().is_some_and(|p| p.sharing) {
            "pending"
        } else {
            "idle"
        };
        Self {
            saved,
            revision: 0,
            watch_revision: 0,
            claimed: false,
            status,
        }
    }
    pub(super) fn public(&self, s: &State) -> Value {
        let saved = self
            .saved
            .as_ref()
            .filter(|p| Scope::from_state(s, false).is_ok_and(|scope| scope == p.scope));
        json!({"sharing":saved.is_some_and(|p|p.sharing),
            "watch":saved.is_some_and(|p|p.watch),"restore":self.status})
    }
    fn persist(&self, store: &dyn SecureStore) -> Result<()> {
        match &self.saved {
            Some(saved) => store.write(KEY, &serde_json::to_vec(saved).unwrap()),
            None => store.delete(KEY),
        }
    }
    fn persist_opt_out(&self, store: &dyn SecureStore) -> Result<()> {
        let result = self.persist(store);
        // If replacing a record fails, removing the old opt-in is the safe fallback.
        if result.is_err() {
            let _ = store.delete(KEY);
        }
        result
    }
}

impl NativeClient {
    pub(super) fn preference_current(&self, revision: u64) -> bool {
        self.preferences.lock().unwrap().revision == revision
            && !self.signing_out.load(Ordering::SeqCst)
    }
    /// Explicit opt-out: invalidate activation synchronously before any network await.
    pub fn stop_sharing_preference(&self) -> Result<()> {
        let mut p = self.preferences.lock().unwrap();
        p.revision = p.revision.wrapping_add(1);
        p.watch_revision = p.watch_revision.wrapping_add(1);
        p.status = "idle";
        self.disable_host_local();
        if let Some(saved) = &mut p.saved {
            saved.sharing = false;
            saved.watch = false;
        }
        p.persist_opt_out(&*self.store)
    }
    pub fn stop_watch_preference(&self) -> Result<()> {
        let mut p = self.preferences.lock().unwrap();
        p.watch_revision = p.watch_revision.wrapping_add(1);
        self.set_remote_watch(false)?;
        if let Some(saved) = &mut p.saved {
            saved.watch = false;
        }
        p.persist_opt_out(&*self.store)
    }
    /// Explicit identity/logout boundary. Also usable while the account state lock is held.
    pub fn forget_sharing_preferences(&self) -> Result<()> {
        let mut p = self.preferences.lock().unwrap();
        p.revision = p.revision.wrapping_add(1);
        p.watch_revision = p.watch_revision.wrapping_add(1);
        p.saved = None;
        p.status = "idle";
        self.disable_host_local();
        p.persist(&*self.store)
    }
    /// Operational shutdown cancels pending restoration but keeps the deliberate choice.
    pub fn cancel_sharing_restore(&self) {
        let mut p = self.preferences.lock().unwrap();
        p.revision = p.revision.wrapping_add(1);
        p.claimed = true;
        p.status = "idle";
        self.disable_host_local();
    }
    pub async fn enable_watch_preference(&self) -> Result<()> {
        let (revision, watch_revision) = {
            let p = self.preferences.lock().unwrap();
            (p.revision, p.watch_revision)
        };
        let state = self.state.lock().await;
        let scope = Scope::from_state(&state, true)?;
        let mut p = self.preferences.lock().unwrap();
        if p.revision != revision
            || p.watch_revision != watch_revision
            || !p
                .saved
                .as_ref()
                .is_some_and(|s| s.scope == scope && s.sharing)
            || self.signing_out.load(Ordering::SeqCst)
        {
            return Err(Error::Invalid("remote watch start cancelled".into()));
        }
        self.set_remote_watch(true)?;
        p.saved.as_mut().unwrap().watch = true;
        if let Err(e) = p.persist(&*self.store) {
            p.saved.as_mut().unwrap().watch = false;
            self.set_remote_watch(false)?;
            return Err(e);
        }
        Ok(())
    }
    pub async fn enable_sharing_preference(
        &self,
        desktop: impl FnOnce() -> Result<()>,
    ) -> Result<Value> {
        let revision = self.preferences.lock().unwrap().revision;
        let scope = Scope::from_state(&*self.state.lock().await, true)?;
        if !self.transport_running().await {
            return Err(Error::Invalid("start the transport endpoint first".into()));
        }
        desktop()?;
        let result = self.set_host_capability_for_preference(revision).await?;
        let saved = {
            let state = self.state.lock().await;
            let mut p = self.preferences.lock().unwrap();
            if p.revision != revision
                || Scope::from_state(&state, true)? != scope
                || !self.hosting_enabled()
                || self.signing_out.load(Ordering::SeqCst)
            {
                return Err(Error::Invalid("sharing start cancelled".into()));
            }
            let watch = p
                .saved
                .as_ref()
                .is_some_and(|s| s.scope == scope && s.watch);
            let previous = p.saved.clone();
            p.saved = Some(Saved {
                version: 1,
                scope,
                sharing: true,
                watch,
            });
            p.status = "ready";
            let saved = p.persist(&*self.store);
            if saved.is_err() {
                p.saved = previous;
                p.status = "failed";
                self.disable_host_local();
            }
            if saved.is_ok() && watch {
                self.set_remote_watch(true)?;
            }
            saved
        };
        if let Err(e) = saved {
            let _ = self.set_host_capability(false).await;
            return Err(e);
        }
        Ok(result)
    }

    /// One startup attempt; normal HTTP/transport deadlines bound each step. No retry loop.
    /// The injected preflight is the same display/Capture check used by the native switch.
    pub async fn restore_sharing(
        self: &Arc<Self>,
        desktop: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let (revision, scope) = {
            let mut p = self.preferences.lock().unwrap();
            if p.claimed {
                return Ok(());
            }
            p.claimed = true;
            let Some(saved) = p.saved.as_ref().filter(|s| s.sharing) else {
                return Ok(());
            };
            let scope = saved.scope.clone();
            p.status = "pending";
            (p.revision, scope)
        };
        let result = self.restore_inner(revision, scope, desktop).await;
        if result.is_err() && self.preference_current(revision) {
            // Keep valid intent, but stop input/approval and best-effort clear advertisement.
            self.disable_host_local();
            let _ =
                tokio::time::timeout(Duration::from_secs(2), self.set_host_capability(false)).await;
            let mut p = self.preferences.lock().unwrap();
            if p.revision == revision {
                p.status = "failed";
            }
        }
        result
    }
    async fn restore_inner(
        self: &Arc<Self>,
        revision: u64,
        scope: Scope,
        desktop: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.call("resume", Value::Null).await?;
        {
            let s = self.state.lock().await;
            if !self.preference_current(revision) {
                return Err(Error::Invalid("sharing restore cancelled".into()));
            }
            if Scope::from_state(&s, true)? != scope {
                self.forget_sharing_preferences()?;
                return Err(Error::Invalid("sharing identity changed".into()));
            }
        }
        self.call("heartbeat", Value::Null).await?;
        if !self.preference_current(revision) {
            return Err(Error::Invalid("sharing restore cancelled".into()));
        }
        if !self.transport_running().await {
            let server = self.state.lock().await.base.clone();
            self.start_transport(default_transport_config(&server)?)
                .await?;
        }
        if !self.preference_current(revision) {
            return Err(Error::Invalid("sharing restore cancelled".into()));
        }
        desktop()?;
        self.set_host_capability_for_preference(revision).await?;
        let s = self.state.lock().await;
        let mut p = self.preferences.lock().unwrap();
        if p.revision != revision
            || Scope::from_state(&s, true)? != scope
            || !self.hosting_enabled()
            || self.signing_out.load(Ordering::SeqCst)
        {
            return Err(Error::Invalid("sharing restore cancelled".into()));
        }
        // Re-read watch intent under its mutation lock, so opting out during slow startup wins.
        if p.saved.as_ref().is_some_and(|s| s.watch) {
            self.set_remote_watch(true)?;
        }
        p.status = "ready";
        Ok(())
    }
}
