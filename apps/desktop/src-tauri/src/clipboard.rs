use crate::{remote::RemoteRuntime, viewer};
use farsail_windows::clipboard::MAX_FILES;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const MAGIC: &[u8; 4] = b"FSCP";
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Packet {
    Prepare {
        serial: u64,
        after: u64,
    },
    Paste {
        serial: u64,
        prepared: u64,
        ids: Vec<String>,
    },
    Result {
        serial: u64,
        error: Option<String>,
    },
}
impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend(serde_json::to_vec(self).expect("serializable clipboard packet"));
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4096 || !bytes.starts_with(MAGIC) {
            return Err("文件粘贴数据无效。".into());
        }
        let packet: Self = serde_json::from_slice(&bytes[4..]).map_err(|_| "文件粘贴数据无效。")?;
        if let Self::Paste { ids, .. } = &packet
            && (ids.is_empty()
                || ids.len() > MAX_FILES
                || ids
                    .iter()
                    .any(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
                || ids.iter().enumerate().any(|(i, id)| ids[..i].contains(id)))
        {
            return Err("文件粘贴列表无效。".into());
        }
        if let Self::Result {
            error: Some(error), ..
        } = &packet
            && error.len() > 512
        {
            return Err("文件粘贴数据无效。".into());
        }
        Ok(packet)
    }
    pub fn is_packet(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }
    pub fn serial(&self) -> u64 {
        match self {
            Self::Prepare { serial, .. }
            | Self::Paste { serial, .. }
            | Self::Result { serial, .. } => *serial,
        }
    }
}

fn validate_paths(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    if paths.len() > MAX_FILES {
        return Err("一次最多粘贴 32 个文件。".into());
    }
    for path in &paths {
        if !path.is_absolute() || !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
            return Err("请选择普通文件；暂不支持复制文件夹或链接。".into());
        }
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(farsail_client::files::valid_name)
        {
            return Err("文件名不受支持，请重命名后重试。".into());
        }
    }
    Ok(paths)
}

#[cfg(target_os = "linux")]
fn uri_paths(uris: &[gtk::glib::GString]) -> Result<Vec<PathBuf>, String> {
    if uris.len() > MAX_FILES {
        return Err("一次最多粘贴 32 个文件。".into());
    }
    uris.iter()
        .map(|uri| {
            url::Url::parse(uri)
                .ok()
                .and_then(|u| u.to_file_path().ok())
                .ok_or_else(|| "仅支持复制本机文件。".to_owned())
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn request_linux_files(
    clipboard: &gtk::Clipboard,
    callback: impl FnOnce(Result<Vec<PathBuf>, String>) + 'static,
) {
    clipboard.request_uris(move |clipboard, uris| {
        if !uris.is_empty() {
            callback(uri_paths(uris).and_then(validate_paths));
            return;
        }
        // Nautilus also uses the GNOME copied-files target, including a copy/cut
        // header. A remote paste always copies; it never removes local sources.
        clipboard.request_contents(
            &gdk::Atom::intern("x-special/gnome-copied-files"),
            move |_, selection| {
                let bytes = selection.data();
                if bytes.is_empty() {
                    callback(Ok(vec![]));
                    return;
                }
                if bytes.len() > 1024 * 1024 {
                    callback(Err("剪贴板文件列表过大。".into()));
                    return;
                }
                let result = std::str::from_utf8(&bytes)
                    .map_err(|_| "剪贴板文件列表无效。".to_owned())
                    .and_then(|text| {
                        let mut lines = text.trim_end_matches('\0').lines();
                        if !matches!(lines.next(), Some("copy" | "cut")) {
                            return Err("剪贴板文件列表无效。".into());
                        }
                        let uris: Vec<gtk::glib::GString> = lines
                            .filter(|line| !line.is_empty())
                            .map(Into::into)
                            .collect();
                        uri_paths(&uris).and_then(validate_paths)
                    });
                callback(result);
            },
        );
    });
}

async fn read_files(window: &tauri::WebviewWindow) -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        let _ = window;
        validate_paths(
            tokio::task::spawn_blocking(farsail_windows::clipboard::read_files)
                .await
                .map_err(|_| "无法读取文件剪贴板。")??,
        )
    }
    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .run_on_main_thread(move || {
                request_linux_files(
                    &gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD),
                    move |result| {
                        let _ = tx.send(result);
                    },
                );
            })
            .map_err(|_| "无法读取文件剪贴板。")?;
        let paths = tokio::time::timeout(std::time::Duration::from_secs(3), rx)
            .await
            .map_err(|_| "读取剪贴板超时，请重新复制文件。")?
            .map_err(|_| "无法读取文件剪贴板。")??;
        validate_paths(paths)
    }
}

