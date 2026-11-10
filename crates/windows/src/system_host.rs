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
    io::{Read, Write},
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
        Com::CoTaskMemFree, Pipes::*, RemoteDesktop::*, Services::*, StationsAndDesktops::*,
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
    if session(pid)? != sid || image(p.0)? != expected || (server && !system(p.0)) {
        return Err(Error::InputDenied);
    }
    Ok(p)
}
fn nowait(file: &File) -> Result<()> {
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    check(unsafe { SetNamedPipeHandleState(file.as_raw_handle(), &mode, null(), null()) })
}
fn transfer(file: &mut File, bytes: &mut [u8], write: bool, deadline: Instant) -> Result<()> {
    let mut done = 0;
    while done < bytes.len() {
        if STOP.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Err(failure());
        }
        let r = if write {
            file.write(&bytes[done..])
        } else {
            file.read(&mut bytes[done..])
        };
        match r {
            // A nonblocking byte pipe may successfully write zero bytes when
            // its kernel buffer is full. That is backpressure, not EOF.
            Ok(0) if write => {
                if Instant::now() >= deadline {
                    return Err(failure());
                }
                thread::sleep(Duration::from_millis(2));
            }
            Ok(0) => return Err(failure()),
            Ok(n) => done += n,
            Err(e) if matches!(e.raw_os_error(), Some(232 | 536 | 231)) => {
                if Instant::now() >= deadline {
                    return Err(failure());
                }
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return Err(failure()),
        }
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
                    FILE_ATTRIBUTE_NORMAL,
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
        nowait(&file)?;
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
                input.release_all()?;
                desk = Some(next);
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
            Err(error) => {
                report_error(&error);
                Response::Error
            }
        };
        if send(&mut file, &response).is_err() {
            break;
        }
    }
    let _ = input.release_all();
    drop(capture);
    drop(desk);
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
            let h = unsafe {
                CreateNamedPipeW(
                    pipe_name(sid).as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    16,
                    65536,
                    65536,
                    0,
                    &security,
                )
            };
            if h == INVALID_HANDLE_VALUE {
                break;
            }
            pending = Some(unsafe { File::from_raw_handle(h) });
        }
        let f = pending.as_ref().unwrap();
        let connected = unsafe { ConnectNamedPipe(f.as_raw_handle(), null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            let file = pending.take().unwrap();
            let expected = expected.clone();
            threads.push(thread::spawn(move || serve(file, sid, expected)));
        } else {
            thread::sleep(Duration::from_millis(10));
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
    loop {
        if let Ok(mut c) = Client::connect()
            && let Ok(Response::Displays { displays }) = c.request(&Request::Displays {})
            && let Some(d) = displays.first()
        {
            for sequence in 1..=60 {
                if let Response::Frame { bytes, desktop } = c.request(&Request::Capture {
                    display: d.id,
                    profile: 0,
                    layout: 1,
                    sequence,
                })? {
                    let frame = farsail_media::JpegFrame::from_wire(&bytes)?;
                    if frame.meta.monitor == d.id && !desktop.is_empty() {
                        return Ok(());
                    }
                }
                thread::sleep(Duration::from_millis(30));
            }
        }
        if Instant::now() >= until {
            return Err(failure());
        }
        thread::sleep(Duration::from_millis(100));
    }
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
    fn nonblocking_pipe_transfers_large_payload_after_zero_progress_backpressure() {
        let name = wide(format!(
            r"\\.\pipe\FarSail.TransferTest.{}.{}",
            unsafe { GetCurrentProcessId() },
            unsafe { GetCurrentThreadId() }
        ));
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_OUTBOUND,
                PIPE_TYPE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                1024,
                1024,
                0,
                null(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        let mut server = unsafe { File::from_raw_handle(handle) };
        let reader = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(3);
            let h = loop {
                let h = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ,
                        0,
                        null(),
                        OPEN_EXISTING,
                        FILE_ATTRIBUTE_NORMAL,
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
            thread::sleep(Duration::from_millis(150));
            let mut data = vec![0; 256 * 1024];
            file.read_exact(&mut data).unwrap();
            assert!(data.iter().all(|b| *b == 173));
        });
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if unsafe { ConnectNamedPipe(server.as_raw_handle(), null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED
            {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        let mut data = vec![173; 256 * 1024];
        transfer(&mut server, &mut data, true, until).unwrap();
        reader.join().unwrap();
    }
}
