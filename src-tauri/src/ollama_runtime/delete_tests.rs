//! Deleting an installed runtime version: what is refused, what is removed, and that nothing
//! outside the version folder is ever touched (links are not followed, names are checked).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "trispr-ollama-del-{}-{}-{}-{}",
            tag,
            std::process::id(),
            crate::util::now_ms(),
            COUNTER.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `<root>/<version>/ollama.exe` (5 bytes) plus `lib/ggml.dll` (7 bytes): 12 bytes in total.
fn install(root: &Path, version: &str) -> PathBuf {
    let dir = root.join(version);
    fs::create_dir_all(dir.join("lib")).expect("create runtime dir");
    fs::write(dir.join("ollama.exe"), b"12345").expect("write exe");
    fs::write(dir.join("lib").join("ggml.dll"), b"1234567").expect("write dll");
    dir
}

/// A system in which `0.40.13` is running and selected and `0.30.4` is just an old install.
fn context<'a>(root: &'a Path) -> RuntimeDeleteContext<'a> {
    RuntimeDeleteContext {
        runtime_root: root,
        target_version: "0.40.13",
        recorded_version: "0.40.13",
        recorded_path: "",
        running_version: Some("0.40.13"),
        managed_alive: true,
        retry_delay: Duration::from_millis(5),
    }
}

fn dot_entries(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default()
}

fn refusal(result: Result<OllamaRuntimeDeleteResult, String>) -> String {
    let result = result.expect("a refusal is an Ok answer");
    assert!(!result.deleted, "must not delete: {:?}", result);
    assert_eq!(result.freed_bytes, 0);
    result.detail.expect("a refusal explains itself")
}

#[test]
fn an_unused_version_is_deleted_and_reports_the_freed_bytes() {
    let root = Scratch::new("ok");
    let old = install(root.path(), "0.30.4");
    let current = install(root.path(), "0.40.13");

    let result = delete_runtime_version_in(&context(root.path()), "0.30.4").unwrap();

    assert_eq!(
        result,
        OllamaRuntimeDeleteResult {
            deleted: true,
            freed_bytes: 12,
            detail: None
        }
    );
    assert!(!old.exists());
    assert!(current.join("ollama.exe").is_file(), "the other version stays");
    assert!(dot_entries(root.path()).is_empty(), "no leftover folder: {:?}", dot_entries(root.path()));
    // and it is really gone from the list the UI builds from the disk
    assert!(!scan_installed_runtime_versions(root.path()).contains_key("0.30.4"));
}

#[test]
fn a_v_prefix_and_surrounding_blanks_do_not_matter() {
    let root = Scratch::new("trim");
    install(root.path(), "0.30.4");
    let result = delete_runtime_version_in(&context(root.path()), " 0.30.4 ").unwrap();
    assert!(result.deleted);
}

#[test]
fn the_version_the_server_reports_is_never_deleted() {
    let root = Scratch::new("running");
    let dir = install(root.path(), "0.30.4");
    let mut ctx = context(root.path());
    ctx.running_version = Some("0.30.4");
    ctx.recorded_version = "0.40.13";

    let detail = refusal(delete_runtime_version_in(&ctx, "0.30.4"));

    assert!(detail.contains("running"), "{}", detail);
    assert!(dir.join("ollama.exe").is_file(), "nothing was touched");
    assert_eq!(fs::read(dir.join("lib").join("ggml.dll")).unwrap(), b"1234567");
}

#[test]
fn the_running_check_also_matches_a_v_prefixed_answer() {
    let root = Scratch::new("running-v");
    install(root.path(), "0.30.4");
    let mut ctx = context(root.path());
    ctx.running_version = Some("v0.30.4");
    refusal(delete_runtime_version_in(&ctx, "0.30.4"));
}

#[test]
fn the_version_recorded_in_the_settings_is_never_deleted_even_with_no_server() {
    let root = Scratch::new("recorded");
    let dir = install(root.path(), "0.30.4");
    let mut ctx = context(root.path());
    ctx.running_version = None;
    ctx.managed_alive = false;
    ctx.recorded_version = "0.30.4";

    let detail = refusal(delete_runtime_version_in(&ctx, "0.30.4"));

    assert!(detail.contains("set up to run"), "{}", detail);
    assert!(dir.join("ollama.exe").is_file());
}

#[test]
fn a_recorded_path_inside_the_folder_blocks_it_whatever_the_case_and_separators() {
    let root = Scratch::new("recorded-path");
    let dir = install(root.path(), "0.30.4");
    let mut ctx = context(root.path());
    ctx.running_version = None;
    ctx.recorded_version = "";
    let spelled = dir
        .join("ollama.exe")
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_uppercase();
    ctx.recorded_path = &spelled;

    let detail = refusal(delete_runtime_version_in(&ctx, "0.30.4"));

    assert!(detail.contains("managed Ollama process runs from"), "{}", detail);
    assert!(dir.join("ollama.exe").is_file());
}

