//! LocalSystem service and session-scoped desktop agents. No network listener.
//! Only the installed FarSail executable in Program Files may connect. Both
//! ends pin the peer process, executable and interactive session; remote auth
//! and view/control permission remain in the ordinary desktop process.
use crate::{
    Error, Result, native,
    system_protocol::{MAX_PACKET, Request, Response},
};
use std::{
    collections::HashMap,
    fs::File,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::Authorization::*,
    Security::*,
    Storage::FileSystem::*,
    System::{
        Com::CoTaskMemFree, IO::*, Pipes::*, RemoteDesktop::*, Services::*, StationsAndDesktops::*,
        Threading::*,
    },
    UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
};
const NAME: &str = "FarSailDesktopHost";
static STOP: AtomicBool = AtomicBool::new(false);
static LAST_CONTACT: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
fn failure() -> Error {
    Error::Unavailable(format!("system desktop broker unavailable ({})", unsafe {
        GetLastError()
    }))
}
fn check(ok: i32) -> Result<()> {
    if ok == 0 { Err(failure()) } else { Ok(()) }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn process(id: u32) -> Result<Handle> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, id) };
    if h.is_null() {
        Err(failure())
    } else {
        Ok(Handle(h))
    }
}
fn session(id: u32) -> Result<u32> {
    let mut n = 0;
    check(unsafe { ProcessIdToSessionId(id, &mut n) })?;
    Ok(n)
}
fn image(h: HANDLE) -> Result<PathBuf> {
    let mut b = vec![0u16; 32768];
    let mut n = b.len() as u32;
    check(unsafe { QueryFullProcessImageNameW(h, 0, b.as_mut_ptr(), &mut n) })?;
    PathBuf::from(String::from_utf16_lossy(&b[..n as usize]))
        .canonicalize()
        .map_err(|_| failure())
}
fn token(h: HANDLE, access: u32) -> Result<Handle> {
    let mut t = null_mut();
    check(unsafe { OpenProcessToken(h, access, &mut t) })?;
    Ok(Handle(t))
}
fn system(h: HANDLE) -> bool {
    let Ok(t) = token(h, TOKEN_QUERY) else {
        return false;
    };
    let mut buf = vec![0usize; 128];
    let mut n = 0;
    unsafe {
        GetTokenInformation(
            t.0,
            TokenUser,
            buf.as_mut_ptr().cast(),
            (buf.len() * size_of::<usize>()) as u32,
            &mut n,
        ) != 0
            && IsWellKnownSid(
                (*(buf.as_ptr().cast::<TOKEN_USER>())).User.Sid,
                WinLocalSystemSid,
            ) != 0
    }
}
fn program_files() -> Result<PathBuf> {
    let mut p = null_mut();
    if unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut p) } < 0 {
        return Err(failure());
    }
    let mut n = 0;
    unsafe {
        while *p.add(n) != 0 {
            n += 1
        }
    };
    let path = PathBuf::from(unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(p, n)) });
    unsafe { CoTaskMemFree(p.cast()) };
    path.canonicalize().map_err(|_| failure())
}
fn installed() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|_| failure())?
        .canonicalize()
        .map_err(|_| failure())?;
    let root = program_files()?;
    if !exe.starts_with(&root)
        || exe
            .file_name()
            .is_none_or(|n| n != "farsail-system-host.exe")
    {
        return Err(Error::Unavailable(
            "system host requires a protected Program Files installation".into(),
        ));
    }
    Ok(exe)
}
fn pipe_name(id: u32) -> Vec<u16> {
    wide(format!(r"\\.\pipe\FarSail.Desktop.v1.{id}"))
}
fn peer(file: &File, server: bool, expected: &Path, sid: u32) -> Result<Handle> {
    let mut pid = 0;
    check(unsafe {
        if server {
            GetNamedPipeServerProcessId(file.as_raw_handle(), &mut pid)
        } else {
            GetNamedPipeClientProcessId(file.as_raw_handle(), &mut pid)
        }
    })?;
    let p = process(pid)?;
    if session(pid)? != sid {
        return Err(Error::Unavailable("broker peer session mismatch".into()));
    }
    if image(p.0)? != expected {
        return Err(Error::Unavailable("broker peer executable mismatch".into()));
    }
    if server && !system(p.0) {
        return Err(Error::Unavailable(
            "broker server token is not LocalSystem".into(),
        ));
    }
    Ok(p)
}
fn event() -> Result<Handle> {
    let handle = unsafe { CreateEventW(null(), 1, 0, null()) };
    if handle.is_null() {
        Err(failure())
    } else {
        Ok(Handle(handle))
    }
}
fn cancel_operation(file: &File, operation: &mut OVERLAPPED) {
    // Cancellation is not completion. Drain it before its buffer/OVERLAPPED or
    // event can leave scope, including races where I/O completed concurrently.
    unsafe {
        CancelIoEx(file.as_raw_handle(), operation);
        let mut count = 0;
        GetOverlappedResult(file.as_raw_handle(), operation, &mut count, 1);
    }
}
fn transfer(file: &mut File, bytes: &mut [u8], write: bool, deadline: Instant) -> Result<()> {
    let mut done = 0;
    while done < bytes.len() {
        if STOP.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Err(Error::Unavailable(
                "system broker I/O cancelled or timed out".into(),
            ));
        }
        let ready = event()?;
        let mut operation: OVERLAPPED = unsafe { zeroed() };
        operation.hEvent = ready.0;
        let mut count = 0;
        let length = u32::try_from(bytes.len() - done).map_err(|_| Error::Geometry)?;
        let ok = unsafe {
            if write {
                WriteFile(
                    file.as_raw_handle(),
                    bytes[done..].as_ptr(),
                    length,
                    &mut count,
                    &mut operation,
                )
            } else {
                ReadFile(
                    file.as_raw_handle(),
                    bytes[done..].as_mut_ptr(),
                    length,
                    &mut count,
                    &mut operation,
                )
            }
        };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_IO_PENDING {
                return Err(Error::Unavailable(format!(
                    "system broker I/O failed ({error})"
                )));
            }
            loop {
                if STOP.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    cancel_operation(file, &mut operation);
                    return Err(Error::Unavailable(
                        "system broker I/O cancelled or timed out".into(),
                    ));
                }
                match unsafe { WaitForSingleObject(ready.0, 20) } {
                    WAIT_OBJECT_0 => break,
                    WAIT_TIMEOUT => (),
                    _ => {
                        cancel_operation(file, &mut operation);
                        return Err(failure());
                    }
                }
            }
        }
        check(unsafe { GetOverlappedResult(file.as_raw_handle(), &operation, &mut count, 0) })?;
        if count == 0 {
            return Err(Error::Unavailable("system broker pipe closed".into()));
        }
        done += count as usize;
    }
    Ok(())
}
fn send<T: serde::Serialize>(file: &mut File, value: &T) -> Result<()> {
    let mut b = serde_json::to_vec(value).map_err(|_| failure())?;
    if b.len() > MAX_PACKET {
        return Err(Error::Geometry);
    }
    let until = Instant::now() + Duration::from_secs(3);
    transfer(file, &mut (b.len() as u32).to_le_bytes(), true, until)?;
    transfer(file, &mut b, true, until)
}
fn recv<T: serde::de::DeserializeOwned>(
    file: &mut File,
    wait: Duration,
    maximum: usize,
) -> Result<T> {
    let until = Instant::now() + wait;
    let mut len = [0; 4];
    transfer(file, &mut len, false, until)?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > maximum {
        return Err(Error::Geometry);
    }
    let mut b = vec![0; n];
    transfer(file, &mut b, false, Instant::now() + Duration::from_secs(3))?;
    serde_json::from_slice(&b).map_err(|_| Error::Geometry)
}
pub(crate) struct Client {
    file: File,
    _peer: Handle,
}
// Handles are owned and requests serialized by &mut self / the existing input mutex.
unsafe impl Send for Client {}
impl Client {
    pub(crate) fn connect() -> Result<Self> {
        let exe = std::env::current_exe()
            .map_err(|_| failure())?
            .canonicalize()
            .map_err(|_| failure())?;
        if !exe.starts_with(program_files()?) {
            return Err(failure());
        }
        let expected = exe.with_file_name("farsail-system-host.exe");
        let sid = session(unsafe { GetCurrentProcessId() })?;
        let name = pipe_name(sid);
        let until = Instant::now() + Duration::from_millis(150);
        let h = loop {
            let h = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                    null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE {
                break h;
            }
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY || Instant::now() >= until {
                return Err(failure());
            }
            thread::sleep(Duration::from_millis(5));
        };
        let file = unsafe { File::from_raw_handle(h) };
        let p = peer(&file, true, &expected, sid)?;
        *LAST_CONTACT.lock().unwrap() = Some(Instant::now());
        Ok(Self { file, _peer: p })
    }
    pub(crate) fn request(&mut self, r: &Request) -> Result<Response> {
        send(&mut self.file, r)?;
        let response = recv(&mut self.file, Duration::from_secs(3), MAX_PACKET)?;
        *LAST_CONTACT.lock().unwrap() = Some(Instant::now());
        Ok(response)
    }
}
pub fn available() -> bool {
    // Existing authenticated capture/input traffic is a stronger liveness
    // witness than repeatedly opening a listener while it is accepting peers.
    if LAST_CONTACT
        .lock()
        .unwrap()
        .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
    {
        return true;
    }
    Client::connect().is_ok()
}
struct Desktop(HDESK);
impl Drop for Desktop {
    fn drop(&mut self) {
        unsafe {
            CloseDesktop(self.0);
        }
    }
}
fn desktop_name(d: HDESK) -> Result<Vec<u16>> {
    let mut b = vec![0u16; 256];
    check(unsafe {
        GetUserObjectInformationW(
            d,
            UOI_NAME,
            b.as_mut_ptr().cast(),
            (b.len() * 2) as u32,
            null_mut(),
        )
    })?;
    let n = b.iter().position(|c| *c == 0).ok_or(Error::Geometry)?;
    b.truncate(n);
    Ok(b)
}
fn serve(mut file: File, sid: u32, expected: PathBuf) {
    let probe = peer(
        &file,
        false,
        &expected.with_file_name("farsail-system-host.exe"),
        sid,
    )
    .is_ok();
    let probe_path = expected.with_file_name("farsail-system-host.exe");
    let Ok(_client) = peer(
        &file,
        false,
        if probe { &probe_path } else { &expected },
        sid,
    ) else {
        return;
    };
    // Borrowed thread desktop stays valid for this worker's entire lifetime.
    let original_desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
    let mut capture: Option<native::Capture> = None;
    let mut capture_key = None;
    let mut input = native::InputSink::default();
    let mut desk: Option<Desktop> = None;
    let mut name = Vec::new();
    while !STOP.load(Ordering::SeqCst) {
        let Ok(request) = recv::<Request>(&mut file, Duration::from_secs(u32::MAX.into()), 4096)
        else {
            break;
        };
        if probe && matches!(request, Request::Input { .. } | Request::Release {}) {
            break;
        }
        let result = (|| -> Result<Response> {
            let d = unsafe {
                OpenInputDesktop(
                    0,
                    0,
                    DESKTOP_READOBJECTS
                        | DESKTOP_WRITEOBJECTS
                        | DESKTOP_SWITCHDESKTOP
                        | DESKTOP_CREATEWINDOW
                        | DESKTOP_ENUMERATE
                        | DESKTOP_HOOKCONTROL
                        | DESKTOP_JOURNALRECORD
                        | DESKTOP_JOURNALPLAYBACK,
                )
            };
            if d.is_null() {
                return Err(failure());
            }
            let next = Desktop(d);
            let next_name = desktop_name(d)?;
            let changed = name != next_name;
            if changed {
                drop(capture.take());
                capture_key = None;
                check(unsafe { SetThreadDesktop(d) })?;
                desk = Some(next);
                input.release_all()?;
                let was = !name.is_empty();
                name = next_name;
                if was && matches!(request, Request::Capture { .. }) {
                    return Ok(Response::Reset);
                }
            } else {
                drop(next)
            }
            match request {
                Request::Displays {} => Ok(Response::Displays {
                    displays: native::displays()?,
                }),
                Request::Capture {
                    display,
                    profile,
                    layout,
                    sequence,
                } => {
                    if capture_key != Some((display, profile)) {
                        capture = Some(native::Capture::new_with_profile(display, profile)?);
                        capture_key = Some((display, profile));
                    }
                    match capture
                        .as_mut()
                        .ok_or(Error::Geometry)?
                        .next_frame(layout, sequence)?
                    {
                        Some(frame) => Ok(Response::Frame {
                            bytes: frame.to_wire()?,
                            desktop: name.clone(),
                        }),
                        None => Ok(Response::Empty),
                    }
                }
                Request::Input {
                    input: action,
                    display,
                    layout,
                    desktop,
                } => {
                    // An old ordinary-desktop click must never approve a new
                    // UAC dialog before its own image has been delivered.
                    if desktop.is_empty() || desktop != name {
                        return Ok(Response::Reset);
                    }
                    if let Some(d) = &display
                        && !native::displays()?.iter().any(|actual| actual == d)
                    {
                        return Err(Error::Geometry);
                    }
                    input.apply(action, display.as_ref(), layout)?;
                    Ok(Response::Ok)
                }
                Request::Release {} => {
                    input.release_all()?;
                    Ok(Response::Ok)
                }
            }
        })();
        let response = match result {
            Ok(r) => r,
            Err(Error::CaptureReset(_)) => Response::Reset,
            Err(Error::Geometry) => Response::Geometry,
            Err(Error::UnsupportedInput) => Response::UnsupportedInput,
            Err(Error::InputDenied) => Response::InputDenied,
            Err(Error::Injection {
                inserted,
                expected,
                code,
            }) => Response::Injection {
                inserted,
                expected,
                code,
            },
            Err(error) => {
                report_error(&error);
                Response::Error
            }
        };
        if send(&mut file, &response).is_err() {
            break;
        }
    }
    drop(capture);
    // Release owed keys on the current input desktop even if it changed while
    // the client disconnected. No new down/click is generated by cleanup.
    let cleanup = unsafe {
        OpenInputDesktop(
            0,
            0,
            DESKTOP_READOBJECTS
                | DESKTOP_WRITEOBJECTS
                | DESKTOP_SWITCHDESKTOP
                | DESKTOP_CREATEWINDOW
                | DESKTOP_ENUMERATE
                | DESKTOP_HOOKCONTROL
                | DESKTOP_JOURNALRECORD
                | DESKTOP_JOURNALPLAYBACK,
        )
    };
    if !cleanup.is_null() {
        let cleanup = Desktop(cleanup);
        if unsafe { SetThreadDesktop(cleanup.0) } != 0 {
            desk = Some(cleanup);
        }
    }
    let _ = input.release_all();
    // A desktop handle cannot be closed while bound to this worker.
    unsafe { SetThreadDesktop(original_desktop) };
    drop(desk);
}
#[derive(Debug, PartialEq, Eq)]
enum PipeConnection {
    Listening,
    Connected,
    Disconnected,
}
struct PendingPipe {
    file: Option<File>,
    operation: Box<OVERLAPPED>,
    _ready: Handle,
    active: bool,
    connected: bool,
}
impl PendingPipe {
    fn new(name: &[u16], security: *const SECURITY_ATTRIBUTES, buffer: u32) -> Result<Self> {
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                16,
                buffer,
                buffer,
                0,
                security,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(failure());
        }
        let file = unsafe { File::from_raw_handle(h) };
        let ready = event()?;
        let mut operation: Box<OVERLAPPED> = Box::new(unsafe { zeroed() });
        operation.hEvent = ready.0;
        let mut pending = Self {
            file: Some(file),
            operation,
            _ready: ready,
            active: false,
            connected: false,
        };
        let ok = unsafe { ConnectNamedPipe(h, pending.operation.as_mut()) };
        if ok != 0 {
            pending.connected = true;
        } else {
            match unsafe { GetLastError() } {
                ERROR_IO_PENDING => pending.active = true,
                ERROR_PIPE_CONNECTED => pending.connected = true,
                _ => return Err(failure()),
            }
        }
        Ok(pending)
    }
    fn poll(&mut self) -> PipeConnection {
        if self.connected {
            return PipeConnection::Connected;
        }
        let mut count = 0;
        if unsafe {
            GetOverlappedResult(
                self.file.as_ref().unwrap().as_raw_handle(),
                self.operation.as_ref(),
                &mut count,
                0,
            )
        } != 0
        {
            self.active = false;
            self.connected = true;
            return PipeConnection::Connected;
        }
        if unsafe { GetLastError() } == ERROR_IO_INCOMPLETE {
            return PipeConnection::Listening;
        }
        self.active = false;
        PipeConnection::Disconnected
    }
}
impl Drop for PendingPipe {
    fn drop(&mut self) {
        if self.active {
            cancel_operation(self.file.as_ref().unwrap(), self.operation.as_mut());
        }
    }
}
fn agent(parent: u32) -> Result<()> {
    if !system(unsafe { GetCurrentProcess() }) {
        return Err(Error::InputDenied);
    }
    let exe = installed()?;
    let parent_process = process(parent)?;
    if !system(parent_process.0) || image(parent_process.0)? != exe {
        return Err(Error::InputDenied);
    }
    let sid = session(unsafe { GetCurrentProcessId() })?;
    if sid == 0 {
        return Err(Error::InputDenied);
    }
    let expected = exe.with_file_name("farsail-desktop.exe");
    let mut descriptor = null_mut();
    check(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)").as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    })?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let stop_event = Handle(unsafe {
        OpenEventW(
            SYNCHRONIZE,
            0,
            wide(format!("Global\\FarSail.Host.Stop.{parent}")).as_ptr(),
        )
    });
    if stop_event.0.is_null() {
        unsafe { LocalFree(descriptor) };
        return Err(failure());
    }
    let mut threads = Vec::new();
    let mut pending = None;
    while unsafe { WaitForSingleObject(parent_process.0, 0) } == WAIT_TIMEOUT
        && unsafe { WaitForSingleObject(stop_event.0, 0) } == WAIT_TIMEOUT
    {
        threads.retain(|t: &thread::JoinHandle<()>| !t.is_finished());
        if threads.len() >= 16 {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        if pending.is_none() {
            pending = Some(PendingPipe::new(&pipe_name(sid), &security, 65536)?);
        }
        match pending.as_mut().unwrap().poll() {
            PipeConnection::Connected => {
                let mut listener = pending.take().unwrap();
                let file = listener.file.take().unwrap();
                let expected = expected.clone();
                threads.push(thread::spawn(move || serve(file, sid, expected)));
            }
            PipeConnection::Disconnected => {
                drop(pending.take());
            }
            PipeConnection::Listening => {
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
    STOP.store(true, Ordering::SeqCst);
    drop(pending);
    for worker in threads {
        let _ = worker.join();
    }
    unsafe { LocalFree(descriptor) };
    Ok(())
}
fn privilege(name: &str) -> Result<()> {
    let t = token(
        unsafe { GetCurrentProcess() },
        TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
    )?;
    let mut luid = unsafe { zeroed() };
    check(unsafe { LookupPrivilegeValueW(null(), wide(name).as_ptr(), &mut luid) })?;
    let state = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    unsafe { SetLastError(0) };
    check(unsafe { AdjustTokenPrivileges(t.0, 0, &state, 0, null_mut(), null_mut()) })?;
    if unsafe { GetLastError() } != 0 {
        return Err(failure());
    }
    Ok(())
}
fn spawn_agent(exe: &Path, sid: u32) -> Result<Handle> {
    let t = token(
        unsafe { GetCurrentProcess() },
        TOKEN_DUPLICATE | TOKEN_QUERY,
    )?;
    let mut copy = null_mut();
    check(unsafe {
        DuplicateTokenEx(
            t.0,
            TOKEN_ALL_ACCESS,
            null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut copy,
        )
    })?;
    let copy = Handle(copy);
    check(unsafe { SetTokenInformation(copy.0, TokenSessionId, (&sid as *const u32).cast(), 4) })?;
    let mut startup: STARTUPINFOW = unsafe { zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    let mut desktop = wide("winsta0\\default");
    startup.lpDesktop = desktop.as_mut_ptr();
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    let mut command = wide(format!("\"{}\" --agent {}", exe.display(), unsafe {
        GetCurrentProcessId()
    }));
    check(unsafe {
        CreateProcessAsUserW(
            copy.0,
            wide(exe).as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_NO_WINDOW,
            null(),
            null(),
            &startup,
            &mut info,
        )
    })?;
    unsafe { CloseHandle(info.hThread) };
    Ok(Handle(info.hProcess))
}
unsafe extern "system" fn control(code: u32) {
    if code == SERVICE_CONTROL_STOP || code == SERVICE_CONTROL_SHUTDOWN {
        STOP.store(true, Ordering::SeqCst);
    }
}
unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let name = wide(NAME);
    let h = unsafe { RegisterServiceCtrlHandlerW(name.as_ptr(), Some(control)) };
    if h.is_null() {
        return;
    }
    let mut status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_START_PENDING,
        dwControlsAccepted: SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
        dwWin32ExitCode: 0,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    unsafe { SetServiceStatus(h, &status) };
    let result = (|| -> Result<()> {
        let exe = installed()?;
        for p in [
            "SeTcbPrivilege",
            "SeAssignPrimaryTokenPrivilege",
            "SeIncreaseQuotaPrivilege",
        ] {
            privilege(p)?;
        }
        let event_name = wide(format!("Global\\FarSail.Host.Stop.{}", unsafe {
            GetCurrentProcessId()
        }));
        let stop_event = Handle(unsafe { CreateEventW(null(), 1, 0, event_name.as_ptr()) });
        if stop_event.0.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(failure());
        }
        status.dwCurrentState = SERVICE_RUNNING;
        unsafe { SetServiceStatus(h, &status) };
        let mut agents: HashMap<u32, Handle> = HashMap::new();
        while !STOP.load(Ordering::SeqCst) {
            let mut list = null_mut();
            let mut count = 0;
            let mut active = Vec::new();
            if unsafe { WTSEnumerateSessionsW(null_mut(), 0, 1, &mut list, &mut count) } != 0 {
                for s in unsafe { std::slice::from_raw_parts(list, count as usize) } {
                    if s.SessionId != 0 && s.State == WTSActive {
                        active.push(s.SessionId)
                    }
                }
                unsafe { WTSFreeMemory(list.cast()) };
            }
            agents.retain(|sid, p| {
                let keep =
                    active.contains(sid) && unsafe { WaitForSingleObject(p.0, 0) } == WAIT_TIMEOUT;
                if !keep {
                    unsafe { TerminateProcess(p.0, 0) };
                }
                keep
            });
            for sid in active {
                if let std::collections::hash_map::Entry::Vacant(e) = agents.entry(sid) {
                    match spawn_agent(&exe, sid) {
                        Ok(p) => {
                            e.insert(p);
                        }
                        Err(error) => {
                            report_error(&error);
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        unsafe { SetEvent(stop_event.0) };
        for agent in agents.values() {
            if unsafe { WaitForSingleObject(agent.0, 5000) } == WAIT_TIMEOUT {
                unsafe { TerminateProcess(agent.0, 0) };
            }
        }
        Ok(())
    })();
    status.dwCurrentState = SERVICE_STOPPED;
    status.dwControlsAccepted = 0;
    if result.is_err() {
        status.dwWin32ExitCode = ERROR_SERVICE_SPECIFIC_ERROR;
        status.dwServiceSpecificExitCode = 1;
    }
    unsafe { SetServiceStatus(h, &status) };
}
fn install() -> Result<()> {
    let exe = installed()?;
    let manager = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CREATE_SERVICE) };
    if manager.is_null() {
        return Err(failure());
    }
    let name = wide(NAME);
    let command = wide(format!(
        "\"{}\" --service",
        exe.to_string_lossy().trim_start_matches(r"\\?\")
    ));
    let svc = unsafe {
        CreateServiceW(
            manager,
            name.as_ptr(),
            wide("FarSail 安全桌面服务").as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            command.as_ptr(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
        )
    };
    let svc = if svc.is_null() && unsafe { GetLastError() } == ERROR_SERVICE_EXISTS {
        let s = unsafe { OpenServiceW(manager, name.as_ptr(), SERVICE_ALL_ACCESS) };
        if !s.is_null() {
            let ok = unsafe {
                ChangeServiceConfigW(
                    s,
                    SERVICE_NO_CHANGE,
                    SERVICE_AUTO_START,
                    SERVICE_NO_CHANGE,
                    command.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                )
            };
            if ok == 0 {
                unsafe { CloseServiceHandle(s) };
                null_mut()
            } else {
                s
            }
        } else {
            s
        }
    } else {
        svc
    };
    if svc.is_null() {
        unsafe { CloseServiceHandle(manager) };
        return Err(failure());
    }
    let ok = unsafe { StartServiceW(svc, 0, null()) };
    let already = unsafe { GetLastError() } == ERROR_SERVICE_ALREADY_RUNNING;
    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(manager)
    };
    if ok == 0 && !already {
        return Err(failure());
    }
    Ok(())
}
pub fn entry() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--install") => install(),
        Some("--stop") => stop_service(false),
        Some("--uninstall") => stop_service(true),
        Some("--probe") => probe(),
        Some("--agent") => agent(
            args.get(2)
                .and_then(|s| s.parse().ok())
                .ok_or(Error::Geometry)?,
        ),
        Some("--service") => {
            if !system(unsafe { GetCurrentProcess() }) {
                return Err(Error::InputDenied);
            }
            let mut name = wide(NAME);
            let table = [
                SERVICE_TABLE_ENTRYW {
                    lpServiceName: name.as_mut_ptr(),
                    lpServiceProc: Some(service_main),
                },
                SERVICE_TABLE_ENTRYW {
                    lpServiceName: null_mut(),
                    lpServiceProc: None,
                },
            ];
            check(unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) })
        }
        _ => Err(Error::Geometry),
    }
}

fn stop_service(remove: bool) -> Result<()> {
    let _ = installed()?;
    let manager = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    if manager.is_null() {
        return Err(failure());
    }
    let svc = unsafe {
        OpenServiceW(
            manager,
            wide(NAME).as_ptr(),
            SERVICE_STOP | SERVICE_QUERY_STATUS | DELETE,
        )
    };
    if svc.is_null() {
        let absent = unsafe { GetLastError() } == ERROR_SERVICE_DOES_NOT_EXIST;
        unsafe { CloseServiceHandle(manager) };
        return if absent { Ok(()) } else { Err(failure()) };
    }
    let mut status: SERVICE_STATUS = unsafe { zeroed() };
    unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut status) };
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if unsafe { QueryServiceStatus(svc, &mut status) } == 0 {
            break;
        }
        if status.dwCurrentState == SERVICE_STOPPED {
            break;
        }
        if Instant::now() >= until {
            unsafe {
                CloseServiceHandle(svc);
                CloseServiceHandle(manager);
            }
            return Err(failure());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let ok = if remove {
        unsafe { DeleteService(svc) }
    } else {
        1
    };
    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(manager);
    }
    check(ok)
}
fn probe() -> Result<()> {
    let _ = installed()?;
    let until = Instant::now() + Duration::from_secs(10);
    let mut last = String::from("probe has not connected");
    while Instant::now() < until {
        let mut c = match Client::connect() {
            Ok(c) => c,
            Err(error) => {
                last = format!("probe connect: {error}");
                thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        let displays = match c.request(&Request::Displays {}) {
            Ok(Response::Displays { displays }) => displays,
            Ok(_) => return Err(Error::Unavailable("probe display response rejected".into())),
            Err(error) => return Err(Error::Unavailable(format!("probe display IPC: {error}"))),
        };
        let d = displays
            .first()
            .ok_or_else(|| Error::Unavailable("probe found no interactive monitors".into()))?;
        for sequence in 1..=60 {
            match c
                .request(&Request::Capture {
                    display: d.id,
                    profile: 0,
                    layout: 1,
                    sequence,
                })
                .map_err(|error| Error::Unavailable(format!("probe capture IPC: {error}")))?
            {
                Response::Frame { bytes, desktop } => {
                    let frame = farsail_media::JpegFrame::from_wire(&bytes)?;
                    if frame.meta.monitor == d.id && !desktop.is_empty() {
                        return Ok(());
                    }
                    return Err(Error::Geometry);
                }
                Response::Empty | Response::Reset => (),
                _ => {
                    return Err(Error::Unavailable(
                        "probe capture response rejected; inspect service event data".into(),
                    ));
                }
            }
            thread::sleep(Duration::from_millis(30));
        }
        last = "probe received no fresh desktop frame".into();
    }
    Err(Error::Unavailable(last))
}

/// Numeric/platform diagnostics only; never log frames, keys or IPC payloads.
pub fn report_error(error: &Error) {
    use windows_sys::Win32::System::EventLog::*;
    let source = unsafe { RegisterEventSourceW(null(), wide("FarSailDesktopHost").as_ptr()) };
    if source.is_null() {
        return;
    }
    let message = wide(error.to_string());
    let strings = [message.as_ptr()];
    unsafe {
        ReportEventW(
            source,
            EVENTLOG_ERROR_TYPE,
            0,
            1,
            null_mut(),
            1,
            0,
            strings.as_ptr(),
            null(),
        );
        DeregisterEventSource(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapped_pipe_waits_for_data_drains_cancelled_reads_and_transfers_large_frames() {
        let name = wide(format!(
            r"\\.\pipe\FarSail.TransferTest.{}.{}",
            unsafe { GetCurrentProcessId() },
            unsafe { GetCurrentThreadId() }
        ));
        let mut listener = PendingPipe::new(&name, null(), 1024).unwrap();
        assert_eq!(listener.poll(), PipeConnection::Listening);
        let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
        let reader = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(3);
            let h = loop {
                let h = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        0,
                        null(),
                        OPEN_EXISTING,
                        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                        null_mut(),
                    )
                };
                if h != INVALID_HANDLE_VALUE {
                    break h;
                }
                assert!(Instant::now() < until);
                thread::sleep(Duration::from_millis(2));
            };
            let mut file = unsafe { File::from_raw_handle(h) };
            let mut header = [0];
            assert!(
                transfer(
                    &mut file,
                    &mut header,
                    false,
                    Instant::now() + Duration::from_millis(30)
                )
                .is_err()
            );
            cancelled_tx.send(()).unwrap();
            transfer(&mut file, &mut header, false, until).unwrap();
            assert_eq!(header, [42]);
            thread::sleep(Duration::from_millis(150));
            let mut data = vec![0; 256 * 1024];
            transfer(&mut file, &mut data, false, until).unwrap();
            assert!(data.iter().all(|b| *b == 173));
        });
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if listener.poll() == PipeConnection::Connected {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        let mut server = listener.file.take().unwrap();
        cancelled_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        transfer(&mut server, &mut [42], true, until).unwrap();
        let mut data = vec![173; 256 * 1024];
        transfer(&mut server, &mut data, true, until).unwrap();
        reader.join().unwrap();
    }
}
