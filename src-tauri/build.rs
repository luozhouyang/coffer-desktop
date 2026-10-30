fn main() {
    // The application manifest is embedded by this script instead of tauri-build
    // so it also reaches the library's unit-test binary.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    // tauri-runtime-wry imports comctl32!TaskDialogIndirect, which only exists in
    // Common Controls v6. tauri-build embeds that manifest in binaries but not in
    // test targets, so `cargo test` fails to load on Windows (0xc0000139).
    // Only the manifest is compiled here so the resource does not collide with
    // the icon/version resource tauri-build links into binaries.
    #[cfg(windows)]
    {
        use std::io::Write;

        println!("cargo:rerun-if-changed=windows-test.manifest");
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-test.manifest");
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let rc = out_dir.join("test-manifest.rc");
        // rc.exe treats backslashes in quoted strings as escapes, so use forward
        // slashes (which Windows accepts) instead of the native separators.
        let manifest_path = manifest.display().to_string().replace('\\', "/");
        let mut file = std::fs::File::create(&rc).expect("failed to create the resource script");
        writeln!(file, "1 24 \"{manifest_path}\"").expect("failed to write the resource script");
        drop(file);

        embed_resource::compile_for_everything(&rc, embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the Windows application manifest");
    }
}
