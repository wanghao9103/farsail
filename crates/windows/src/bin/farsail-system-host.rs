#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    if let Err(error) = farsail_windows::system_host::entry() {
        // No remote pixels, credentials or input payloads enter diagnostics.
        farsail_windows::system_host::report_error(&error);
        eprintln!("FarSail system host: {error}");
        std::process::exit(1);
    }
}
