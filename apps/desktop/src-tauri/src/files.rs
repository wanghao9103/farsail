use crate::viewer;
use farsail_client::{NativeClient, file_runtime::NativeFiles};
use std::{path::PathBuf, sync::Arc};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

// The webview may select a session and transfer ID, but never a filesystem path.
// Paths come exclusively from these native, user-controlled dialogs.
async fn pick(
    window: &tauri::WebviewWindow,
    name: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = window.app_handle().dialog().file().set_parent(window);
    if let Some(name) = name {
        dialog
            .set_title("保存接收的文件")
            .set_file_name(name)
            .save_file(move |path| {
                let _ = tx.send(path);
            });
    } else {
        dialog.set_title("选择要发送的文件").pick_file(move |path| {
            let _ = tx.send(path);
        });
    }
    rx.await
        .map_err(|_| "文件选择已取消。".to_owned())?
        .map(|path| {
            path.into_path()
                .map_err(|_| "请选择本机的普通文件。".to_owned())
        })
        .transpose()
}

#[tauri::command]
pub async fn files_enable(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    files: tauri::State<'_, Arc<NativeFiles>>,
    enabled: bool,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    if !enabled {
        client.disable_files_local();
        files.stop_hosts();
    }
    client
        .set_files_capability(enabled)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn files_status(
    window: tauri::WebviewWindow,
    files: tauri::State<'_, Arc<NativeFiles>>,
    id: Option<String>,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    files.status(id.as_deref())
}

#[tauri::command]
pub async fn files_send(
    window: tauri::WebviewWindow,
    files: tauri::State<'_, Arc<NativeFiles>>,
    id: String,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    if files.status(Some(&id))?["state"] != "connected" {
        return Err("文件连接已结束，请重新申请连接。".into());
    }
    if let Some(path) = pick(&window, None).await? {
        files.send_path(&id, path).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn files_accept(
    window: tauri::WebviewWindow,
    files: tauri::State<'_, Arc<NativeFiles>>,
    id: String,
    transfer_id: String,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    let offer = files.incoming_offer(&id, &transfer_id)?;
    if let Some(path) = pick(&window, Some(&offer.name)).await? {
        files.accept_path(&id, &transfer_id, path).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn files_reject(
    window: tauri::WebviewWindow,
    files: tauri::State<'_, Arc<NativeFiles>>,
    id: String,
    transfer_id: String,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    files.reject(&id, &transfer_id).await
}

#[tauri::command]
pub async fn files_cancel(
    window: tauri::WebviewWindow,
    files: tauri::State<'_, Arc<NativeFiles>>,
    id: String,
    transfer_id: String,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    files.cancel(&id, &transfer_id).await
}
