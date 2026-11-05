use super::*;
use std::io::Cursor;
use std::net::TcpListener;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

const STALL: Duration = Duration::from_secs(5);

fn sha_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buf);
        for (name, data) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    buf.into_inner()
}

/// Files of the "good" test runtime: name -> content.
fn good_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("whisper-cli.exe", b"cli-binary".to_vec()),
        ("whisper-server.exe", b"server-binary".to_vec()),
        ("whisper.dll", vec![7u8; 5000]),
    ]
}

fn pin_for(files: &[(&str, Vec<u8>)], zip: &[u8], url: &str) -> Variant {
    Variant {
        url: url.to_string(),
        zip_sha256: sha_hex(zip),
        zip_size: zip.len() as u64,
        files: files
            .iter()
            .map(|(name, data)| FileEntry {
                name: name.to_string(),
                length: data.len() as u64,
                sha256: sha_hex(data),
            })
            .collect(),
    }
}

fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    make_zip(&entries)
}

/// Serves `body` once on a local port; `send` bytes of it are written (fewer than the
/// announced `content_length` simulates a connection that dies mid-download).
fn serve(body: Vec<u8>, send: usize, content_length: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_length
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body[..send.min(body.len())]);
            let _ = stream.flush();
        }
    });
    format!("http://127.0.0.1:{}/whisper-runtime-test.zip", port)
}

fn serve_whole(body: &[u8]) -> String {
    serve(body.to_vec(), body.len(), body.len())
}

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "trispr-whisper-runtime-test-{}-{}-{}",
            tag,
            std::process::id(),
            unique_install_stamp()
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    /// The runtime root used by the install; a child, so a traversing entry would leave
    /// a trace in `self.0`.
    fn root(&self) -> PathBuf {
        self.0.join("runtime")
    }
    fn names(&self, dir: &Path) -> Vec<String> {
        names_in(dir)
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn install(variant: &Variant, root: &Path) -> Result<PathBuf, String> {
    install_variant("vulkan", variant, root, STALL, &mut |_, _, _, _| {})
}

#[test]
fn embedded_manifest_is_valid_and_covers_the_runtime_preflight() {
    for name in VARIANTS {
        let (version, variant) = load_variant(name).expect(name);
        assert_eq!(version, "v1.9.5");
        let names: Vec<&str> = variant.files.iter().map(|f| f.name.as_str()).collect();
        let required = match name {
            "cuda" => crate::transcription::CUDA_RUNTIME_REQUIRED_FILES,
            _ => crate::transcription::VULKAN_RUNTIME_REQUIRED_FILES,
        };
        for file in required {
            assert!(names.contains(file), "{} manifest lacks {}", name, file);
        }
        assert!(names.contains(&"whisper-server.exe"));
    }
    assert!(load_variant("cpu").is_err());
    assert!(load_variant("../cuda").is_err());
}

#[test]
fn url_allowlist_accepts_only_the_project_release_path() {
    let good = "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-vulkan-win64.zip";
    assert!(check_pinned_url(good).is_ok());
    for bad in [
        "http://github.com/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "https://evil.example/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "https://github.com.evil.example/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "https://github.com@evil.example/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "https://user:pw@github.com/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "https://github.com/Trissilein/Other/releases/download/t/a.zip",
        "https://github.com/Trissilein/Trispr_Flow/releases/latest/download/a.zip",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/../../x/a.zip",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/t/a.zip?x=1",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/t/a.zip#frag",
        "https://github.com:444/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "ftp://github.com/Trissilein/Trispr_Flow/releases/download/t/a.zip",
        "",
        "not a url",
    ] {
        assert!(check_pinned_url(bad).is_err(), "accepted: {}", bad);
    }
}

#[test]
fn install_refuses_a_foreign_url_before_touching_the_disk() {
    let tmp = TempRoot::new("foreign-url");
    let files = good_files();
    let zip = zip_of(&files);
    let pin = pin_for(&files, &zip, "https://evil.example/whisper.zip");
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(error.contains("Refusing"), "{}", error);
    assert!(!tmp.root().exists());
}

#[test]
fn install_places_exactly_the_pinned_files_and_leaves_no_leftovers() {
    let tmp = TempRoot::new("ok");
    let files = good_files();
    let zip = zip_of(&files);
    let pin = pin_for(&files, &zip, &serve_whole(&zip));
    let mut stages = Vec::new();
    let target = install_variant("vulkan", &pin, &tmp.root(), STALL, &mut |stage, _, _, _| {
        if stages.last().map(String::as_str) != Some(stage) {
            stages.push(stage.to_string());
        }
    })
    .unwrap();
    assert_eq!(target, tmp.root().join("vulkan"));
    assert_eq!(
        tmp.names(&target),
        vec!["whisper-cli.exe", "whisper-server.exe", "whisper.dll"]
    );
    for (name, data) in &files {
        assert_eq!(&fs::read(target.join(name)).unwrap(), data);
    }
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);
    assert_eq!(stages, ["download", "verify", "extract", "install", "done"]);
    assert!(downloaded_complete(&target, &pin.files));
}

