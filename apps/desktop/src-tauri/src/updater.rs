use crate::{remote::RemoteRuntime, viewer};
use farsail_client::{NativeClient, file_runtime::NativeFiles};
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
    pub can_prompt: bool,
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
            Err(e) => {
                return Err(user_error(
                    "preferences read",
                    e,
                    "无法读取更新设置，请重试。",
                ));
            }
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
                can_prompt: false,
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
        let message = reason.to_string();
        self.phase("error", &message);
        message
    }
    pub fn preferences(&self, preferences: Preferences) -> Result<Status, String> {
        // Serialize preferences independently of network operations and persist before applying.
        let _writer = self.preference_write.lock().unwrap();
        let bytes = serde_json::to_vec(&preferences)
            .map_err(|e| user_error("preferences", e, "无法保存更新设置，请重试。"))?;
        let temporary = self.path.with_extension("json.new");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&temporary)
            .map_err(|e| user_error("preferences", e, "无法保存更新设置，请重试。"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| user_error("preferences", e, "无法保存更新设置，请重试。"))?;
        std::fs::rename(temporary, &self.path)
            .map_err(|e| user_error("preferences", e, "无法保存更新设置，请重试。"))?;
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
            // A custom target is an exact manifest key; never fall back to another architecture.
            let target = update_target(std::env::consts::OS, std::env::consts::ARCH)?;
            let builder = app
                .updater_builder()
                .target(target)
                .timeout(Duration::from_secs(20));
            let mut update = builder
                .build()
                .map_err(check_error)?
                .check()
                .await
                .map_err(check_error)?;
            if let Some(update) = update.as_mut() {
                validate_download(&update.download_url, &update.version, target)?;
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
        let bytes = result.map_err(|e| self.failed(download_error(e)))?;
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
        let files = app.state::<Arc<NativeFiles>>().inner().clone();
        if app.webview_windows().keys().any(|label| label != "main") || !files.begin_update() {
            self.phase("waiting", "更新已就绪，等待屏幕或文件连接结束后安装。");
            return Ok(self.snapshot());
        }
        if !remote.begin_update().await {
            files.finish_update();
            self.phase("waiting", "更新已就绪，等待屏幕或文件连接结束后安装。");
            return Ok(self.snapshot());
        }
        self.phase("installing", "正在安装更新，完成后将重新启动。");
        let client = app.state::<Arc<NativeClient>>().inner().clone();
        client.cancel_sharing_restore();
        files.stop_all();
        remote.stop_all().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), client.disable_remote_access()).await;
        client.stop_transport().await;
        let result = tokio::task::spawn_blocking(move || install_update(update, &bytes)).await;
        match result {
            Ok(Ok(())) => {
                app.restart();
            }
            Ok(Err(e)) => {
                remote.finish_update();
                files.finish_update();
                Err(self.failed(e))
            }
            Err(e) => {
                remote.finish_update();
                files.finish_update();
                Err(self.failed(user_error("installer task", e, "更新安装失败，请重试。")))
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
fn install_update(update: Update, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::{
            io::Write,
            os::unix::fs::MetadataExt,
            process::{Command, Stdio},
        };
        let helper = std::path::Path::new("/usr/libexec/farsail-update-helper");
        if helper.exists() {
            for path in helper.ancestors() {
                let meta = std::fs::symlink_metadata(path)
                    .map_err(|e| user_error("installer IO", e, "更新安装失败，请重试。"))?;
                if meta.uid() != 0 || meta.mode() & 0o022 != 0 || meta.file_type().is_symlink() {
                    return Err("更新安装失败，请重试。".into());
                }
            }
            let mut child = Command::new("/usr/bin/pkexec")
                .arg(helper)
                .arg("install")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| user_error("installer IO", e, "更新安装失败，请重试。"))?;
            let written = (|| -> std::io::Result<()> {
                let mut stdin = child.stdin.take().unwrap();
                let header = serde_json::json!({ "signature": update.signature });
                writeln!(stdin, "{header}")?;
                stdin.write_all(bytes)
            })();
            let output = child
                .wait_with_output()
                .map_err(|e| user_error("installer IO", e, "更新安装失败，请重试。"))?;
            if !output.status.success() {
                return Err(if matches!(output.status.code(), Some(126 | 127)) {
                    "更新授权未完成，请重试。".into()
                } else {
                    user_error(
                        "privileged installer",
                        String::from_utf8_lossy(&output.stderr),
                        "更新安装失败，请重试。",
                    )
                });
            }
            return written.map_err(|e| user_error("installer pipe", e, "更新安装失败，请重试。"));
        }
    }
    // Older installations have no helper yet. This signed update installs it via dpkg once.
    update.install(bytes).map_err(|e| {
        if matches!(e, tauri_plugin_updater::Error::AuthenticationFailed) {
            user_error("installer authorization", e, "更新授权未完成，请重试。")
        } else {
            user_error("installer", e, "更新安装失败，请重试。")
        }
    })
}
fn update_target(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("linux-x86_64-deb"),
        ("linux", "aarch64") => Ok("linux-aarch64-deb"),
        ("windows", "x86_64") => Ok("windows-x86_64-nsis"),
        _ => Err("当前系统架构暂不支持在线更新，请联系维护者。".into()),
    }
}
fn check_error(error: tauri_plugin_updater::Error) -> String {
    eprintln!("FarSail update check: {error}");
    match error {
        tauri_plugin_updater::Error::TargetNotFound(_)
        | tauri_plugin_updater::Error::TargetsNotFound(_) => {
            "更新通道暂无适用于当前系统架构的安装包，请稍后再检查。".into()
        }
        _ => "暂时无法检查更新，请稍后重试。".into(),
    }
}
fn user_error(operation: &str, error: impl std::fmt::Display, message: &str) -> String {
    eprintln!("FarSail update {operation}: {error}");
    message.into()
}
fn download_error(error: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    let message = match &error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion
        | Error::InvalidUpdaterFormat => "更新包校验失败，请重新下载。",
        _ => "下载中断，请重试。",
    };
    user_error("download", error, message)
}
fn validate_download(url: &tauri::Url, version: &str, target: &str) -> Result<(), String> {
    let suffix = match target {
        "linux-x86_64-deb" => "amd64.deb",
        "linux-aarch64-deb" => "arm64.deb",
        "windows-x86_64-nsis" => "x64-setup.exe",
        _ => return Err("当前系统架构暂不支持在线更新，请联系维护者。".into()),
    };
    let expected = format!(
        "/wanghao9103/farsail/releases/download/client-v{version}/FarSail_{version}_{suffix}"
    );
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|p| p != 443)
        || url.path() != expected
    {
        return Err("更新来源无效，请联系维护者。".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn update_status(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, Arc<Runtime>>,
) -> Result<Status, String> {
    viewer::main_only(&window)?;
    let mut status = runtime.snapshot();
    let app = window.app_handle();
    let remote = app.state::<Arc<RemoteRuntime>>();
    let files = app.state::<Arc<NativeFiles>>();
    status.can_prompt = !app.webview_windows().keys().any(|label| label != "main")
        && !remote.has_active_sessions().await
        && !files.has_active_sessions();
    Ok(status)
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
    fn public_errors_do_not_include_internal_diagnostics() {
        use tauri_plugin_updater::Error;
        let diagnostic = "reqwest /tmp/private/updater.rs:123 https://internal.invalid";
        assert_eq!(
            check_error(Error::Network(diagnostic.into())),
            "暂时无法检查更新，请稍后重试。"
        );
        assert_eq!(
            download_error(Error::Network(diagnostic.into())),
            "下载中断，请重试。"
        );
        assert_eq!(
            download_error(Error::SignedVersionMismatch {
                signed: diagnostic.into(),
                announced: "99.0.0".into()
            }),
            "更新包校验失败，请重新下载。"
        );
    }
    #[test]
    fn preferences_write_failure_is_friendly_and_does_not_change_the_setting() {
        let path = std::env::temp_dir().join(format!(
            "farsail-missing-update-dir-{}/child/updates.json",
            std::process::id()
        ));
        let runtime = Runtime::new(path, "0.1.24".into()).unwrap();
        let message = runtime
            .preferences(Preferences {
                auto_check: false,
                auto_install: true,
            })
            .err()
            .unwrap();
        assert_eq!(message, "无法保存更新设置，请重试。");
        assert!(runtime.snapshot().preferences.auto_check);
        assert!(!runtime.snapshot().preferences.auto_install);
    }
    #[test]
    fn update_source_must_match_repository_and_signed_version() {
        let url = "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.21/FarSail_0.1.21_amd64.deb";
        assert!(validate_download(&url.parse().unwrap(), "0.1.21", "linux-x86_64-deb").is_ok());
        for invalid in [
            url.replace("https:", "http:"),
            url.replace("github.com", "github.com.example.org"),
            url.replace("wanghao9103", "another-owner"),
            url.replace("client-v0.1.21", "client-v0.1.20"),
            format!("{url}?redirect=elsewhere"),
            url.replace("github.com", "user@github.com"),
        ] {
            assert!(
                validate_download(&invalid.parse().unwrap(), "0.1.21", "linux-x86_64-deb").is_err()
            );
        }
    }
    #[test]
    fn architecture_selects_exact_target_and_rejects_cross_architecture_installers() {
        let targets = [
            ("linux", "x86_64", "linux-x86_64-deb", "amd64.deb"),
            ("linux", "aarch64", "linux-aarch64-deb", "arm64.deb"),
            ("windows", "x86_64", "windows-x86_64-nsis", "x64-setup.exe"),
        ];
        for (os, arch, target, suffix) in targets {
            assert_eq!(update_target(os, arch).unwrap(), target);
            for (_, _, _, candidate_suffix) in targets {
                let url = format!(
                    "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.21/FarSail_0.1.21_{candidate_suffix}"
                );
                assert_eq!(
                    validate_download(&url.parse().unwrap(), "0.1.21", target).is_ok(),
                    candidate_suffix == suffix
                );
            }
        }
        for (os, arch) in [
            ("linux", "arm"),
            ("windows", "aarch64"),
            ("macos", "aarch64"),
        ] {
            assert!(update_target(os, arch).is_err());
        }
    }
    #[test]
    fn missing_arm64_entry_does_not_use_the_existing_x86_64_entry() {
        let release: tauri_plugin_updater::RemoteRelease = serde_json::from_value(serde_json::json!({
            "version": "0.1.20",
            "platforms": {
                "linux-x86_64-deb": {
                    "url": "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.20/FarSail_0.1.20_amd64.deb",
                    "signature": "test signature"
                },
                "windows-x86_64-nsis": {
                    "url": "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.20/FarSail_0.1.20_x64-setup.exe",
                    "signature": "test signature"
                }
            }
        })).unwrap();
        let target = update_target("linux", "aarch64").unwrap();
        let error = release.download_url(target).unwrap_err();
        assert!(matches!(
            error,
            tauri_plugin_updater::Error::TargetNotFound(_)
        ));
        assert_eq!(
            check_error(error),
            "更新通道暂无适用于当前系统架构的安装包，请稍后再检查。"
        );
        assert!(release.signature(target).is_err());
        assert!(release.download_url("linux-x86_64-deb").is_ok());
        assert!(release.download_url("windows-x86_64-nsis").is_ok());
    }
    #[test]
    fn architecture_url_must_be_the_exact_installer_name() {
        let url = "https://github.com/wanghao9103/farsail/releases/download/client-v0.1.21/FarSail_0.1.21_arm64.deb";
        for invalid in [
            url.replace("FarSail_0.1.21_", "FarSail_0.1.20_"),
            format!("{url}/extra"),
            format!("{url}.sig"),
            url.replace("arm64.deb", "aarch64.deb"),
            url.replace("arm64.deb", "amd64.deb"),
            url.replace("FarSail_", "%46arSail_"),
        ] {
            assert!(
                validate_download(&invalid.parse().unwrap(), "0.1.21", "linux-aarch64-deb")
                    .is_err()
            );
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
