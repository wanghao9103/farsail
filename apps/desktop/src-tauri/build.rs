fn main() {
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    println!("cargo:rerun-if-env-changed=FARSAIL_RENDER_SMOKE_BUILD");
    let mut manifest = include_str!("windows-app-manifest.xml").to_owned();
    if std::env::var_os("FARSAIL_RENDER_SMOKE_BUILD").is_some() {
        assert_eq!(
            std::env::var("PROFILE").as_deref(),
            Ok("debug"),
            "render fixture is debug-only"
        );
        // Synthetic rendering needs no elevated input/capture privileges.
        manifest = manifest.replace("level=\"requireAdministrator\"", "level=\"asInvoker\"");
    }
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
