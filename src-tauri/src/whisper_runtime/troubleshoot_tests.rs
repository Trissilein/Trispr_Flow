use super::tests::{good_files, names_in, pin_for, serve_whole, sha_hex, zip_of, TempRoot, STALL};
use super::troubleshoot::*;
use super::*;
use std::cell::Cell;
use std::net::TcpListener;

const NO_URL: &str = "http://127.0.0.1:1/unused.zip";

fn cuda_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("whisper-cli.exe", b"cuda-cli".to_vec()),
        ("whisper-server.exe", b"cuda-server".to_vec()),
        ("cudart64_13.dll", vec![3u8; 7000]),
    ]
}

/// Test pins for both variants plus the zips that belong to them.
struct Packages {
    pins: Vec<(&'static str, Variant)>,
    vulkan_zip: Vec<u8>,
    cuda_zip: Vec<u8>,
}

fn packages() -> Packages {
    let vulkan_zip = zip_of(&good_files());
    let cuda_zip = zip_of(&cuda_files());
    Packages {
        pins: vec![
            ("cuda", pin_for(&cuda_files(), &cuda_zip, NO_URL)),
            ("vulkan", pin_for(&good_files(), &vulkan_zip, NO_URL)),
        ],
        vulkan_zip,
        cuda_zip,
    }
}

/// Writes a "download" next to the runtime folder and returns its path.
fn put(tmp: &TempRoot, name: &str, bytes: &[u8]) -> String {
    let dir = tmp.0.join("downloads");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn import(
    tmp: &TempRoot,
    variant: &str,
    pins: &[(&str, Variant)],
    path: &str,
) -> Result<PathBuf, RuntimeError> {
    import_with_pins(&tmp.root(), variant, pins, path, &mut |_, _, _, _| {})
}

fn error_of(result: Result<PathBuf, RuntimeError>) -> RuntimeError {
    result.expect_err("the import should have been refused")
}

fn assert_nothing_installed(tmp: &TempRoot) {
    let names = names_in(&tmp.root());
    assert!(
        names.is_empty(),
        "left behind in the runtime root: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------------------

#[test]
fn import_installs_the_right_zip_exactly_like_the_download() {
    let tmp = TempRoot::new("import-ok");
    let pk = packages();
    let path = put(&tmp, "whisper-runtime-vulkan-win64.zip", &pk.vulkan_zip);
    let mut stages: Vec<String> = Vec::new();
    let target = import_with_pins(
        &tmp.root(),
        "vulkan",
        &pk.pins,
        &path,
        &mut |stage, _, _, _| {
            if stages.last().map(String::as_str) != Some(stage) {
                stages.push(stage.to_string());
            }
        },
    )
    .unwrap();
    assert_eq!(target, tmp.root().join("vulkan"));
    assert_eq!(
        tmp.names(&target),
        vec!["whisper-cli.exe", "whisper-server.exe", "whisper.dll"]
    );
    for (name, data) in good_files() {
        assert_eq!(fs::read(target.join(name)).unwrap(), data);
    }
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);
    assert_eq!(stages, ["verify", "extract", "install", "done"]);
    // The user's file is only read.
    assert_eq!(fs::read(&path).unwrap(), pk.vulkan_zip);
    assert!(downloaded_complete(&target, &pk.pins[1].1.files));
}

#[test]
fn import_accepts_an_upper_case_zip_extension() {
    let tmp = TempRoot::new("import-ext");
    let pk = packages();
    let path = put(&tmp, "WHISPER-RUNTIME-VULKAN-WIN64.ZIP", &pk.vulkan_zip);
    assert!(import(&tmp, "vulkan", &pk.pins, &path).is_ok());
}

#[test]
fn import_of_the_other_variants_package_names_both() {
    let pk = packages();
    for (selected, zip, found_label, selected_label) in [
        ("cuda", &pk.vulkan_zip, "Vulkan", "NVIDIA CUDA"),
        ("vulkan", &pk.cuda_zip, "NVIDIA CUDA", "Vulkan"),
    ] {
        let tmp = TempRoot::new("import-wrong-variant");
        let path = put(&tmp, "package.zip", zip);
        let error = error_of(import(&tmp, selected, &pk.pins, &path));
        assert_eq!(error.code, "wrong_package", "{:?}", error);
        assert_eq!(
            error.message,
            format!(
                "This is the {} package, but you selected {}.",
                found_label, selected_label
            )
        );
        assert_nothing_installed(&tmp);
        assert!(!tmp.root().exists() || names_in(&tmp.root()).is_empty());
    }
}

#[test]
fn import_of_a_manipulated_zip_is_refused_with_the_detected_hash() {
    let tmp = TempRoot::new("import-tampered");
    let pk = packages();
    let mut tampered = pk.vulkan_zip.clone();
    // Flip one byte inside the payload of the big entry: same size, different SHA-256.
    let at = tampered
        .windows(8)
        .position(|w| w == [7u8; 8])
        .expect("payload of the test zip");
    tampered[at] ^= 0xFF;
    assert_eq!(tampered.len(), pk.vulkan_zip.len());
    let path = put(&tmp, "whisper-runtime-vulkan-win64.zip", &tampered);

    let error = error_of(import(&tmp, "vulkan", &pk.pins, &path));
    assert_eq!(error.code, "wrong_file", "{:?}", error);
    assert!(
        error.message.contains("does not match"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains(&sha_hex(&tampered)),
        "detected hash missing: {}",
        error.message
    );
    assert!(
        error.hint.contains(&pk.pins[1].1.zip_sha256),
        "{}",
        error.hint
    );
    assert_nothing_installed(&tmp);
}

#[test]
fn import_of_something_that_is_not_the_package_is_refused() {
    let pk = packages();
    // A zip-named file of the wrong size never gets hashed.
    let tmp = TempRoot::new("import-garbage");
    let path = put(&tmp, "notes.zip", b"this is not a speech engine");
    let error = error_of(import(&tmp, "vulkan", &pk.pins, &path));
    assert_eq!(error.code, "wrong_file");
    assert!(
        error.message.contains("does not match"),
        "{}",
        error.message
    );
    assert_nothing_installed(&tmp);

    // An empty file.
    let path = put(&tmp, "empty.zip", b"");
    assert_eq!(
        error_of(import(&tmp, "vulkan", &pk.pins, &path)).code,
        "wrong_file"
    );

    // Not a .zip at all, an empty path, a missing file, a folder.
    let path = put(&tmp, "readme.txt", &pk.vulkan_zip);
    let error = error_of(import(&tmp, "vulkan", &pk.pins, &path));
    assert_eq!(error.code, "not_zip");
    assert!(error.message.contains(".zip"));
    assert_eq!(
        error_of(import(&tmp, "vulkan", &pk.pins, "")).code,
        "not_zip"
    );
    assert_eq!(
        error_of(import(&tmp, "vulkan", &pk.pins, "  ")).code,
        "not_zip"
    );
    let missing = tmp.0.join("downloads").join("gone.zip");
    assert_eq!(
        error_of(import(
            &tmp,
            "vulkan",
            &pk.pins,
            &missing.display().to_string()
        ))
        .code,
        "file_missing"
    );
    let folder = tmp.0.join("downloads").join("folder.zip");
    fs::create_dir_all(&folder).unwrap();
    assert_eq!(
        error_of(import(
            &tmp,
            "vulkan",
            &pk.pins,
            &folder.display().to_string()
        ))
        .code,
        "file_missing"
    );
    assert_nothing_installed(&tmp);
}

#[test]
fn import_with_an_unknown_variant_is_refused() {
    let tmp = TempRoot::new("import-unknown-variant");
    let pk = packages();
    let path = put(&tmp, "x.zip", &pk.vulkan_zip);
    assert!(import(&tmp, "cpu", &pk.pins, &path).is_err());
    assert!(import(&tmp, "../vulkan", &pk.pins, &path).is_err());
    assert_nothing_installed(&tmp);
}

#[test]
fn import_with_a_matching_hash_but_unsafe_content_leaves_nothing() {
    // The pin names the hash of the evil zip, so the hash check passes: the pinned file list
    // and the unpacker's path checks are what stop it.
    let files = good_files();
    let cases: Vec<(&str, Vec<(&str, Vec<u8>)>)> = vec![
        (
            "zip-slip",
            vec![
                ("whisper-cli.exe", b"cli-binary".to_vec()),
                ("whisper-server.exe", b"server-binary".to_vec()),
                ("../evil.dll", vec![7u8; 5000]),
            ],
        ),
        (
            "unlisted-file",
            vec![
                ("whisper-cli.exe", b"cli-binary".to_vec()),
                ("whisper-server.exe", b"server-binary".to_vec()),
                ("other.dll", vec![7u8; 5000]),
            ],
        ),
        (
            "extra-entry",
            vec![
                ("whisper-cli.exe", b"cli-binary".to_vec()),
                ("whisper-server.exe", b"server-binary".to_vec()),
                ("whisper.dll", vec![7u8; 5000]),
                ("extra.dll", vec![1u8; 10]),
            ],
        ),
        (
            "wrong-content",
            vec![
                ("whisper-cli.exe", b"cli-binary".to_vec()),
                ("whisper-server.exe", b"server-binary".to_vec()),
                ("whisper.dll", vec![9u8; 5000]),
            ],
        ),
    ];
    for (tag, served) in cases {
        let tmp = TempRoot::new(tag);
        let zip = zip_of(&served);
        let pins = vec![("vulkan", pin_for(&files, &zip, NO_URL))];
        let path = put(&tmp, "package.zip", &zip);
        let error = error_of(import(&tmp, "vulkan", &pins, &path));
        assert!(
            matches!(error.code, "invalid_package" | "checksum"),
            "{}: {:?}",
            tag,
            error
        );
        assert!(!tmp.0.join("evil.dll").exists(), "{}", tag);
        assert!(!tmp.root().join("evil.dll").exists(), "{}", tag);
        assert_nothing_installed(&tmp);
    }
}

#[test]
fn failed_import_keeps_the_previous_install_and_a_good_one_replaces_it() {
    let tmp = TempRoot::new("import-keep-old");
    let pk = packages();
    let old = tmp.root().join("vulkan");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("whisper-cli.exe"), b"old").unwrap();

    let mut tampered = pk.vulkan_zip.clone();
    let at = tampered.windows(8).position(|w| w == [7u8; 8]).unwrap();
    tampered[at] ^= 0xFF;
    let bad = put(&tmp, "bad.zip", &tampered);
    assert!(import(&tmp, "vulkan", &pk.pins, &bad).is_err());
    assert_eq!(fs::read(old.join("whisper-cli.exe")).unwrap(), b"old");
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);

    let good = put(&tmp, "good.zip", &pk.vulkan_zip);
    let target = import(&tmp, "vulkan", &pk.pins, &good).unwrap();
    assert_eq!(
        fs::read(target.join("whisper-cli.exe")).unwrap(),
        b"cli-binary"
    );
    assert_eq!(tmp.names(&tmp.root()), vec!["vulkan"]);
}

#[test]
fn import_waits_for_a_running_setup_of_the_same_variant() {
    let tmp = TempRoot::new("import-busy");
    let pk = packages();
    let path = put(&tmp, "package.zip", &pk.vulkan_zip);
    let _running = begin_job("vulkan", &tmp.root()).unwrap();
    let error = error_of(import(&tmp, "vulkan", &pk.pins, &path));
    assert_eq!(error.code, "busy");
    assert_nothing_installed(&tmp);
}

// ---------------------------------------------------------------------------------------
// Error classification (with the texts the pipeline really produces)
// ---------------------------------------------------------------------------------------

fn code_of(raw: &str) -> &'static str {
    classify_error(raw).code
}

/// Serves one HTTP response with `status` and no body.
fn serve_status(status: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status
    );
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(head.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}/x.zip", port)
}