#[test]
fn zip_checksum_mismatch_leaves_nothing_behind() {
    let tmp = TempRoot::new("zip-hash");
    let files = good_files();
    let zip = zip_of(&files);
    let mut pin = pin_for(&files, &zip, &serve_whole(&zip));
    pin.zip_sha256 = sha_hex(b"something else");
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(error.contains("checksum mismatch"), "{}", error);
    assert!(
        tmp.names(&tmp.root()).is_empty(),
        "{:?}",
        tmp.names(&tmp.root())
    );
}

#[test]
fn interrupted_download_leaves_no_partial_file() {
    let tmp = TempRoot::new("cut-off");
    let files = good_files();
    let zip = zip_of(&files);
    // The server announces the full size but dies after half of it.
    let url = serve(zip.clone(), zip.len() / 2, zip.len());
    let pin = pin_for(&files, &zip, &url);
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(
        error.contains("connection")
            || error.contains("ended early")
            || error.contains("Try again"),
        "{}",
        error
    );
    assert!(
        tmp.names(&tmp.root()).is_empty(),
        "{:?}",
        tmp.names(&tmp.root())
    );
}

#[test]
fn download_with_a_size_other_than_the_pin_is_refused() {
    let tmp = TempRoot::new("wrong-size");
    let files = good_files();
    let zip = zip_of(&files);
    let mut padded = zip.clone();
    padded.extend_from_slice(&[0u8; 64]);
    let pin = pin_for(&files, &zip, &serve_whole(&padded));
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(error.contains("expected"), "{}", error);
    assert!(tmp.names(&tmp.root()).is_empty());
}

#[test]
fn download_error_status_is_reported_and_leaves_nothing() {
    let tmp = TempRoot::new("http-404");
    let files = good_files();
    let zip = zip_of(&files);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let pin = pin_for(&files, &zip, &format!("http://127.0.0.1:{}/x.zip", port));
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(error.contains("HTTP 404"), "{}", error);
    assert!(tmp.names(&tmp.root()).is_empty());
}

#[test]
fn zip_slip_entry_is_rejected_and_nothing_escapes() {
    let tmp = TempRoot::new("slip");
    let files = good_files();
    // Same number of entries as the pin, but one of them walks out of the folder.
    let evil: Vec<(&str, Vec<u8>)> = vec![
        ("whisper-cli.exe", b"cli-binary".to_vec()),
        ("whisper-server.exe", b"server-binary".to_vec()),
        ("../evil.dll", vec![7u8; 5000]),
    ];
    let zip = zip_of(&evil);
    let pin = pin_for(&files, &zip, &serve_whole(&zip));
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(
        error.contains("Unsafe") || error.contains("Unexpected"),
        "{}",
        error
    );
    assert!(!tmp.0.join("evil.dll").exists());
    assert!(!tmp.root().join("evil.dll").exists());
    assert!(tmp.names(&tmp.root()).is_empty());
}

