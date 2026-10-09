//! Public machine metadata, obtained from the operating system rather than browser identity.

#[cfg(windows)]
pub fn name() -> Option<String> {
    // COMPUTER_NAME_FORMAT::ComputerNamePhysicalDnsHostname (no domain suffix).
    const PHYSICAL_DNS_HOSTNAME: i32 = 5;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetComputerNameExW(format: i32, buffer: *mut u16, size: *mut u32) -> i32;
    }
    let mut size = 0;
    // SAFETY: null buffer with zero size requests the required UTF-16 capacity.
    unsafe { GetComputerNameExW(PHYSICAL_DNS_HOSTNAME, std::ptr::null_mut(), &mut size) };
    if size == 0 || size > 32768 {
        return None;
    }
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: the buffer holds `size` UTF-16 code units and remains live for the call.
    let success =
        unsafe { GetComputerNameExW(PHYSICAL_DNS_HOSTNAME, buffer.as_mut_ptr(), &mut size) };
    if success == 0 || size as usize > buffer.len() {
        return None;
    }
    let value = String::from_utf16(&buffer[..size as usize]).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(target_os = "linux")]
pub fn name() -> Option<String> {
    let value = std::fs::read_to_string("/proc/sys/kernel/hostname").ok()?;
    let value = value.trim();
    (!value.is_empty() && !value.contains('\0')).then(|| value.to_owned())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn name() -> Option<String> {
    None
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    #[test]
    fn reads_system_name_without_logging_machine_identity() {
        let name = super::name().expect("the OS should provide a computer name");
        assert!(!name.is_empty());
        assert!(!name.contains('\0'));
        assert_eq!(Some(name), super::name());
    }
}
