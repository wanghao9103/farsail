#[cfg(target_os = "linux")]
mod linux;
fn main() {
    #[cfg(target_os = "linux")]
    if let Err(error) = linux::run() {
        eprintln!("FarSail 更新失败：{error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    std::process::exit(1);
}
