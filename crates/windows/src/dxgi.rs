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
                CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_NOT_FOUND,
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
fn unavailable(e: impl std::fmt::Display) -> Error {
    Error::Unavailable(e.to_string())
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
                    .map_err(unavailable)?;
                    let rotation = unsafe { duplication.GetDesc() }.Rotation.0 as u32;
                    return Ok(Self {
                        _adapter: adapter,
                        device,
                        context,
                        duplication,
                        rotation,
                        has_frame: false,
                    });
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
        Err(Error::Unavailable(
            "no DXGI output owns the selected display".into(),
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
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                return Err(Error::Unavailable(
                    "DXGI access lost; select display again after desktop or mode change".into(),
                ));
            }
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
        let mut staging = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut staging)) }
            .map_err(unavailable)?;
        let staging =
            staging.ok_or_else(|| Error::Unavailable("D3D staging texture missing".into()))?;
        unsafe { self.context.CopyResource(&staging, &texture) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(unavailable)?;
        let _mapped = Mapped {
            context: &self.context,
            texture: &staging,
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