fn install_error(pin: &Variant, root: &Path) -> String {
    install_variant("vulkan", pin, root, STALL, &mut |_, _, _, _| {}).unwrap_err()
}

#[test]
fn download_failures_map_to_a_cause_and_a_hint() {
    let files = good_files();
    let zip = zip_of(&files);

    // HTTP status
    for (status, expected_message) in [
        ("404 Not Found", "HTTP 404"),
        ("403 Forbidden", "HTTP 403"),
        ("503 Service Unavailable", "HTTP 503"),
    ] {
        let tmp = TempRoot::new("class-http");
        let pin = pin_for(&files, &zip, &serve_status(status));
        let error = classify_error(&install_error(&pin, &tmp.root()));
        assert_eq!(error.code, "http_status", "{}: {:?}", status, error);
        assert!(
            error.message.contains(expected_message),
            "{}",
            error.message
        );
        assert!(!error.hint.is_empty());
    }

    // Nobody listens: connection refused is "cannot reach", not a crash.
    let tmp = TempRoot::new("class-refused");
    let free_port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let pin = pin_for(
        &files,
        &zip,
        &format!("http://127.0.0.1:{}/x.zip", free_port),
    );
    let raw = install_error(&pin, &tmp.root());
    assert_eq!(code_of(&raw), "offline", "{}", raw);

    // Hash mismatch of the whole archive.
    let tmp = TempRoot::new("class-checksum");
    let mut pin = pin_for(&files, &zip, &serve_whole(&zip));
    pin.zip_sha256 = sha_hex(b"something else");
    assert_eq!(code_of(&install_error(&pin, &tmp.root())), "checksum");

    // A cut connection.
    let tmp = TempRoot::new("class-cut");
    let url = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = zip.clone();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body[..body.len() / 2]);
            }
        });
        format!("http://127.0.0.1:{}/x.zip", port)
    };
    let pin = pin_for(&files, &zip, &url);
    let raw = install_error(&pin, &tmp.root());
    assert_eq!(code_of(&raw), "interrupted", "{}", raw);

    // The archive is fine but holds something else than the pin lists.
    let tmp = TempRoot::new("class-package");
    let other = zip_of(&[
        ("whisper-cli.exe", b"cli-binary".to_vec()),
        ("whisper-server.exe", b"server-binary".to_vec()),
        ("other.dll", vec![7u8; 5000]),
    ]);
    let pin = pin_for(&files, &other, &serve_whole(&other));
    assert_eq!(
        code_of(&install_error(&pin, &tmp.root())),
        "invalid_package"
    );
}

