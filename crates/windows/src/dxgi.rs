//! Adapter-matched DXGI duplication. All COM objects stay on the capture thread.
use super::{Error, Result};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Dxgi::{
                CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED,
                DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_MODE_CHANGE_IN_PROGRESS,
                DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND, DXGI_ERROR_UNSUPPORTED,
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1, IDXGIFactory1,
                IDXGIOutput5, IDXGIOutputDuplication,
            },
        },
    },
    core::Interface,
};

pub struct RawFrame {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
}
pub struct Duplication {
    _adapter: IDXGIAdapter1,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    pub rotation: u32,
    has_frame: bool,
    staging: Option<(D3D11_TEXTURE2D_DESC, ID3D11Texture2D)>,
}
struct Held<'a>(&'a IDXGIOutputDuplication);
impl Drop for Held<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.0.ReleaseFrame() };
    }
}
struct Mapped<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}
impl Drop for Mapped<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}
fn unavailable(e: windows::core::Error) -> Error {
    if matches!(
        e.code(),
        DXGI_ERROR_ACCESS_LOST | DXGI_ERROR_DEVICE_REMOVED | DXGI_ERROR_DEVICE_RESET
    ) {
        Error::CaptureReset(e.to_string())
    } else {
        Error::Unavailable(e.to_string())
    }
}
fn duplication_creation_error(e: windows::core::Error) -> Error {
    // These results describe the current output's mode/resource availability,
    // not every D3D/DXGI API. The host's bounded recovery policy decides how long
    // to wait; access-denied, protected-content and disconnected desktops still
    // remain terminal and are never bypassed here.
    if matches!(
        e.code(),
        DXGI_ERROR_UNSUPPORTED
            | DXGI_ERROR_MODE_CHANGE_IN_PROGRESS
            | DXGI_ERROR_NOT_CURRENTLY_AVAILABLE
    ) {
        Error::CaptureReset(format!("DuplicateOutput1: {e}"))
    } else {
        unavailable(e)
    }
}
impl Duplication {
    pub fn new(hmonitor: *mut std::ffi::c_void) -> Result<Self> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(unavailable)?;
        let mut adapter_index = 0;
        loop {
            let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
                Ok(a) => a,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(unavailable(e)),
            };
            let mut output_index = 0;
            loop {
                let output = match unsafe { adapter.EnumOutputs(output_index) } {
                    Ok(o) => o,
                    Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(e) => return Err(unavailable(e)),
                };
                let desc = unsafe { output.GetDesc() }.map_err(unavailable)?;
                if desc.Monitor.0 == hmonitor {
                    let mut device = None;
                    let mut context = None;
                    unsafe {
                        D3D11CreateDevice(
                            &adapter,
                            D3D_DRIVER_TYPE_UNKNOWN,
                            HMODULE::default(),
                            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                            Some(&[D3D_FEATURE_LEVEL_11_0]),
                            D3D11_SDK_VERSION,
                            Some(&mut device),
                            None,
                            Some(&mut context),
                        )
                    }
                    .map_err(unavailable)?;
                    let device =
                        device.ok_or_else(|| Error::Unavailable("D3D device missing".into()))?;
                    let context =
                        context.ok_or_else(|| Error::Unavailable("D3D context missing".into()))?;
                    let output: IDXGIOutput5 = output.cast().map_err(unavailable)?;
                    let duplication = unsafe {
                        output.DuplicateOutput1(&device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])
                    }
                    .map_err(duplication_creation_error)?;
                    let rotation = unsafe { duplication.GetDesc() }.Rotation.0 as u32;
                    return Ok(Self {
                        _adapter: adapter,
                        device,
                        context,
                        duplication,
                        rotation,
                        has_frame: false,
                        staging: None,
                    });
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
        // A mode switch can briefly invalidate the HMONITOR obtained just
        // before DXGI enumerates its outputs. Retry this missing selected route
        // within the host's existing deadline; do not retry arbitrary API errors.
        Err(Error::CaptureReset(
            "selected DXGI output is temporarily absent".into(),
        ))
    }
    pub fn frame(&mut self) -> Result<Option<RawFrame>> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        match unsafe {
            self.duplication
                .AcquireNextFrame(80, &mut info, &mut resource)
        } {
            Ok(()) => (),
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
            Err(e) => return Err(unavailable(e)),
        }
        let _held = Held(&self.duplication);
        if info.LastPresentTime == 0 && self.has_frame {
            return Ok(None);
        }
        self.has_frame = true;
        let resource =
            resource.ok_or_else(|| Error::Unavailable("DXGI frame resource missing".into()))?;
        let texture: ID3D11Texture2D = resource.cast().map_err(unavailable)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            || desc.Width == 0
            || desc.Height == 0
            || desc.Width as u64 * desc.Height as u64 > 16_000_000
        {
            return Err(Error::Geometry);
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        // Keep the GPU readback allocation across frames. A mode/descriptor
        // change recreates it before CopyResource; mapped access remains scoped.
        if self
            .staging
            .as_ref()
            .is_none_or(|(cached, _)| *cached != desc)
        {
            let mut staging = None;
            unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut staging)) }
                .map_err(unavailable)?;
            self.staging = Some((
                desc,
                staging.ok_or_else(|| Error::Unavailable("D3D staging texture missing".into()))?,
            ));
        }
        let (_, staging) = self.staging.as_ref().unwrap();
        unsafe { self.context.CopyResource(staging, &texture) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(unavailable)?;
        let _mapped = Mapped {
            context: &self.context,
            texture: staging,
        };
        let width = desc.Width as usize;
        let height = desc.Height as usize;
        let pitch = mapped.RowPitch as usize;
        if pitch < width * 4
            || pitch
                .checked_mul(height)
                .is_none_or(|n| n > 16_000_000 * 4 + 4096 * height)
        {
            return Err(Error::Geometry);
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * height) }
                .to_vec();
        Ok(Some(RawFrame {
            data: bytes,
            width,
            height,
            pitch,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{E_ACCESSDENIED, E_INVALIDARG},
            Graphics::Dxgi::{
                DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_CANNOT_PROTECT_CONTENT,
                DXGI_ERROR_INVALID_CALL, DXGI_ERROR_SESSION_DISCONNECTED,
            },
        },
        core::{Error as WindowsError, HRESULT},
    };

    fn error(code: HRESULT) -> WindowsError {
        WindowsError::from_hresult(code)
    }

    #[test]
    fn temporary_output_creation_failures_are_recoverable_only_at_creation() {
        for code in [
            DXGI_ERROR_UNSUPPORTED,
            DXGI_ERROR_MODE_CHANGE_IN_PROGRESS,
            DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
        ] {
            assert!(matches!(
                duplication_creation_error(error(code)),
                Error::CaptureReset(_)
            ));
            assert!(matches!(unavailable(error(code)), Error::Unavailable(_)));
        }
    }

    #[test]
    fn lost_or_reset_devices_still_recreate_in_both_contexts() {
        for code in [
            DXGI_ERROR_ACCESS_LOST,
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
        ] {
            assert!(matches!(unavailable(error(code)), Error::CaptureReset(_)));
            assert!(matches!(
                duplication_creation_error(error(code)),
                Error::CaptureReset(_)
            ));
        }
    }

    #[test]
    fn security_disconnect_and_programming_errors_are_not_mode_recovery() {
        for code in [
            E_ACCESSDENIED,
            DXGI_ERROR_ACCESS_DENIED,
            DXGI_ERROR_CANNOT_PROTECT_CONTENT,
            DXGI_ERROR_SESSION_DISCONNECTED,
            E_INVALIDARG,
            DXGI_ERROR_INVALID_CALL,
        ] {
            assert!(matches!(
                duplication_creation_error(error(code)),
                Error::Unavailable(_)
            ));
            assert!(matches!(unavailable(error(code)), Error::Unavailable(_)));
        }
    }
}