#[test]
fn nested_and_absolute_entries_are_rejected() {
    for bad_name in [
        "sub/whisper.dll",
        "/whisper.dll",
        "C:\\whisper.dll",
        "whisper.dll/",
    ] {
        let tmp = TempRoot::new("nested");
        let files = good_files();
        let evil: Vec<(&str, Vec<u8>)> = vec![
            ("whisper-cli.exe", b"cli-binary".to_vec()),
            ("whisper-server.exe", b"server-binary".to_vec()),
            (bad_name, vec![7u8; 5000]),
        ];
        let zip = zip_of(&evil);
        let pin = pin_for(&files, &zip, &serve_whole(&zip));
        assert!(install(&pin, &tmp.root()).is_err(), "accepted {}", bad_name);
        assert!(tmp.names(&tmp.root()).is_empty(), "{}", bad_name);
    }
}

fn assert_install_fails_cleanly(tag: &str, served: Vec<(&str, Vec<u8>)>, expect: &str) {
    let tmp = TempRoot::new(tag);
    let pinned_files = good_files();
    let zip = zip_of(&served);
    let pin = pin_for(&pinned_files, &zip, &serve_whole(&zip));
    let error = install(&pin, &tmp.root()).unwrap_err();
    assert!(error.contains(expect), "{}: {}", tag, error);
    assert!(
        tmp.names(&tmp.root()).is_empty(),
        "{}: {:?}",
        tag,
        tmp.names(&tmp.root())
    );
}

#[test]
fn wrong_file_lists_fail_without_leaving_a_partial_runtime() {
    let good = good_files();
    // A pinned file is missing (entry count differs).
    assert_install_fails_cleanly("missing", good[..2].to_vec(), "entries");
    // An extra file next to the pinned ones.
    let mut extra = good.clone();
    extra.push(("extra.dll", vec![1u8; 10]));
    assert_install_fails_cleanly("extra", extra, "entries");
    // Right count, but a file that is not on the list.
    let mut renamed = good.clone();
    renamed[2].0 = "other.dll";
    assert_install_fails_cleanly("renamed", renamed, "Unexpected file");
    // Same name, different content.
    let mut tampered = good.clone();
    tampered[2].1 = vec![9u8; 5000];
    assert_install_fails_cleanly("tampered", tampered, "checksum");
    // Same name, different length.
    let mut short = good.clone();
    short[2].1 = vec![7u8; 4999];
    assert_install_fails_cleanly("short", short, "bytes");
}

#[test]
fn failed_update_keeps_the_previous_install() {
    let tmp = TempRoot::new("keep-old");
    let old = tmp.root().join("vulkan");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("whisper-cli.exe"), b"old").unwrap();

    let files = good_files();
    let zip = zip_of(&files);
    let mut pin = pin_for(&files, &zip, &serve_whole(&zip));
    pin.files[2].sha256 = sha_hex(b"not the file");
    assert!(install(&pin, &tmp.root()).is_err());
    assert_eq!(fs::read(old.join("whisper-cli.exe")).unwrap(), b"old");
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);
}

#[test]
fn successful_update_replaces_the_previous_install() {
    let tmp = TempRoot::new("replace-old");
    let old = tmp.root().join("vulkan");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("stale.dll"), b"old").unwrap();

    let files = good_files();
    let zip = zip_of(&files);
    let pin = pin_for(&files, &zip, &serve_whole(&zip));
    let target = install(&pin, &tmp.root()).unwrap();
    assert!(!target.join("stale.dll").exists());
    assert!(target.join("whisper-cli.exe").exists());
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);
}

#[test]
fn interrupted_runs_are_cleaned_up_and_a_lost_install_is_restored() {
    let tmp = TempRoot::new("leftovers");
    let root = tmp.root();
    fs::create_dir_all(root.join(".staging-vulkan-5")).unwrap();
    fs::write(root.join(".download-vulkan-5.zip"), b"partial").unwrap();
    fs::create_dir_all(root.join(".old-vulkan-3")).unwrap();
    fs::create_dir_all(root.join(".old-vulkan-9")).unwrap();
    fs::write(
        root.join(".old-vulkan-9").join("whisper-cli.exe"),
        b"newest",
    )
    .unwrap();
    fs::create_dir_all(root.join(".staging-cuda-5")).unwrap();

    clean_leftovers(&root, "vulkan");
    assert_eq!(tmp.names(&root), vec![".staging-cuda-5", "vulkan"]);
    assert_eq!(
        fs::read(root.join("vulkan").join("whisper-cli.exe")).unwrap(),
        b"newest"
    );

    // With an install in place, backups are just deleted.
    fs::create_dir_all(root.join(".old-vulkan-12")).unwrap();
    clean_leftovers(&root, "vulkan");
    assert_eq!(tmp.names(&root), vec![".staging-cuda-5", "vulkan"]);
}