#[test]
fn local_failures_map_to_a_cause_and_a_hint() {
    use std::io::{Error, ErrorKind};
    // Disk full, as the pipeline words it.
    let full = Error::from(ErrorKind::StorageFull);
    assert_eq!(
        code_of(&describe_write_error("write 'a.dll'", &full)),
        "disk_full"
    );
    assert_eq!(
        code_of(&describe_write_error(
            "write 'a.dll'",
            &Error::from_raw_os_error(112)
        )),
        "disk_full"
    );
    // A download that stalls.
    let stalled = describe_stream_error(&Error::from(ErrorKind::TimedOut), 100, Some(1000));
    assert_eq!(code_of(&stalled), "timeout", "{}", stalled);
    // Access denied / sharing violation / antivirus, whatever the Windows language.
    for os_error in [5, 32, 225, 1224] {
        let raw = describe_write_error("write 'a.dll'", &Error::from_raw_os_error(os_error));
        assert_eq!(
            code_of(&raw),
            "file_access",
            "os error {}: {}",
            os_error,
            raw
        );
    }
    assert_eq!(
        code_of("Failed to open the downloaded archive: The system cannot find the file specified. (os error 2)"),
        "file_access"
    );
    // Other pipeline texts.
    assert_eq!(
        code_of("The speech engine is in use and cannot be replaced. Close other dictation activity, restart Trispr Flow and try again."),
        "in_use"
    );
    assert_eq!(
        code_of("Another setup of the vulkan speech engine is already running."),
        "busy"
    );
    assert_eq!(
        code_of("'whisper.dll' failed its checksum (expected a, got b)."),
        "checksum"
    );
    assert_eq!(
        code_of("The downloaded archive is not a valid zip file: invalid Zip archive"),
        "invalid_package"
    );
}

