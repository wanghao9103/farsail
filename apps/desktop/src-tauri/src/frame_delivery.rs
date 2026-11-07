//! Window-scoped, read-only frame delivery. Other runtimes keep binary IPC.
#[cfg(windows)]
pub async fn post(
    window: tauri::WebviewWindow,
    bytes: Vec<u8>,
    id: String,
    request: u64,
) -> Result<&'static str, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, ICoreWebView2_17, ICoreWebView2Environment12,
    };
    use windows::core::{HSTRING, Interface};
    if bytes.is_empty() {
        return Ok("empty");
    }
    if bytes.len() > farsail_media::MAX_BYTES + 49 {
        return Err("invalid frame size".into());
    }
    let metadata =
        serde_json::json!({"kind":"farsail-frame","id":id,"request":request}).to_string();
    let (done, completed) = tokio::sync::oneshot::channel();
    let owner = window.clone();
    window
        .with_webview(move |platform| {
            // A tab may have moved while this callback waited on the UI thread.
            if let Err(error) = crate::viewer::scoped(&owner, &id) {
                let _ = done.send(Err(error));
                return;
            }
            // All COM objects and pointers stay on the WebView's UI thread.
            let sent = (|| -> Result<(), String> {
                let environment = platform
                    .environment()
                    .cast::<ICoreWebView2Environment12>()
                    .map_err(|_| "unsupported")?;
                let core = unsafe { platform.controller().CoreWebView2() }
                    .map_err(|_| "unsupported")?
                    .cast::<ICoreWebView2_17>()
                    .map_err(|_| "unsupported")?;
                let buffer = unsafe { environment.CreateSharedBuffer(bytes.len() as u64) }
                    .map_err(|_| "unsupported")?;
                let result = (|| -> Result<(), String> {
                    let mut address = std::ptr::null_mut();
                    unsafe { buffer.Buffer(&mut address) }.map_err(|_| "unsupported")?;
                    if address.is_null() {
                        return Err("unsupported".into());
                    }
                    // SAFETY: CreateSharedBuffer allocated exactly bytes.len() bytes;
                    // the pointer is non-null and remains mapped until Close below.
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), address, bytes.len());
                    }
                    unsafe {
                        core.PostSharedBufferToScript(
                            &buffer,
                            COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY,
                            &HSTRING::from(metadata),
                        )
                    }
                    .map_err(|_| "unsupported".into())
                })();
                // Script owns a separate mapping until chrome.webview.releaseBuffer.
                let _ = unsafe { buffer.Close() };
                result
            })();
            let _ = done.send(Ok(sent.is_ok()));
        })
        .map_err(|e| e.to_string())?;
    Ok(if completed.await.unwrap_or(Ok(false))? {
        "shared"
    } else {
        "unsupported"
    })
}

#[cfg(not(windows))]
pub async fn post(
    _: tauri::WebviewWindow,
    _: Vec<u8>,
    _: String,
    _: u64,
) -> Result<&'static str, String> {
    Ok("unsupported")
}
