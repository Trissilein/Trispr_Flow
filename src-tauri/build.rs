fn main() {
    // On Windows, embed a comctl32 v6 manifest so that test binaries also
    // load the correct Common Controls DLL (required for TaskDialogIndirect).
    // Without this, cargo test --lib fails with STATUS_ENTRYPOINT_NOT_FOUND
    // because Windows loads comctl32.dll v5 which lacks that export.
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}\\comctl32-v6.manifest",
            manifest_dir
        );
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    }
    tauri_build::build()
}