#[test]
fn transport_failures_are_told_apart() {
    let reach = "Failed to download the speech engine: could not reach the download server: https://github.com/x.zip";
    // Texts ureq prints for its error kinds.
    assert_eq!(
        code_of(&format!("{}: Dns Failed: failed to lookup address information: No such host is known. (os error 11001). Check the internet connection or proxy settings", reach)),
        "offline"
    );
    assert_eq!(
        code_of(&format!("{}: Connection Failed: Connect error: timed out. Check the internet connection or proxy settings", reach)),
        "offline"
    );
    for tls in [
        "Network Error: invalid peer certificate: UnknownIssuer",
        "Network Error: received fatal alert: HandshakeFailure",
        "Proxy failed to connect",
        "Provided proxy credentials are incorrect",
    ] {
        assert_eq!(
            code_of(&format!(
                "{}: {}. Check the internet connection or proxy settings",
                reach, tls
            )),
            "tls_proxy",
            "{}",
            tls
        );
    }
}

#[test]
fn every_class_has_a_message_and_a_hint_and_unknown_keeps_the_text() {
    for raw in [
        "Another setup of the vulkan speech engine is already running.",
        "Not enough free disk space to write 'x'",
        "The speech engine is in use and cannot be replaced.",
        "Speech engine archive checksum mismatch. Expected a, got b.",
        "the download server answered HTTP 500 (the release file may have been removed)",
        "could not reach the download server: Dns Failed",
        "The download stalled (no data for 60 s)",
        "The connection was closed before the download finished",
        "x (os error 5)",
        "Unexpected file in the archive: 'a'.",
        "something nobody has seen before",
    ] {
        let error = classify_error(raw);
        assert!(
            !error.message.is_empty() && !error.hint.is_empty(),
            "{}",
            raw
        );
        assert_eq!(error.detail, raw);
    }
    let unknown = classify_error("something nobody has seen before");
    assert_eq!(unknown.code, "unknown");
    assert_eq!(unknown.message, "something nobody has seen before");
    // The UI reads exactly these four fields.
    let json = serde_json::to_value(&unknown).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["code", "detail", "hint", "message"]);
}