#[tauri::command]
pub async fn remote_paste_files(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<bool, String> {
    viewer::scoped(&window, &id)?;
    remote.check_file_control(&id).await?;
    let paths = read_files(&window).await?;
    if paths.is_empty() {
        return Ok(false);
    }
    remote.paste_paths(&id, paths).await?;
    Ok(true)
}

#[tauri::command]
pub async fn remote_send_files(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<bool, String> {
    viewer::scoped(&window, &id)?;
    remote.check_file_control(&id).await?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("选择要粘贴到远程的文件")
        .pick_files(move |paths| {
            let _ = tx.send(paths);
        });
    let paths = rx.await.map_err(|_| "文件选择已取消。")?;
    if let Some(paths) = paths {
        let paths = paths
            .into_iter()
            .map(|p| p.into_path().map_err(|_| "请选择本机普通文件。".into()))
            .collect::<Result<Vec<_>, String>>()?;
        remote.paste_paths(&id, validate_paths(paths)?).await?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_sources_reject_directories_and_missing_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(validate_paths(vec![directory.path().to_owned()]).is_err());
        assert!(validate_paths(vec![directory.path().join("missing")]).is_err());
        let path = directory.path().join("正常文件.txt");
        std::fs::write(&path, b"clipboard test").unwrap();
        assert!(validate_paths(vec![path]).is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an isolated Xvfb clipboard; never run on the user's display"]
    fn native_linux_file_clipboard_roundtrip() {
        assert_eq!(
            std::env::var("FARSAIL_CLIPBOARD_TEST_ISOLATED").as_deref(),
            Ok("1")
        );
        gtk::init().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("本地复制 带空格.txt");
        std::fs::write(&path, b"native GTK clipboard").unwrap();
        let uri = url::Url::from_file_path(&path).unwrap().to_string();
        let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
        let standard_uri = uri.clone();
        assert!(clipboard.set_with_data(
            &[gtk::TargetEntry::new(
                "text/uri-list",
                gtk::TargetFlags::empty(),
                0
            )],
            move |_, selection, _| {
                assert!(selection.set_uris(&[&standard_uri]));
            }
        ));
        let result = std::rc::Rc::new(std::cell::RefCell::new(None));
        let observed = result.clone();
        request_linux_files(&clipboard, move |result| {
            *observed.borrow_mut() = Some(result);
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while result.borrow().is_none() && std::time::Instant::now() < deadline {
            gtk::glib::MainContext::default().iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            result.borrow_mut().take().unwrap().unwrap(),
            vec![path.clone()]
        );
        let target = gdk::Atom::intern("x-special/gnome-copied-files");
        assert!(clipboard.set_with_data(
            &[gtk::TargetEntry::new(
                "x-special/gnome-copied-files",
                gtk::TargetFlags::empty(),
                0
            )],
            move |_, selection, _| {
                selection.set(&target, 8, format!("copy\n{uri}").as_bytes());
            }
        ));
        let observed = result.clone();
        request_linux_files(&clipboard, move |response| {
            *observed.borrow_mut() = Some(response);
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while result.borrow().is_none() && std::time::Instant::now() < deadline {
            gtk::glib::MainContext::default().iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(result.borrow_mut().take().unwrap().unwrap(), vec![path]);
        assert!(
            uri_paths(&[gtk::glib::GString::from(
                "file://other-host/shared/file.txt"
            )])
            .is_err()
        );
        assert!(
            uri_paths(&[gtk::glib::GString::from("https://example.invalid/file.txt")]).is_err()
        );
        clipboard.clear();
    }
    #[test]
    fn clipboard_packets_reject_paths_duplicates_and_oversize() {
        assert!(
            Packet::decode(
                &Packet::Prepare {
                    serial: 1,
                    after: 0
                }
                .encode()
            )
            .is_ok()
        );
        assert!(
            Packet::decode(b"FSCP{\"kind\":\"paste\",\"serial\":1,\"ids\":[\"../../target\"]}")
                .is_err()
        );
        assert!(
            Packet::decode(b"FSCP{\"kind\":\"prepare\",\"serial\":1,\"path\":\"/tmp/target\"}")
                .is_err()
        );
        let id = "a".repeat(32);
        assert!(
            Packet::decode(
                &Packet::Paste {
                    serial: 2,
                    prepared: 1,
                    ids: vec![id.clone(), id]
                }
                .encode()
            )
            .is_err()
        );
        assert!(Packet::decode(&vec![0; 4097]).is_err());
    }
}