#[test]
fn a_recorded_path_in_a_sibling_with_a_similar_name_does_not_block() {
    // `0.30.4` must not be protected by `0.30.40`'s path (prefix without separator).
    let root = Scratch::new("recorded-sibling");
    install(root.path(), "0.30.4");
    let sibling = install(root.path(), "0.30.40");
    let mut ctx = context(root.path());
    ctx.running_version = None;
    ctx.recorded_version = "0.30.40";
    let recorded = sibling.join("ollama.exe").to_string_lossy().to_string();
    ctx.recorded_path = &recorded;

    assert!(delete_runtime_version_in(&ctx, "0.30.4").unwrap().deleted);
    assert!(sibling.join("ollama.exe").is_file());
}

#[test]
fn the_selected_target_is_never_deleted_and_the_reason_says_what_to_do() {
    let root = Scratch::new("target");
    let dir = install(root.path(), "0.30.4");
    let mut ctx = context(root.path());
    ctx.target_version = "0.30.4";

    let detail = refusal(delete_runtime_version_in(&ctx, "0.30.4"));

    assert!(detail.contains("selected target"), "{}", detail);
    assert!(detail.contains("Select another version first"), "{}", detail);
    assert!(dir.join("ollama.exe").is_file());
}

#[test]
fn names_that_are_not_plain_versions_are_rejected_before_any_path_is_built() {
    let root = Scratch::new("names");
    let outside = Scratch::new("names-outside");
    fs::write(outside.path().join("keep.txt"), "keep").unwrap();
    install(root.path(), "0.30.4");

    for name in [
        "",
        "   ",
        "..",
        "../x",
        "..\\x",
        "0.30.4/..",
        "0.30.4\\..\\..",
        ".old-0.30.4-1",
        ".deleted-0.30.4-1",
        "a b",
        "C:\\Windows",
        "0.30.4;calc",
        "0.30..4",
        &"9".repeat(40),
    ] {
        let answer = delete_runtime_version_in(&context(root.path()), name);
        assert!(answer.is_err(), "{:?} must be rejected, got {:?}", name, answer);
    }
    assert!(root.path().join("0.30.4").join("ollama.exe").is_file());
    assert_eq!(fs::read_to_string(outside.path().join("keep.txt")).unwrap(), "keep");
}

#[test]
fn a_missing_folder_is_reported_not_invented() {
    let root = Scratch::new("missing");
    install(root.path(), "0.40.13");
    let detail = refusal(delete_runtime_version_in(&context(root.path()), "0.30.4"));
    assert!(detail.contains("not installed"), "{}", detail);
    assert!(dot_entries(root.path()).is_empty());
}

#[test]
fn a_folder_without_an_exe_is_not_a_runtime_and_stays() {
    let root = Scratch::new("no-exe");
    let dir = root.path().join("0.30.4");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("notes.txt"), "mine").unwrap();

    let detail = refusal(delete_runtime_version_in(&context(root.path()), "0.30.4"));

    assert!(detail.contains("no ollama.exe"), "{}", detail);
    assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "mine");
}

#[test]
fn a_plain_file_with_the_version_name_is_left_alone() {
    let root = Scratch::new("file");
    let file = root.path().join("0.30.4");
    fs::write(&file, "x").unwrap();
    refusal(delete_runtime_version_in(&context(root.path()), "0.30.4"));
    assert!(file.is_file());
}

#[test]
fn an_exe_that_is_running_blocks_the_delete_even_when_no_setting_says_so() {
    // A mapped, running image cannot be opened for writing: the same probe the installer uses.
    // A read-sharing-only handle reproduces that sharing violation.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let root = Scratch::new("exe-busy");
        let dir = install(root.path(), "0.30.4");
        let mut ctx = context(root.path());
        ctx.running_version = None;
        ctx.managed_alive = false;
        ctx.recorded_version = "0.40.13";

        let _held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ: nobody may write while this is open
            .open(dir.join("ollama.exe"))
            .expect("hold the exe");

        let detail = refusal(delete_runtime_version_in(&ctx, "0.30.4"));

        assert!(detail.contains("ollama.exe is running"), "{}", detail);
        assert!(dir.join("ollama.exe").is_file());
        assert!(dot_entries(root.path()).is_empty());
    }
}

#[cfg(target_os = "windows")]
fn junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .expect("run mklink");
    assert!(status.success(), "mklink /J {} {}", link.display(), target.display());
}