// ---------------------------------------------------------------------------------------
// Manual info and opening the browser
// ---------------------------------------------------------------------------------------

#[test]
fn manual_info_comes_from_the_compiled_in_pin() {
    let (version, pins) = load_pins().unwrap();
    let info = manual_info_from(&version, &pins);
    assert_eq!(
        info.release_page_url,
        "https://github.com/Trissilein/Trispr_Flow/releases/tag/whisper-runtime-v1.9.5"
    );
    assert_eq!(info.whisper_cpp_version, "v1.9.5");
    assert_eq!(info.variants.len(), 2);
    for (entry, (name, pin)) in info.variants.iter().zip(&pins) {
        assert_eq!(entry.variant, *name);
        assert_eq!(entry.url, pin.url);
        assert_eq!(entry.sha256, pin.zip_sha256);
        assert_eq!(entry.size_bytes, pin.zip_size);
        assert_eq!(
            entry.size_mb,
            (pin.zip_size as f64 / 1048576.0).round() as u64
        );
        assert_eq!(entry.asset, format!("whisper-runtime-{}-win64.zip", name));
        assert!(entry.url.ends_with(&entry.asset));
        assert!(!entry.label.is_empty() && !entry.which_gpu_hint.is_empty());
    }
    assert!(info.variants[0].which_gpu_hint.contains("580"));
    assert_eq!(info.variants[0].variant, "cuda");
}

#[test]
fn only_the_pinned_downloads_may_be_opened() {
    let (_, pins) = load_pins().unwrap();
    let (_, cuda) = &pins[0];
    assert!(check_openable_url(&cuda.url, &pins).is_ok());
    assert!(check_openable_url(
        "https://github.com/Trissilein/Trispr_Flow/releases/tag/whisper-runtime-v1.9.5",
        &pins
    )
    .is_ok());
    for foreign in [
        "https://evil.example/whisper-runtime-cuda-win64.zip",
        "https://github.com.evil.example/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-cuda-win64.zip",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/other.zip",
        "https://github.com/Trissilein/Other/releases/tag/whisper-runtime-v1.9.5",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-cuda-win64.zip&calc",
        "https://github.com/Trissilein/Trispr_Flow/releases/download/whisper-runtime-v1.9.5/whisper-runtime-cuda-win64.zip?x=1",
        "http://github.com/Trissilein/Trispr_Flow/releases/tag/whisper-runtime-v1.9.5",
        "file:///C:/Windows/System32/calc.exe",
        "javascript:alert(1)",
        "calc.exe",
        "",
    ] {
        assert!(check_openable_url(foreign, &pins).is_err(), "accepted {}", foreign);
    }
}

