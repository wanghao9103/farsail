//! Native file-list clipboard access. No paths are accepted from the webview.
use std::path::PathBuf;

pub const MAX_FILES: usize = 32;

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };
    use windows_sys::Win32::{
        Foundation::{GlobalFree, HWND},
        System::{DataExchange::*, Memory::*},
        UI::{Shell::DragQueryFileW, WindowsAndMessaging::*},
    };

    const HDROP_FORMAT: u32 = 15;
    struct Clipboard(HWND);
    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
                if !self.0.is_null() {
                    DestroyWindow(self.0);
                }
            }
        }
    }
    fn open(write: bool) -> Result<Clipboard, String> {
        let owner = if write {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            // EmptyClipboard needs a real owner when publishing CF_HDROP.
            unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            }
        } else {
            std::ptr::null_mut()
        };
        if write && owner.is_null() {
            return Err("无法打开文件剪贴板。".into());
        }
        if unsafe { OpenClipboard(owner) } == 0 {
            if !owner.is_null() {
                unsafe {
                    DestroyWindow(owner);
                }
            }
            return Err("剪贴板正被其他应用使用，请重试。".into());
        }
        Ok(Clipboard(owner))
    }
    pub fn read_files() -> Result<Vec<PathBuf>, String> {
        let _clipboard = open(false)?;
        if unsafe { IsClipboardFormatAvailable(HDROP_FORMAT) } == 0 {
            return Ok(vec![]);
        }
        let drop = unsafe { GetClipboardData(HDROP_FORMAT) };
        if drop.is_null() {
            return Err("无法读取文件剪贴板。".into());
        }
        let count = unsafe { DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0) } as usize;
        if count == 0 || count > MAX_FILES {
            return Err("一次最多粘贴 32 个文件。".into());
        }
        (0..count)
            .map(|index| {
                let n = unsafe { DragQueryFileW(drop, index as u32, std::ptr::null_mut(), 0) };
                if n == 0 || n > 32767 {
                    return Err("文件路径不受支持。".into());
                }
                let mut path = vec![0u16; n as usize + 1];
                if unsafe { DragQueryFileW(drop, index as u32, path.as_mut_ptr(), n + 1) } != n {
                    return Err("文件剪贴板已变化，请重新复制。".into());
                }
                Ok(PathBuf::from(OsString::from_wide(&path[..n as usize])))
            })
            .collect()
    }
    pub fn write_files(paths: &[PathBuf]) -> Result<(), String> {
        if paths.is_empty() || paths.len() > MAX_FILES {
            return Err("文件列表无效。".into());
        }
        let mut names = Vec::new();
        for path in paths {
            if !path.is_absolute() || !path.is_file() {
                return Err("接收文件已不可用。".into());
            }
            let name: Vec<u16> = path.as_os_str().encode_wide().collect();
            if name.contains(&0) {
                return Err("文件路径无效。".into());
            }
            names.extend(name);
            names.push(0);
        }
        names.push(0);
        // DROPFILES: offset, POINT, fNC, fWide; followed by UTF-16 paths.
        let size = 20 + names.len() * 2;
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size) };
        if memory.is_null() {
            return Err("无法准备文件剪贴板。".into());
        }
        let data = unsafe { GlobalLock(memory) } as *mut u8;
        if data.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            return Err("无法准备文件剪贴板。".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(20u32.to_le_bytes().as_ptr(), data, 4);
            std::ptr::copy_nonoverlapping(1u32.to_le_bytes().as_ptr(), data.add(16), 4);
            std::ptr::copy_nonoverlapping(
                names.as_ptr().cast::<u8>(),
                data.add(20),
                names.len() * 2,
            );
            GlobalUnlock(memory);
        }
        let result = (|| {
            let _clipboard = open(true)?;
            if unsafe { EmptyClipboard() } == 0
                || unsafe { SetClipboardData(HDROP_FORMAT, memory) }.is_null()
            {
                return Err("无法写入远程文件剪贴板，请重试。".into());
            }
            Ok(())
        })();
        // Windows owns a successfully published block.
        if result.is_err() {
            unsafe {
                GlobalFree(memory);
            }
        }
        result
    }
}
#[cfg(windows)]
pub use native::{read_files, write_files};

#[cfg(not(windows))]
pub fn write_files(_: &[PathBuf]) -> Result<(), String> {
    Err("当前平台尚不支持作为远程文件粘贴目标。".into())
}
