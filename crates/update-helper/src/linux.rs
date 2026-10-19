use farsail_update_helper::{err, read_request, rule_text, validate_metadata, verify_package};
use fs2::FileExt;
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    process::Command,
};

const STATE: &str = "/var/lib/farsail-updater";
const RULES: &str = "/etc/polkit-1/rules.d";

fn trusted_directory(path: &Path) -> Result<(), String> {
    for item in path.ancestors() {
        let meta = fs::symlink_metadata(item).map_err(err)?;
        if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            return Err("更新助手目录权限无效。".into());
        }
    }
    Ok(())
}

fn caller() -> Result<(u32, String), String> {
    let uid: u32 = std::env::var("PKEXEC_UID")
        .map_err(err)?
        .parse()
        .map_err(err)?;
    if uid == 0 {
        return Err("不能为 root 保存桌面账号授权。".into());
    }
    // NSS, rather than a caller-provided account name, determines the subject.
    let mut buffer = vec![0u8; 65536];
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let code = unsafe {
        libc::getpwuid_r(
            uid,
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if code != 0 || result.is_null() {
        return Err("无法确认当前账号。".into());
    }
    let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }
        .to_str()
        .map_err(err)?
        .to_owned();
    Ok((uid, name))
}

fn register() -> Result<(), String> {
    let (uid, username) = caller()?;
    trusted_directory(Path::new(RULES))?;
    let mut file = tempfile::NamedTempFile::new_in(RULES).map_err(err)?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(err)?;
    file.write_all(rule_text(&username).as_bytes())
        .map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    file.persist(format!("{RULES}/49-farsail-update-{uid}.rules"))
        .map_err(err)?;
    fs::File::open(RULES)
        .and_then(|directory| directory.sync_all())
        .map_err(err)?;
    Ok(())
}

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C");
    cmd.current_dir("/");
    cmd
}
fn output(cmd: &mut Command) -> Result<String, String> {
    let output = cmd.output().map_err(err)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    String::from_utf8(output.stdout).map_err(err)
}

fn install() -> Result<(), String> {
    let (signature, bytes) = read_request(&mut std::io::stdin().lock())?;
    // Verify before handing even metadata to dpkg-deb. Only our release key grants code execution.
    let version = verify_package(&bytes, &signature)?;
    trusted_directory(Path::new("/var/lib"))?;
    match fs::create_dir(STATE) {
        Ok(()) => fs::set_permissions(STATE, fs::Permissions::from_mode(0o700)).map_err(err)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(err(e)),
    }
    trusted_directory(Path::new(STATE))?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(format!("{STATE}/lock"))
        .map_err(err)?;
    lock.try_lock_exclusive()
        .map_err(|_| "其他更新正在安装。")?;
    let dir = tempfile::tempdir_in(STATE).map_err(err)?;
    let package = dir.path().join("update.deb");
    fs::write(&package, &bytes).map_err(err)?;
    drop(bytes);
    let mut fields = Vec::new();
    for field in ["Package", "Version", "Architecture"] {
        fields.push(
            output(
                command("/usr/bin/dpkg-deb")
                    .arg("--field")
                    .arg(&package)
                    .arg(field),
            )?
            .trim()
            .to_owned(),
        );
    }
    let installed =
        output(command("/usr/bin/dpkg-query").args(["-W", "-f=${Version}", "far-sail"]))?;
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return Err("不支持当前架构。".into()),
    };
    validate_metadata(&fields.join("\n"), &version, arch, &installed)?;
    let status = command("/usr/bin/dpkg")
        .arg("--install")
        .arg(&package)
        .status()
        .map_err(err)?;
    if !status.success() {
        return Err("软件包安装未完成，请检查系统包管理器。".into());
    }
    let actual = output(command("/usr/bin/dpkg-query").args([
        "-W",
        "-f=${Status}\n${Version}",
        "far-sail",
    ]))?;
    if actual != format!("install ok installed\n{version}") {
        return Err("安装后的版本核验失败。".into());
    }
    register()?;
    Ok(())
}

pub fn run() -> Result<(), String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("需要系统授权。".into());
    }
    unsafe {
        libc::umask(0o022);
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode] if mode == "install" => {
            caller()?;
            install()
        }
        // Called by our root-owned postinst after the transition from the older updater.
        [mode] if mode == "register" => register(),
        [mode] if mode == "revoke" => {
            let (uid, _) = caller()?;
            trusted_directory(Path::new(RULES))?;
            match fs::remove_file(format!("{RULES}/49-farsail-update-{uid}.rules")) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(err(e)),
            }
        }
        _ => Err("不支持的更新操作。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writable_directories_and_symlinks_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        // /tmp itself is writable even if the leaf directory has safe permissions.
        assert!(trusted_directory(dir.path()).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("/usr", &link).unwrap();
        assert!(trusted_directory(&link).is_err());
    }
}