#[test]
fn open_download_page_launches_only_the_pinned_url_of_a_known_variant() {
    let (_, pins) = load_pins().unwrap();
    let launched = Cell::new(0);
    let mut opened = String::new();
    open_download_page("vulkan", &pins, |url| {
        launched.set(launched.get() + 1);
        opened = url.to_string();
        Ok(())
    })
    .unwrap();
    assert_eq!(launched.get(), 1);
    assert_eq!(opened, pins[1].1.url);

    for bad in ["cpu", "", "../cuda", "https://evil.example/x"] {
        let error = open_download_page(bad, &pins, |_| {
            launched.set(launched.get() + 1);
            Ok(())
        });
        assert!(error.is_err(), "{}", bad);
    }
    // A pin that points elsewhere (tampered manifest) is refused before anything starts.
    let evil = vec![(
        "vulkan",
        pin_for(
            &good_files(),
            &zip_of(&good_files()),
            "https://evil.example/x.zip",
        ),
    )];
    assert!(open_download_page("vulkan", &evil, |_| {
        launched.set(launched.get() + 1);
        Ok(())
    })
    .is_err());
    assert_eq!(launched.get(), 1, "the launcher ran for a refused request");
}

#[cfg(target_os = "windows")]
#[test]
fn the_browser_launcher_refuses_shell_characters() {
    // Fails on the character check, before any process is started.
    for url in [
        "https://github.com/x&calc",
        "https://github.com/x|calc",
        "https://github.com/x^y",
        "https://github.com/%PATH%",
        "https://github.com/x y",
        "https://github.com/x\"y",
    ] {
        assert!(launch_browser(url).is_err(), "{}", url);
    }
}

// ---------------------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------------------

#[test]
fn diagnostics_mask_the_profile_folder() {
    let local = r"C:\Users\Max\AppData\Local";
    let profile = r"C:\Users\Max";
    assert_eq!(
        mask_user_dirs(
            r"C:\Users\Max\AppData\Local\Trispr Flow\runtime",
            Some(local),
            Some(profile)
        ),
        r"%LOCALAPPDATA%\Trispr Flow\runtime"
    );
    assert_eq!(
        mask_user_dirs(
            r"c:\users\max\appdata\local\Trispr Flow\runtime",
            Some(local),
            Some(profile)
        ),
        r"%LOCALAPPDATA%\Trispr Flow\runtime"
    );
    assert_eq!(
        mask_user_dirs(
            r"C:\Users\Max\Documents\runtime",
            Some(local),
            Some(profile)
        ),
        r"%USERPROFILE%\Documents\runtime"
    );
    // Only whole folder names count: "Maxwell" is not "Max".
    assert_eq!(
        mask_user_dirs(r"C:\Users\Maxwell\runtime", Some(local), Some(profile)),
        r"C:\Users\Maxwell\runtime"
    );
    assert_eq!(
        mask_user_dirs(r"D:\tmp\runtime", Some(local), Some(profile)),
        r"D:\tmp\runtime"
    );
    assert_eq!(
        mask_user_dirs(r"D:\tmp\runtime", None, None),
        r"D:\tmp\runtime"
    );
}

