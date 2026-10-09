use crate::{remote::RemoteRuntime, viewer};
use farsail_client::NativeClient;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub auto_check: bool,
    pub auto_install: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            auto_check: true,
            auto_install: false,
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub current_version: String,
    pub available_version: Option<String>,
    pub phase: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: String,
    pub preferences: Preferences,
}
struct Pending {
    update: Update,
    bytes: Option<Arc<Vec<u8>>>,
}
pub struct Runtime {
    path: PathBuf,
    status: Mutex<Status>,
    pending: Mutex<Option<Pending>>,
    operation: tokio::sync::Mutex<()>,
    preference_write: Mutex<()>,
}
impl Runtime {
    pub fn new(path: PathBuf, version: String) -> Result<Arc<Self>, String> {
        let preferences = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Preferences {
                auto_check: false,
                auto_install: false,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
            Err(e) => return Err(format!("更新设置读取失败：{e}")),
        };
        Ok(Arc::new(Self {
            path,
            status: Mutex::new(Status {
                current_version: version,
                available_version: None,
                phase: "idle".into(),
                downloaded: 0,
                total: None,
                message: if preferences.auto_check {
                    "将自动检查新版，也可手动检查更新。"
                } else {
                    "自动检查已关闭，可手动检查更新。"
                }
                .into(),
                preferences,
            }),
            pending: Mutex::new(None),
            operation: tokio::sync::Mutex::new(()),
            preference_write: Mutex::new(()),
        }))
    }
    pub fn snapshot(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    fn phase(&self, phase: &str, message: &str) {
        let mut status = self.status.lock().unwrap();
        status.phase = phase.into();
        status.message = message.into();
    }
    fn failed(&self, reason: impl std::fmt::Display) -> String {
        let message = format!("更新未完成，当前版本仍可使用。{reason}");
        self.phase("error", &message);
        message
    }
    pub fn preferences(&self, preferences: Preferences) -> Result<Status, String> {
        // Serialize preferences independently of network operations and persist before applying.
        let _writer = self.preference_write.lock().unwrap();
        let bytes = serde_json::to_vec(&preferences).map_err(|e| e.to_string())?;
        let temporary = self.path.with_extension("json.new");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, &self.path).map_err(|e| e.to_string())?;
        self.status.lock().unwrap().preferences = preferences;
        Ok(self.snapshot())
    }
    pub async fn check(&self, app: &AppHandle) -> Result<Status, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "更新任务正在进行，请稍候。")?;
        self.phase("checking", "正在检查更新…");
        let result = async {
            let builder = app.updater_builder().timeout(Duration::from_secs(20));
            // Explicit Windows target preserves the established byte-identical installer verification.
            #[cfg(windows)]
            let builder = builder.target("windows-x86_64-nsis");
            let mut update = builder
                .build()
                .map_err(|e| e.to_string())?
                .check()
                .await
                .map_err(|e| e.to_string())?;
            if let Some(update) = update.as_mut() {
                validate_download(&update.download_url, &update.version)?;
                update.timeout = Some(Duration::from_secs(300));
            }
            Ok::<_, String>(update)
        }
        .await;
        match result {
            Ok(Some(update)) => {
                let mut pending = self.pending.lock().unwrap();
                // A repeat check of the same release preserves an already verified download.
                let bytes = pending
                    .as_ref()
                    .filter(|p| p.update.version == update.version)
                    .and_then(|p| p.bytes.clone());
                let mut status = self.status.lock().unwrap();
                status.available_version = Some(update.version.clone());
                status.phase = if bytes.is_some() {
                    "ready"
                } else {
                    "available"
                }
                .into();
                status.message = format!("发现新版本 {}。", update.version);
                if bytes.is_none() {
                    status.downloaded = 0;
                    status.total = None;
                }
                *pending = Some(Pending { update, bytes });
            }
            Ok(None) => {
                *self.pending.lock().unwrap() = None;
                let mut status = self.status.lock().unwrap();
                status.available_version = None;
                status.phase = "upToDate".into();
                status.message = "当前已是最新版本。".into();
                status.downloaded = 0;
                status.total = None;
            }
            Err(e) => {
                return Err(self.failed(e));
            }
        }
        Ok(self.snapshot())
    }
    pub async fn download(&self) -> Result<Status, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "更新任务正在进行，请稍候。")?;
        let update = {
            let pending = self.pending.lock().unwrap();
            let pending = pending.as_ref().ok_or("请先检查更新。")?;
            if pending.bytes.is_some() {
                self.phase("ready", "更新已下载并通过签名校验，可以安装。");
                return Ok(self.snapshot());
            }
            pending.update.clone()
        };
        self.phase("downloading", "正在下载并验证更新…");
        {
            let mut status = self.status.lock().unwrap();
            status.downloaded = 0;
            status.total = None;
        }
        let result = update
            .download(
                |chunk, total| {
                    let mut status = self.status.lock().unwrap();
                    status.downloaded += chunk as u64;
                    status.total = total;
                },
                || {},
            )
            .await;
        let bytes = result.map_err(|e| self.failed(e))?;
        self.pending
            .lock()
            .unwrap()
            .as_mut()
            .ok_or("更新任务已取消。")?
            .bytes = Some(Arc::new(bytes));
        self.phase("ready", "更新已下载并通过签名校验，可以安装。");
        Ok(self.snapshot())
    }
    pub async fn install(&self, app: &AppHandle) -> Result<Status, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "更新任务正在进行，请稍候。")?;
        let (update, bytes) = {
            let pending = self.pending.lock().unwrap();
            let pending = pending.as_ref().ok_or("请先检查更新。")?;
            (
                pending.update.clone(),
                pending.bytes.clone().ok_or("请先下载更新。")?,
            )
        };
        let remote = app.state::<Arc<RemoteRuntime>>().inner().clone();
        if app.webview_windows().keys().any(|label| label != "main") || !remote.begin_update().await
        {
            self.phase(
                "waiting",
                "更新已就绪，等待远程连接结束。请关闭远程窗口后安装。",
            );
            return Ok(self.snapshot());
        }
        self.phase(
            "installing",
            "正在安装更新，完成后将重新启动。Ubuntu 可能需要系统授权。",
        );
        let client = app.state::<Arc<NativeClient>>().inner().clone();
        client.cancel_sharing_restore();
        client.disable_host_local();
        remote.stop_all().await;
        let _ =
            tokio::time::timeout(Duration::from_secs(2), client.set_host_capability(false)).await;
        client.stop_transport().await;
        let result = tokio::task::spawn_blocking(move || update.install(bytes.as_slice())).await;
        match result {
            Ok(Ok(())) => {
                app.restart();
            }
            Ok(Err(e)) => {
                remote.finish_update();
                Err(self.failed(format!("{e} 请重新开启共享或连接后重试。")))
            }
            Err(e) => {
                remote.finish_update();
                Err(self.failed(e))
            }
        }
    }
    #[cfg(not(debug_assertions))]
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        use std::time::Instant;
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut checked: Option<Instant> = None;
            tokio::time::sleep(Duration::from_secs(10)).await;
            loop {
                let status = runtime.snapshot();
                if status.preferences.auto_check
                    && checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(3600))
                {
                    checked = Some(Instant::now());
                    let _ = runtime.check(&app).await;
                }
                let status = runtime.snapshot();
                if status.preferences.auto_install {
                    if status.phase == "available" {
                        let _ = runtime.download().await;
                    }
                    if matches!(runtime.snapshot().phase.as_str(), "ready" | "waiting") {
                        let _ = runtime.install(&app).await;
                    }
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }
}
fn validate_download(url: &tauri::Url, version: &str) -> Result<(), String> {
    let expected = format!(
        "/wanghao9103/farsail/releases/download/client-v{}/",
        version
    );
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|p| p != 443)
        || !url.path().starts_with(&expected)
    {
        return Err("更新来源无效，请联系维护者。".into());
    }
    Ok(())
}
#[tauri::command]
pub fn update_status(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    Ok(runtime.snapshot())
}
#[tauri::command]
pub fn update_preferences(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
    preferences: Preferences,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    runtime.preferences(preferences)
}
#[tauri::command]
pub async fn update_check(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    runtime.check(window.app_handle()).await
}
#[tauri::command]
pub async fn update_download(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    runtime.download().await
}
#[tauri::command]
pub async fn update_install(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    runtime.install(window.app_handle()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_source_must_match_repository_and_signed_version() {
        let url = "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.21/FarSail_0.1.21_amd64.deb";
        assert!(validate_download(&url.parse().unwrap(), "0.1.21").is_ok());
        for invalid in [
            url.replace("https:", "http:"),
            url.replace("github.com", "github.com.example.org"),
            url.replace("wanghao9103", "another-owner"),
            url.replace("client-v0.1.21", "client-v0.1.20"),
            format!("{url}?redirect=elsewhere"),
            url.replace("github.com", "user@github.com"),
        ] {
            assert!(validate_download(&invalid.parse().unwrap(), "0.1.21").is_err());
        }
    }
    #[test]
    fn settings_survive_restart_and_corruption_disables_automation() {
        let directory =
            std::env::temp_dir().join(format!("farsail-update-settings-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("updates.json");
        let runtime = Runtime::new(path.clone(), "0.1.20".into()).unwrap();
        assert!(runtime.snapshot().preferences.auto_check);
        assert!(!runtime.snapshot().preferences.auto_install);
        runtime
            .preferences(Preferences {
                auto_check: true,
                auto_install: true,
            })
            .unwrap();
        assert!(
            Runtime::new(path.clone(), "0.1.21".into())
                .unwrap()
                .snapshot()
                .preferences
                .auto_install
        );
        std::fs::write(&path, "broken").unwrap();
        let recovered = Runtime::new(path, "0.1.21".into()).unwrap().snapshot();
        assert!(!recovered.preferences.auto_check);
        assert!(!recovered.preferences.auto_install);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