#[test]
fn downloaded_status_needs_every_pinned_file_with_its_length() {
    let tmp = TempRoot::new("status");
    let files = good_files();
    let zip = zip_of(&files);
    let pin = pin_for(&files, &zip, "http://127.0.0.1:1/x.zip");
    let dir = tmp.root().join("vulkan");
    assert!(!downloaded_complete(&dir, &pin.files));
    fs::create_dir_all(&dir).unwrap();
    for (name, data) in &files {
        fs::write(dir.join(name), data).unwrap();
    }
    assert!(downloaded_complete(&dir, &pin.files));
    fs::write(dir.join("whisper.dll"), b"truncated").unwrap();
    assert!(!downloaded_complete(&dir, &pin.files));
    fs::remove_file(dir.join("whisper-cli.exe")).unwrap();
    assert!(!downloaded_complete(&dir, &pin.files));
}

#[test]
fn status_reports_the_downloaded_variant_in_a_custom_root() {
    let tmp = TempRoot::new("status-root");
    let (_, vulkan) = load_variant("vulkan").unwrap();
    let dir = tmp.root().join("vulkan");
    fs::create_dir_all(&dir).unwrap();
    for file in &vulkan.files {
        // Right length, wrong content: the status check is a cheap completeness check.
        fs::write(dir.join(&file.name), vec![0u8; file.length as usize]).unwrap();
    }
    let status = status_in(&tmp.root()).unwrap();
    let vulkan_status = status
        .variants
        .iter()
        .find(|v| v.variant == "vulkan")
        .unwrap();
    assert!(vulkan_status.downloaded && vulkan_status.ready);
    let cuda_status = status
        .variants
        .iter()
        .find(|v| v.variant == "cuda")
        .unwrap();
    assert!(!cuda_status.downloaded);
    assert_eq!(status.whisper_cpp_version, "v1.9.5");
}

fn adapter(vendor: u32, name: &str) -> (u32, String) {
    (vendor, name.to_string())
}

#[test]
fn nvidia_gpu_gets_cuda_everything_else_vulkan() {
    let nvidia = adapter(NVIDIA_VENDOR_ID, "NVIDIA GeForce RTX 5070 Ti");
    let intel = adapter(0x8086, "Intel(R) UHD Graphics");
    let amd = adapter(0x1002, "AMD Radeon RX 7800 XT");

    let rec = recommend_from_adapters(&[intel.clone(), nvidia.clone()]);
    assert_eq!(rec.recommended, "cuda");
    assert!(rec.nvidia);
    assert_eq!(rec.gpu_name.as_deref(), Some("NVIDIA GeForce RTX 5070 Ti"));

    let rec = recommend_from_adapters(&[amd]);
    assert_eq!((rec.recommended.as_str(), rec.nvidia), ("vulkan", false));
    let rec = recommend_from_adapters(&[intel]);
    assert_eq!(rec.recommended, "vulkan");
    let rec = recommend_from_adapters(&[]);
    assert_eq!(rec.recommended, "vulkan");
    assert!(rec.gpu_name.is_none());
}