#[test]
fn diagnostics_carry_version_and_os_but_not_the_user_name() {
    let local =
        std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\Users\\x\\AppData\\Local".into());
    let root = PathBuf::from(&local).join("Trispr Flow").join("runtime");
    let diagnostics = collect_diagnostics(&root);
    let json = serde_json::to_string(&diagnostics).unwrap();
    assert_eq!(diagnostics.app_version, env!("CARGO_PKG_VERSION"));
    assert!(!diagnostics.os.is_empty());
    assert_eq!(diagnostics.variants.len(), 2);
    assert!(
        diagnostics.runtime_dir.starts_with("%LOCALAPPDATA%"),
        "{}",
        diagnostics.runtime_dir
    );
    if let Ok(user) = std::env::var("USERNAME") {
        if user.len() > 2 {
            assert!(
                !json.contains(&user),
                "user name leaked into the diagnostics: {}",
                json
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Live (on demand): the real release zips
// ---------------------------------------------------------------------------------------

/// `TRISPR_LIVE_ZIP=<file> TRISPR_LIVE_VARIANT=vulkan|cuda cargo test --lib live_import -- --ignored --nocapture`
/// Imports the real zip into a temporary runtime folder (`TRISPR_WHISPER_RUNTIME_DIR`), never
/// into the user's `%LOCALAPPDATA%`, and then tries the failure modes against the real pins.
#[test]
#[ignore]
fn live_import_of_a_real_release_zip() {
    let zip = std::env::var("TRISPR_LIVE_ZIP").expect("TRISPR_LIVE_ZIP");
    let variant = std::env::var("TRISPR_LIVE_VARIANT").expect("TRISPR_LIVE_VARIANT");
    let other = if variant == "cuda" { "vulkan" } else { "cuda" };
    let root = std::env::temp_dir().join(format!(
        "trispr-whisper-runtime-live-import-{}-{}",
        variant,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("TRISPR_WHISPER_RUNTIME_DIR", &root);
    assert_eq!(crate::paths::whisper_runtime_root(), root);
    let (_, pins) = load_pins().unwrap();

    // 1. Wrong variant selected: refused with a clear message, nothing on disk.
    let error = import_with_pins(
        &crate::paths::whisper_runtime_root(),
        other,
        &pins,
        &zip,
        &mut |_, _, _, _| {},
    )
    .expect_err("the other variant must refuse this zip");
    println!("[live] wrong variant -> {} | {}", error.code, error.message);
    assert_eq!(error.code, "wrong_package");
    assert!(!root.exists() || names_in(&root).is_empty());

    // 2. Right variant: full import.
    let started = Instant::now();
    let mut last = String::new();
    let target = import_with_pins(
        &crate::paths::whisper_runtime_root(),
        &variant,
        &pins,
        &zip,
        &mut |stage, message, _, _| {
            if stage != last {
                println!("[live] {}: {}", stage, message);
                last = stage.to_string();
            }
        },
    )
    .expect("live import failed");
    println!(
        "[live] imported into {} in {:?}",
        target.display(),
        started.elapsed()
    );
    let pin = &pins.iter().find(|(n, _)| *n == variant).unwrap().1;
    for file in &pin.files {
        let bytes = fs::read(target.join(&file.name)).unwrap();
        assert_eq!(bytes.len() as u64, file.length, "{}", file.name);
        assert_eq!(sha_hex(&bytes), file.sha256, "{}", file.name);
    }
    println!(
        "[live] {} files verified (length + sha256); root entries: {:?}",
        pin.files.len(),
        names_in(&root)
    );
    assert_eq!(names_in(&root), vec![variant.clone()]);
    assert!(downloaded_complete(&target, &pin.files));
    println!(
        "[live] status: {}",
        serde_json::to_string(&status_in(&root).unwrap()).unwrap()
    );
    println!(
        "[live] whisper-cli --version -> {:?}",
        probe_whisper_cli(&target.join("whisper-cli.exe"))
    );
    println!("[live] self-test -> {:?}", verify_variant(&variant));
    if variant == "cuda" {
        println!(
            "[live] cudaGetDeviceCount of the imported folder -> {:?}",
            cuda_device_count(&target)
        );
    }

    // 3. The zip with one flipped byte: refused, the good install stays untouched.
    let mut bytes = fs::read(&zip).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    let copy = root.with_extension("tampered.zip");
    fs::write(&copy, &bytes).unwrap();
    let error = import_with_pins(
        &crate::paths::whisper_runtime_root(),
        &variant,
        &pins,
        &copy.display().to_string(),
        &mut |_, _, _, _| {},
    )
    .expect_err("a tampered zip must be refused");
    println!("[live] tampered -> {} | {}", error.code, error.message);
    assert_eq!(error.code, "wrong_file");
    assert!(downloaded_complete(&target, &pin.files));
    assert_eq!(names_in(&root), vec![variant.clone()]);
    let _ = fs::remove_file(&copy);
    let _ = fs::remove_dir_all(&root);
}