#[cfg(target_os = "windows")]
#[test]
fn a_junction_inside_the_folder_is_removed_as_a_link_and_its_target_survives() {
    let root = Scratch::new("junction-inside");
    let outside = Scratch::new("junction-outside");
    fs::create_dir_all(outside.path().join("deep")).unwrap();
    fs::write(outside.path().join("important.txt"), "keep me").unwrap();
    fs::write(outside.path().join("deep").join("more.txt"), "and me").unwrap();
    let dir = install(root.path(), "0.30.4");
    junction(&dir.join("linked"), outside.path());

    let result = delete_runtime_version_in(&context(root.path()), "0.30.4").unwrap();

    assert!(result.deleted, "{:?}", result);
    assert_eq!(result.freed_bytes, 12, "the linked folder is not part of the size");
    assert!(!dir.exists());
    assert_eq!(fs::read_to_string(outside.path().join("important.txt")).unwrap(), "keep me");
    assert_eq!(
        fs::read_to_string(outside.path().join("deep").join("more.txt")).unwrap(),
        "and me"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn a_version_folder_that_is_itself_a_junction_is_refused_and_its_target_survives() {
    let root = Scratch::new("junction-self");
    let elsewhere = Scratch::new("junction-self-target");
    install(elsewhere.path(), "real");
    junction(&root.path().join("0.30.4"), &elsewhere.path().join("real"));

    let detail = refusal(delete_runtime_version_in(&context(root.path()), "0.30.4"));

    assert!(detail.contains("not a plain folder"), "{}", detail);
    assert!(elsewhere.path().join("real").join("ollama.exe").is_file());
    assert!(root.path().join("0.30.4").exists(), "the link is untouched too");
}

#[cfg(target_os = "windows")]
#[test]
fn a_locked_file_never_leaves_a_half_deleted_runtime_behind() {
    // A file held open without delete sharing (a scanner, a running tool). Whatever Windows
    // allows here, the answer must be consistent: either nothing happened and the runtime is
    // whole, or the version left the list and only a dot folder remains.
    use std::os::windows::fs::OpenOptionsExt;
    let root = Scratch::new("locked");
    let dir = install(root.path(), "0.30.4");
    let locked = dir.join("lib").join("ggml.dll");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .expect("lock a file");

    let started = Instant::now();
    let result = delete_runtime_version_in(&context(root.path()), "0.30.4").unwrap();
    println!("locked-file delete answered {:?} after {:?}", result, started.elapsed());

    if result.deleted {
        assert!(!dir.exists(), "deleted means gone from the list");
        assert!(
            !scan_installed_runtime_versions(root.path()).contains_key("0.30.4"),
            "leftovers must not count as an installed version"
        );
        assert!(result.detail.is_some(), "a partial removal says so");
        assert!(result.freed_bytes < 12);
    } else {
        assert!(dir.join("ollama.exe").is_file(), "refused means untouched");
        assert!(locked.is_file());
        assert!(result.detail.unwrap().contains("could not be removed"));
        assert!(dot_entries(root.path()).is_empty());
    }

    // After the lock is gone the same call finishes the job.
    drop(held);
    cleanup_stale_runtime_dirs_older_than(root.path(), Duration::from_secs(0));
    if dir.exists() {
        assert!(delete_runtime_version_in(&context(root.path()), "0.30.4").unwrap().deleted);
    }
    assert!(!dir.exists());
    assert!(dot_entries(root.path()).is_empty(), "{:?}", dot_entries(root.path()));
}

#[test]
fn leftovers_of_a_delete_are_cleaned_up_and_never_restored_as_a_runtime() {
    let root = Scratch::new("leftover");
    let trash = root.path().join(".deleted-0.30.4-1");
    fs::create_dir_all(&trash).unwrap();
    fs::write(trash.join("ollama.exe"), "x").unwrap();

    // the interrupted-install recovery only knows `.old-*`
    assert!(recover_interrupted_swaps(root.path()).is_empty());
    assert!(!root.path().join("0.30.4").exists());

    cleanup_stale_runtime_dirs_older_than(root.path(), Duration::from_secs(0));
    assert!(!trash.exists());
}

#[test]
fn retry_io_stops_at_the_first_success_and_reports_the_last_error() {
    let mut calls = 0;
    let ok = retry_io(4, Duration::from_millis(1), || {
        calls += 1;
        if calls < 3 {
            Err(std::io::Error::other("busy"))
        } else {
            Ok(calls)
        }
    });
    assert_eq!(ok.unwrap(), 3);

    let mut calls = 0;
    let failed: std::io::Result<()> = retry_io(3, Duration::from_millis(1), || {
        calls += 1;
        Err(std::io::Error::other(format!("busy {}", calls)))
    });
    assert_eq!(calls, 3);
    assert!(failed.unwrap_err().to_string().contains("busy 3"));
}

#[test]
fn the_size_ignores_links_and_counts_nested_files() {
    let root = Scratch::new("size");
    let dir = install(root.path(), "0.30.4");
    fs::create_dir_all(dir.join("a").join("b")).unwrap();
    fs::write(dir.join("a").join("b").join("c.bin"), vec![0u8; 100]).unwrap();
    assert_eq!(directory_size_without_links(&dir), 112);
    assert_eq!(directory_size_without_links(&root.path().join("nothing")), 0);
}