#[test]
fn nvidia_gpus_without_a_cuda_kernel_build_get_vulkan() {
    for name in [
        "NVIDIA GeForce GTX 1080 Ti",
        "NVIDIA GeForce GTX 960",
        "NVIDIA GeForce GT 1030",
        "NVIDIA TITAN Xp",
        "Quadro P2000",
        "Tesla P40",
    ] {
        let rec = recommend_from_adapters(&[adapter(NVIDIA_VENDOR_ID, name)]);
        assert_eq!(rec.recommended, "vulkan", "{}", name);
        assert!(rec.nvidia, "{}", name);
    }
    for name in [
        "NVIDIA GeForce GTX 1650",
        "NVIDIA GeForce GTX 1660 SUPER",
        "NVIDIA GeForce RTX 2060",
        "NVIDIA TITAN RTX",
        "NVIDIA RTX A4000",
        "NVIDIA GeForce RTX 4090",
    ] {
        let rec = recommend_from_adapters(&[adapter(NVIDIA_VENDOR_ID, name)]);
        assert_eq!(rec.recommended, "cuda", "{}", name);
    }
    // An old NVIDIA card next to a current one: the current one decides.
    let rec = recommend_from_adapters(&[
        adapter(NVIDIA_VENDOR_ID, "NVIDIA GeForce GTX 1060"),
        adapter(NVIDIA_VENDOR_ID, "NVIDIA GeForce RTX 3060"),
    ]);
    assert_eq!(rec.recommended, "cuda");
}

#[test]
fn cuda_errors_are_worded_for_the_user() {
    assert!(cuda_error_message(35).contains("driver"));
    assert!(cuda_error_message(100).contains("No CUDA-capable"));
    assert!(cuda_error_message(999).contains("999"));
}

/// Live test against the real release asset (Vulkan, 16 MB). Run on demand:
/// `cargo test --lib live_vulkan_download -- --ignored --nocapture`
/// Installs into a temporary folder, never into the user's `%LOCALAPPDATA%`.
#[test]
#[ignore]
fn live_vulkan_download_verifies_installs_and_starts() {
    let root = std::env::temp_dir().join(format!(
        "trispr-whisper-runtime-live-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let (_, vulkan) = load_variant("vulkan").unwrap();
    let mut last_stage = String::new();
    let target = install_variant(
        "vulkan",
        &vulkan,
        &root,
        DOWNLOAD_STALL_TIMEOUT,
        &mut |stage, message, done, total| {
            if stage != last_stage {
                println!("[live] {}: {} ({:?}/{:?})", stage, message, done, total);
                last_stage = stage.to_string();
            }
        },
    )
    .expect("live download failed");
    println!("[live] installed into {}", target.display());
    let mut names = Vec::new();
    for file in &vulkan.files {
        let bytes = fs::read(target.join(&file.name)).unwrap();
        assert_eq!(bytes.len() as u64, file.length);
        assert_eq!(sha_hex(&bytes), file.sha256);
        names.push(format!("{} ({} bytes, sha256 ok)", file.name, file.length));
    }
    println!("[live] files: {}", names.join(", "));
    println!("[live] root entries: {:?}", names_in(&root));
    let version =
        probe_whisper_cli(&target.join("whisper-cli.exe")).expect("whisper-cli --version");
    println!("[live] whisper-cli --version -> {}", version);
    assert!(downloaded_complete(&target, &vulkan.files));
    let status = status_in(&root).unwrap();
    println!("[live] status: {}", serde_json::to_string(&status).unwrap());
    assert!(status
        .variants
        .iter()
        .any(|v| v.variant == "vulkan" && v.downloaded));
    let _ = fs::remove_dir_all(&root);
}

/// Live self-test of a real CUDA runtime folder (needs an NVIDIA GPU + driver):
/// `TRISPR_LIVE_CUDA_DIR=<folder> cargo test --lib live_cuda_probe -- --ignored --nocapture`
#[test]
#[ignore]
fn live_cuda_probe_reports_device_count() {
    let dir = PathBuf::from(std::env::var("TRISPR_LIVE_CUDA_DIR").expect("TRISPR_LIVE_CUDA_DIR"));
    let count = cuda_device_count(&dir);
    println!("[live] cudaGetDeviceCount -> {:?}", count);
    println!(
        "[live] whisper-cli --version -> {:?}",
        probe_whisper_cli(&dir.join("whisper-cli.exe"))
    );
    println!(
        "[live] empty folder -> {:?}",
        cuda_device_count(&std::env::temp_dir().join("trispr-no-such-runtime"))
    );
    assert!(count.is_ok());
}
