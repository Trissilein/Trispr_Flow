//! Fault-injection tests for the runtime management: a stand-in GitHub that rate-limits,
//! cuts answers off or is not there at all, downloads that break mid-stream, installs that
//! race or die half way, and starts that fail after the old server was already stopped.
//! Every test shows the behaviour at the object, not just the happy path.

use super::*;
use crate::ai_fallback::test_support::{MockReply, MockRequest, MockServer};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

/// Self-cleaning scratch directory.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "trispr-ollama-rel-{}-{}-{}-{}",
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

fn marker(dir: &Path, text: &str) {
    fs::create_dir_all(dir).expect("create dir");
    fs::write(dir.join("ollama.exe"), text).expect("write marker");
}

fn read_marker(dir: &Path) -> String {
    fs::read_to_string(dir.join("ollama.exe")).unwrap_or_default()
}

fn dot_entries(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// ───────────────────────── 1. version catalog: offline / degraded ─────────────────────────

fn github_api(base: &str) -> GithubApi {
    GithubApi {
        tags_url: format!("{}/tags", base),
        releases_url: format!("{}/releases", base),
    }
}

fn release_body(version: &str) -> serde_json::Value {
    json!({
        "name": format!("v{}", version),
        "prerelease": false,
        "published_at": "2026-10-28T00:00:00Z",
        "body": "notes",
        "assets": [{
            "name": "ollama-windows-amd64.zip",
            "size": 1_600_000_000u64,
            "digest": format!("sha256:{}", "ab".repeat(32)),
            "browser_download_url": format!(
                "https://github.com/ollama/ollama/releases/download/v{}/ollama-windows-amd64.zip",
                version
            )
        }]
    })
}

fn tag_refs(tags: &[&str]) -> serde_json::Value {
    serde_json::Value::Array(
        tags.iter()
            .map(|tag| json!({ "ref": format!("refs/tags/{}", tag) }))
            .collect(),
    )
}

const TAGS: [&str; 5] = ["v0.40.13", "v0.40.7", "v0.39.2", "v0.38.1", "v0.37.9"];

/// A GitHub that answers the tag list and every release, counting both kinds of request.
fn healthy_github() -> (MockServer, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let tag_requests = Arc::new(AtomicUsize::new(0));
    let release_requests = Arc::new(AtomicUsize::new(0));
    let (tags, releases) = (tag_requests.clone(), release_requests.clone());
    let server = MockServer::start(move |request: &MockRequest| {
        if request.path == "/tags" {
            tags.fetch_add(1, AtomicOrdering::SeqCst);
            return MockReply::ok(tag_refs(&TAGS));
        }
        if let Some(version) = request.path.strip_prefix("/releases/tags/v") {
            releases.fetch_add(1, AtomicOrdering::SeqCst);
            return MockReply::ok(release_body(version));
        }
        MockReply::error(404, "not found")
    });
    (server, tag_requests, release_requests)
}

fn epoch_now() -> u64 {
    now_epoch_secs()
}

#[test]
fn waits_are_written_the_way_a_person_reads_them() {
    assert_eq!(format_wait(0), "1 s");
    assert_eq!(format_wait(45), "45 s");
    assert_eq!(format_wait(60), "1 min");
    assert_eq!(format_wait(61), "2 min");
    assert_eq!(format_wait(754), "13 min");
    assert_eq!(format_wait(3600), "1 h");
    assert_eq!(format_wait(3600 + 5 * 60), "1 h 5 min");
    assert_eq!(format_wait(u64::MAX), format!("{} h {} min", u64::MAX.div_ceil(60) / 60, u64::MAX.div_ceil(60) % 60));
}

#[test]
fn the_wait_comes_from_retry_after_first_then_from_the_reset_time() {
    assert_eq!(github_wait_secs(Some("90"), Some("5000"), 1000), Some(90));
    assert_eq!(github_wait_secs(None, Some("1754"), 1000), Some(754));
    // a reset time that already passed is "now", never an underflow
    assert_eq!(github_wait_secs(None, Some("900"), 1000), Some(0));
    assert_eq!(github_wait_secs(Some("soon"), Some("junk"), 1000), None);
    assert_eq!(github_wait_secs(None, None, 1000), None);
}

#[test]
fn an_unreachable_github_is_reported_with_advice_and_without_panic() {
    let started = Instant::now();
    let error = match fetch_online_catalog_from(&github_api("http://127.0.0.1:1"), &[]) {
        Ok(_) => panic!("nothing listens on that port"),
        Err(error) => error,
    };
    assert!(error.contains("Could not reach GitHub"), "{}", error);
    assert!(error.contains("internet connection"), "{}", error);
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
}

#[test]
fn a_rate_limit_names_the_wait_from_the_reset_header() {
    let reset = (epoch_now() + 11 * 60 + 30).to_string();
    let server = MockServer::start(move |_| {
        MockReply::error(403, "API rate limit exceeded for 1.2.3.4")
            .with_header("X-RateLimit-Limit", "60")
            .with_header("X-RateLimit-Remaining", "0")
            .with_header("X-RateLimit-Reset", &reset)
    });
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("rate limited"),
        Err(error) => error,
    };
    assert!(error.contains("rate limit"), "{}", error);
    assert!(error.contains("60 requests per hour"), "{}", error);
    assert!(error.contains("Try again in 12 min"), "{}", error);
}

#[test]
fn a_429_with_retry_after_names_that_wait() {
    let server = MockServer::start(|_| {
        MockReply::error(429, "slow down").with_header("Retry-After", "90")
    });
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("rate limited"),
        Err(error) => error,
    };
    assert!(error.contains("rate limit"), "{}", error);
    assert!(error.contains("Try again in 2 min"), "{}", error);
}

#[test]
fn a_rate_limit_without_a_reset_time_still_says_when_to_retry() {
    let server = MockServer::start(|_| {
        MockReply::error(403, "rate limited").with_header("X-RateLimit-Remaining", "0")
    });
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("rate limited"),
        Err(error) => error,
    };
    assert!(error.contains("Try again in about an hour"), "{}", error);
}

#[test]
fn a_plain_403_is_a_block_not_a_rate_limit() {
    let server = MockServer::start(|_| MockReply::error(403, "forbidden by proxy"));
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("blocked"),
        Err(error) => error,
    };
    assert!(error.contains("refused the request"), "{}", error);
    assert!(error.contains("proxy or firewall"), "{}", error);
    assert!(!error.contains("rate limit"), "{}", error);
}

#[test]
fn a_github_outage_is_described_as_such() {
    let server = MockServer::start(|_| MockReply::error(503, "unavailable"));
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("outage"),
        Err(error) => error,
    };
    assert!(error.contains("HTTP 503") && error.contains("Try again later"), "{}", error);
}

#[test]
fn a_cut_off_tag_list_is_reported_as_unreadable() {
    let body = tag_refs(&TAGS).to_string();
    let half = body.len() / 2;
    let server = MockServer::start(move |_| MockReply::text(200, &body).cut_at(half));
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("cut off"),
        Err(error) => error,
    };
    assert!(error.contains("could not be read"), "{}", error);
}

#[test]
fn broken_json_and_foreign_documents_are_not_a_release_list() {
    for (body, expect) in [
        ("<html>captive portal</html>", "could not be read"),
        ("{\"message\":\"Not Found\",\"documentation_url\":\"x\"}", "Not Found"),
        ("{}", "unexpected answer"),
        ("[]", "no Ollama release tags"),
        ("[{\"nope\":1},{\"ref\":7}]", "no Ollama release tags"),
        ("[{\"ref\":\"refs/tags/v0.1.32-rc1\"},{\"ref\":\"refs/tags/not-a-version\"}]", "no stable Ollama release"),
    ] {
        let body = body.to_string();
        let server = MockServer::start(move |_| MockReply::text(200, &body));
        let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
            Ok(_) => panic!("{} must not produce a catalog", expect),
            Err(error) => error,
        };
        assert!(error.contains(expect), "wanted '{}' in '{}'", expect, error);
    }
}

#[test]
fn a_long_foreign_message_is_cut_on_a_character_boundary() {
    let message = "ä".repeat(400);
    let body = json!({ "message": message }).to_string();
    let server = MockServer::start(move |_| MockReply::text(200, &body));
    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(_) => panic!("not a list"),
        Err(error) => error,
    };
    assert!(error.chars().filter(|c| *c == 'ä').count() == 160, "{}", error);
}

#[test]
fn a_catalog_where_every_release_lookup_is_rate_limited_is_an_error_not_a_list_of_dead_entries() {
    let reset = (epoch_now() + 600).to_string();
    let release_requests = Arc::new(AtomicUsize::new(0));
    let counter = release_requests.clone();
    let server = MockServer::start(move |request| {
        if request.path == "/tags" {
            return MockReply::ok(tag_refs(&TAGS));
        }
        counter.fetch_add(1, AtomicOrdering::SeqCst);
        MockReply::error(403, "rate limited")
            .with_header("X-RateLimit-Remaining", "0")
            .with_header("X-RateLimit-Reset", &reset)
    });

    let error = match fetch_online_catalog_from(&github_api(&server.base_url), &[]) {
        Ok(fetch) => panic!(
            "{} entries, all unusable, would have been cached for hours",
            fetch.versions.len()
        ),
        Err(error) => error,
    };

    assert!(error.contains("rate limit") && error.contains("Try again in 10 min"), "{}", error);
    assert_eq!(
        release_requests.load(AtomicOrdering::SeqCst),
        1,
        "after the first 403 the remaining lookups are skipped"
    );
}

#[test]
fn a_partial_catalog_is_returned_but_never_cached() {
    let reset = (epoch_now() + 600).to_string();
    let release_requests = Arc::new(AtomicUsize::new(0));
    let counter = release_requests.clone();
    let server = MockServer::start(move |request| {
        if request.path == "/tags" {
            return MockReply::ok(tag_refs(&TAGS));
        }
        let version = request.path.strip_prefix("/releases/tags/v").unwrap_or("");
        if counter.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
            MockReply::ok(release_body(version))
        } else {
            MockReply::error(429, "slow down").with_header("X-RateLimit-Reset", &reset)
        }
    });
    let scratch = Scratch::new("partial");
    let cache = scratch.path().join("online-catalog.json");

    let versions = refresh_online_catalog(
        &cache,
        &github_api(&server.base_url),
        &[],
        true,
        crate::util::now_ms(),
    )
    .expect("one release resolved: a partial list is still shown");

    assert!(versions[0].installable, "the resolved release stays usable");
    assert!(
        versions[1..].iter().all(|v| !v.installable
            && v.installable_reason.as_deref().unwrap_or("").contains("rate limit")),
        "{:?}",
        versions
            .iter()
            .map(|v| (&v.version, &v.installable_reason))
            .collect::<Vec<_>>()
    );
    assert!(!cache.exists(), "a list with gaps must not be served for 6 hours");
}

#[test]
fn a_degraded_catalog_never_hides_the_pinned_version() {
    let settings = {
        let mut settings = crate::state::Settings::default();
        settings.providers.ollama.runtime_target_version = DEFAULT_RUNTIME_VERSION.to_string();
        settings.providers.ollama.runtime_version = String::new();
        settings
    };
    for degraded in [
        Vec::new(),
        vec![unresolved_candidate("0.39.2".into(), "rate limited".into())],
    ] {
        let versions = build_version_list(&settings, &degraded, None).expect("list");
        let pinned = versions
            .iter()
            .find(|v| v.version == DEFAULT_RUNTIME_VERSION)
            .expect("the pinned version is always listed");
        assert!(pinned.installable && pinned.recommended && pinned.source == "pinned");
        assert!(!versions.iter().any(|v| v.latest), "nothing is 'latest' without a catalog");
    }
}

// ── cache ────────────────────────────────────────────────────────────────────────────────

fn cache_with(fetched_at_ms: u64, versions: &[&str]) -> OnlineCatalogCache {
    OnlineCatalogCache {
        fetched_at_ms,
        versions: versions
            .iter()
            .map(|v| OnlineRuntimeVersionCandidate {
                version: v.to_string(),
                prerelease: false,
                installable: true,
                installable_reason: None,
                release_name: None,
                release_notes_summary: None,
                released_at: None,
                size_bytes: None,
            })
            .collect(),
    }
}

#[test]
fn a_damaged_cache_file_is_treated_as_no_cache() {
    let scratch = Scratch::new("cache-damaged");
    let path = scratch.path().join("online-catalog.json");
    for content in [
        "",
        "   ",
        "{",
        "null",
        "[]",
        "{\"fetched_at_ms\":\"yesterday\",\"versions\":[]}",
        "{\"fetched_at_ms\":1,\"versions\":[{\"version\":3}]}",
        "\u{feff}garbage \u{0}\u{0}",
    ] {
        fs::write(&path, content).unwrap();
        assert!(load_catalog_cache_at(&path).is_none(), "{:?}", content);
    }
    // not even a file
    fs::remove_file(&path).unwrap();
    assert!(load_catalog_cache_at(&path).is_none());
    fs::create_dir(&path).unwrap();
    assert!(load_catalog_cache_at(&path).is_none(), "a directory in its place");
}

#[test]
fn cache_entries_that_are_not_release_versions_are_dropped_on_load() {
    let scratch = Scratch::new("cache-hostile");
    let path = scratch.path().join("online-catalog.json");
    let cache = cache_with(1, &["0.40.13", "..\\..\\evil", "a b", "", "0.40.7/../x"]);
    fs::write(&path, serde_json::to_string(&cache).unwrap()).unwrap();
    let loaded = load_catalog_cache_at(&path).expect("loads");
    assert_eq!(
        loaded.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
        vec!["0.40.13"]
    );
}

#[test]
fn freshness_rejects_empty_future_and_expired_caches() {
    let now = 10_000_000_000u64;
    let ttl = CATALOG_CACHE_TTL_MS;
    assert!(catalog_cache_is_fresh(&cache_with(now - 1000, &["0.40.13"]), now, ttl));
    // empty list: nothing to show, so it must not count as fresh
    assert!(!catalog_cache_is_fresh(&cache_with(now - 1000, &[]), now, ttl));
    // expired
    assert!(!catalog_cache_is_fresh(&cache_with(now - ttl, &["0.40.13"]), now, ttl));
    // small clock skew is tolerated, a cache from "the future" is not
    assert!(catalog_cache_is_fresh(&cache_with(now + 60_000, &["0.40.13"]), now, ttl));
    assert!(!catalog_cache_is_fresh(
        &cache_with(now + CATALOG_CLOCK_SKEW_MS + 1, &["0.40.13"]),
        now,
        ttl
    ));
    assert!(!catalog_cache_is_fresh(&cache_with(u64::MAX, &["0.40.13"]), now, ttl));
    // arithmetic at the edges
    assert!(!catalog_cache_is_fresh(&cache_with(0, &["0.40.13"]), u64::MAX, ttl));
    assert!(catalog_cache_is_fresh(&cache_with(u64::MAX, &["0.40.13"]), u64::MAX, ttl));
}

#[test]
fn a_cache_from_the_future_does_not_block_refreshing() {
    let (server, tag_requests, _) = healthy_github();
    let scratch = Scratch::new("cache-future");
    let path = scratch.path().join("online-catalog.json");
    let stale = cache_with(u64::MAX / 2, &["0.1.0"]);
    fs::write(&path, serde_json::to_string(&stale).unwrap()).unwrap();

    let versions = refresh_online_catalog(
        &path,
        &github_api(&server.base_url),
        &[],
        false,
        crate::util::now_ms(),
    )
    .expect("refresh");

    assert_eq!(tag_requests.load(AtomicOrdering::SeqCst), 1, "GitHub was asked");
    assert!(versions.iter().any(|v| v.version == "0.40.13"));
    let rewritten = load_catalog_cache_at(&path).expect("cache replaced");
    assert!(rewritten.fetched_at_ms < u64::MAX / 2, "the poisoned stamp is gone");
}

#[test]
fn a_fresh_cache_needs_no_network_and_an_empty_one_does() {
    let (server, tag_requests, _) = healthy_github();
    let scratch = Scratch::new("cache-fresh");
    let path = scratch.path().join("online-catalog.json");
    let now = crate::util::now_ms();
    fs::write(&path, serde_json::to_string(&cache_with(now - 1000, &["0.40.13"])).unwrap()).unwrap();

    let cached = refresh_online_catalog(&path, &github_api(&server.base_url), &[], false, now)
        .expect("from cache");
    assert_eq!(cached.len(), 1);
    assert_eq!(tag_requests.load(AtomicOrdering::SeqCst), 0);

    // same stamp but an empty list: refreshed
    fs::write(&path, serde_json::to_string(&cache_with(now - 1000, &[])).unwrap()).unwrap();
    let fetched = refresh_online_catalog(&path, &github_api(&server.base_url), &[], false, now)
        .expect("fetched");
    assert!(fetched.len() > 1);
    assert_eq!(tag_requests.load(AtomicOrdering::SeqCst), 1);
}

#[test]
fn a_failed_refresh_keeps_the_stale_cache_and_says_so() {
    let server = MockServer::start(|_| MockReply::error(503, "down"));
    let scratch = Scratch::new("cache-stale");
    let path = scratch.path().join("online-catalog.json");
    let now = crate::util::now_ms();
    let old = cache_with(now - CATALOG_CACHE_TTL_MS - 1000, &["0.40.13"]);
    fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();

    let error = refresh_online_catalog(&path, &github_api(&server.base_url), &[], false, now)
        .expect_err("GitHub is down");

    assert!(error.contains("Showing the last cached list"), "{}", error);
    let still = load_catalog_cache_at(&path).expect("cache untouched");
    assert_eq!(still.versions.len(), 1);
    assert_eq!(still.fetched_at_ms, old.fetched_at_ms);
}

#[test]
fn a_cache_that_cannot_be_written_does_not_fail_the_refresh() {
    let (server, _, _) = healthy_github();
    let scratch = Scratch::new("cache-readonly");
    // a directory sits where the cache file belongs: the rename must fail, the list must not
    let path = scratch.path().join("online-catalog.json");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep.txt"), "x").unwrap();

    let versions = refresh_online_catalog(
        &path,
        &github_api(&server.base_url),
        &[],
        true,
        crate::util::now_ms(),
    )
    .expect("the fetched list is returned although it could not be cached");

    assert!(versions.len() > 1);
    assert!(path.join("keep.txt").exists());
    assert_eq!(
        fs::read_dir(scratch.path()).unwrap().count(),
        1,
        "no temp file left behind"
    );

    // and a cache directory that does not exist at all
    let missing = scratch.path().join("no-such-dir").join("online-catalog.json");
    assert!(refresh_online_catalog(
        &missing,
        &github_api(&server.base_url),
        &[],
        true,
        crate::util::now_ms() + 20_000
    )
    .is_ok());
}

#[test]
fn simultaneous_refreshes_cost_github_one_listing() {
    let (server, tag_requests, release_requests) = healthy_github();
    let scratch = Scratch::new("cache-flight");
    let path = scratch.path().join("online-catalog.json");
    let api = github_api(&server.base_url);

    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    refresh_online_catalog(&path, &api, &[], false, crate::util::now_ms())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("no panic")).collect()
    });

    assert!(results.iter().all(|r| r.as_ref().map(|v| v.len() > 1).unwrap_or(false)));
    assert_eq!(tag_requests.load(AtomicOrdering::SeqCst), 1, "one listing for eight callers");
    assert!(release_requests.load(AtomicOrdering::SeqCst) <= CATALOG_MAX_MINOR_LINES);
    let first = &results[0].as_ref().unwrap();
    assert!(results.iter().all(|r| r.as_ref().unwrap().len() == first.len()));
}

#[test]
fn a_forced_refresh_right_behind_another_reuses_it() {
    let (server, tag_requests, _) = healthy_github();
    let scratch = Scratch::new("cache-force");
    let path = scratch.path().join("online-catalog.json");
    let api = github_api(&server.base_url);

    refresh_online_catalog(&path, &api, &[], true, crate::util::now_ms()).expect("first");
    refresh_online_catalog(&path, &api, &[], true, crate::util::now_ms()).expect("double click");

    assert_eq!(tag_requests.load(AtomicOrdering::SeqCst), 1);
}

#[test]
fn installed_and_selected_versions_are_always_looked_up() {
    let (server, _, release_requests) = healthy_github();
    let versions = fetch_online_catalog_from(
        &github_api(&server.base_url),
        &["0.20.2".to_string(), "v0.40.13".to_string(), "../x".to_string(), "".to_string()],
    )
    .expect("catalog")
    .versions;

    assert!(versions.iter().any(|v| v.version == "0.20.2"), "extra version resolved");
    assert_eq!(
        versions.iter().filter(|v| v.version == "0.40.13").count(),
        1,
        "no duplicate for a version already in the top lines"
    );
    assert!(!versions.iter().any(|v| v.version.contains('/')));
    assert_eq!(
        release_requests.load(AtomicOrdering::SeqCst),
        versions.len(),
        "exactly one lookup per entry"
    );
}

#[test]
fn the_install_path_gets_the_same_helpful_rate_limit_text() {
    let server = MockServer::start(|_| {
        MockReply::error(429, "slow down").with_header("Retry-After", "30")
    });
    let error = fetch_online_manifest_from(&github_api(&server.base_url), "0.38.1")
        .expect_err("rate limited");
    assert!(
        error.contains("rate limit") && error.contains("Try again in 30 s"),
        "{}",
        error
    );
    let gone = fetch_online_manifest_from(&github_api("http://127.0.0.1:1"), "0.38.1")
        .expect_err("offline");
    assert!(gone.contains("Could not reach GitHub"), "{}", gone);
}

// ───────────────────────── 2. download / install under disturbance ─────────────────────────

fn sha_of(bytes: &[u8]) -> String {
    sha256_reader(&mut std::io::Cursor::new(bytes.to_vec())).expect("hash")
}

fn manifest_for(version: &str, base: &str, body: &str) -> RuntimeManifestResolved {
    RuntimeManifestResolved {
        version: version.to_string(),
        url: format!("{}/archive.zip", base),
        sha256: sha_of(body.as_bytes()),
        release_name: None,
        release_notes_summary: None,
    }
}

fn archive_body() -> String {
    // stands in for a runtime ZIP: the download layer only streams and hashes bytes
    "PK-stand-in-archive-".repeat(5000)
}

fn download(
    manifest: &RuntimeManifestResolved,
    cache: &Path,
    max_bytes: u64,
    stall: Duration,
) -> Result<PathBuf, String> {
    download_runtime_archive(manifest, cache, max_bytes, stall, &mut |_, _, _| {})
}

fn only_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_complete_download_is_verified_cached_and_reused_without_a_request() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let cache = Scratch::new("dl-ok");
    let manifest = manifest_for("9.9.1", &server.base_url, &body);

    let mut messages: Vec<String> = Vec::new();
    let path = download_runtime_archive(
        &manifest,
        cache.path(),
        MAX_RUNTIME_ARCHIVE_BYTES,
        Duration::from_secs(5),
        &mut |message, _, _| messages.push(message.to_string()),
    )
    .expect("download");

    assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
    assert_eq!(
        only_entries(cache.path()),
        vec![
            "ollama-windows-amd64-v9.9.1.json".to_string(),
            "ollama-windows-amd64-v9.9.1.zip".to_string()
        ],
        "archive + sidecar, no .part"
    );
    assert!(messages.iter().any(|m| m.contains("checksum verified")), "{:?}", messages);

    // second call: the cached archive still matches, so no request is made
    let before = server.requests().len();
    download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect("cached");
    assert_eq!(server.requests().len(), before);
}

#[test]
fn a_download_cut_off_mid_stream_leaves_nothing_behind() {
    let body = archive_body();
    let served = body.clone();
    let half = body.len() / 2;
    let server = MockServer::start(move |_| MockReply::text(200, &served).cut_at(half));
    let cache = Scratch::new("dl-cut");
    let manifest = manifest_for("9.9.2", &server.base_url, &body);

    let error = download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect_err("half an archive must not pass");

    assert!(
        error.contains("closed before the download finished")
            || error.contains("ended early")
            || error.contains("broke during the download"),
        "{}",
        error
    );
    assert_eq!(
        only_entries(cache.path()),
        Vec::<String>::new(),
        "no .zip.part, no archive, no sidecar"
    );
}

#[test]
fn a_connection_that_went_silent_ends_the_download_instead_of_hanging() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| {
        MockReply::text(200, &served)
            .cut_at(1000)
            .then_stall(Duration::from_secs(8))
    });
    let cache = Scratch::new("dl-stall");
    let manifest = manifest_for("9.9.3", &server.base_url, &body);

    let started = Instant::now();
    let error = download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_millis(700))
        .expect_err("a dead connection must end");

    assert!(started.elapsed() < Duration::from_secs(6), "{:?}", started.elapsed());
    assert!(error.contains("stalled"), "{}", error);
    assert_eq!(only_entries(cache.path()), Vec::<String>::new());
}

#[test]
fn a_wrong_checksum_removes_the_archive_and_says_so() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let cache = Scratch::new("dl-sha");
    let mut manifest = manifest_for("9.9.4", &server.base_url, &body);
    manifest.sha256 = "0".repeat(64);

    let error = download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect_err("checksum mismatch");

    assert!(error.contains("checksum mismatch"), "{}", error);
    assert!(error.contains(&"0".repeat(64)) && error.contains(&sha_of(body.as_bytes())), "{}", error);
    assert_eq!(only_entries(cache.path()), Vec::<String>::new());
}

#[test]
fn a_cached_archive_with_other_content_is_replaced_not_trusted() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let cache = Scratch::new("dl-badcache");
    let manifest = manifest_for("9.9.5", &server.base_url, &body);
    fs::write(cache.path().join("ollama-windows-amd64-v9.9.5.zip"), b"tampered").unwrap();
    fs::write(cache.path().join("ollama-windows-amd64-v9.9.5.json"), b"{}").unwrap();

    let path = download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect("redownloaded");

    assert_eq!(fs::read(path).unwrap(), body.as_bytes());
}

#[test]
fn a_leftover_part_file_from_a_killed_download_is_simply_overwritten() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let cache = Scratch::new("dl-leftover");
    let manifest = manifest_for("9.9.6", &server.base_url, &body);
    // garbage that is longer than the real archive: a truncating create must cope with it
    fs::write(
        cache.path().join("ollama-windows-amd64-v9.9.6.zip.part"),
        vec![7u8; body.len() * 2],
    )
    .unwrap();

    let path = download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect("download over a stale .part");

    assert_eq!(fs::read(path).unwrap(), body.as_bytes());
    assert!(!only_entries(cache.path()).iter().any(|n| n.ends_with(".part")));
}

#[test]
fn stale_part_files_are_swept_young_ones_and_archives_stay() {
    let cache = Scratch::new("dl-sweep");
    fs::write(cache.path().join("ollama-windows-amd64-v1.0.0.zip.part"), b"x").unwrap();
    fs::write(cache.path().join("ollama-windows-amd64-v1.0.0.zip"), b"keep").unwrap();
    fs::write(cache.path().join("online-catalog.json"), b"keep").unwrap();

    cleanup_stale_download_parts(cache.path(), Duration::from_secs(3600));
    assert!(cache.path().join("ollama-windows-amd64-v1.0.0.zip.part").exists(), "young");

    std::thread::sleep(Duration::from_millis(50));
    cleanup_stale_download_parts(cache.path(), Duration::from_millis(10));
    assert_eq!(
        only_entries(cache.path()),
        vec!["ollama-windows-amd64-v1.0.0.zip".to_string(), "online-catalog.json".to_string()]
    );
}

#[test]
fn download_server_failures_are_described() {
    let cache = Scratch::new("dl-http");
    let body = archive_body();

    let gone = MockServer::start(|_| MockReply::error(404, "no such asset"));
    let error = download(&manifest_for("9.9.7", &gone.base_url, &body), cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect_err("404");
    assert!(error.contains("HTTP 404") && error.contains("removed"), "{}", error);

    let error = download(&manifest_for("9.9.8", "http://127.0.0.1:1", &body), cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect_err("nothing there");
    assert!(error.contains("could not reach the download server"), "{}", error);
    assert_eq!(only_entries(cache.path()), Vec::<String>::new());
}

#[test]
fn an_archive_over_the_size_limit_is_refused_before_a_byte_is_written() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let cache = Scratch::new("dl-big");
    let manifest = manifest_for("9.9.9", &server.base_url, &body);

    let error = download(&manifest, cache.path(), 1000, Duration::from_secs(5))
        .expect_err("announced size exceeds the limit");

    assert!(error.contains("safety limit"), "{}", error);
    assert_eq!(only_entries(cache.path()), Vec::<String>::new());
}

/// A reader that never ends, like a hijacked or buggy server without a Content-Length.
struct Endless;

impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(1);
        Ok(buf.len())
    }
}

#[test]
fn an_endless_body_is_cut_at_the_limit() {
    let mut sink = std::io::sink();
    match stream_download(&mut Endless, &mut sink, 1024 * 1024, &mut |_| {}) {
        Err(StreamError::TooLarge) => {}
        _ => panic!("the stream must stop at the limit"),
    }
}

/// Accepts `room` bytes, then fails like a full disk.
struct FullDisk {
    room: usize,
}

impl Write for FullDisk {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.room == 0 {
            return Err(std::io::Error::from_raw_os_error(112)); // ERROR_DISK_FULL
        }
        let n = buf.len().min(self.room);
        self.room -= n;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_full_disk_is_reported_as_such_everywhere_bytes_are_written() {
    let data = vec![1u8; 600_000];
    let mut reader = std::io::Cursor::new(data.clone());
    let error = match stream_download(&mut reader, &mut FullDisk { room: 300_000 }, u64::MAX, &mut |_| {}) {
        Err(StreamError::Write(e)) => e,
        _ => panic!("the write failure must surface as a write error"),
    };
    assert!(is_disk_full(&error));
    let message = describe_write_error("write the runtime archive to disk", &error);
    assert!(message.contains("Not enough free disk space"), "{}", message);

    // extraction copies through the same kind of writer
    let mut reader = std::io::Cursor::new(data);
    let error = copy_with_background_throttle(&mut reader, &mut FullDisk { room: 10 })
        .expect_err("disk full while unpacking");
    assert!(is_disk_full(&error));

    // other write failures keep their own text
    let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    assert!(!is_disk_full(&other));
    assert!(describe_write_error("create the download file", &other).contains("access denied"));
    assert!(is_disk_full(&std::io::Error::from(std::io::ErrorKind::StorageFull)));
}

#[test]
fn a_download_that_cannot_create_its_file_fails_cleanly() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| MockReply::text(200, &served));
    let scratch = Scratch::new("dl-nodir");
    let manifest = manifest_for("9.8.1", &server.base_url, &body);

    // the cache "directory" is a file: creating `.zip.part` inside it must fail without panic
    let not_a_dir = scratch.path().join("cache");
    fs::write(&not_a_dir, b"i am a file").unwrap();
    let error = download(&manifest, &not_a_dir, MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect_err("no place to write");
    assert!(error.contains("download file"), "{}", error);
}

#[test]
fn two_downloads_of_one_version_never_share_a_part_file() {
    let body = archive_body();
    let served = body.clone();
    let server = MockServer::start(move |_| {
        MockReply::text(200, &served).delayed(Duration::from_millis(800))
    });
    let cache = Scratch::new("dl-race");
    let manifest = manifest_for("9.8.2", &server.base_url, &body);

    let results: Vec<Result<PathBuf, String>> = std::thread::scope(|scope| {
        let first = scope.spawn(|| download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(200));
        let second = scope.spawn(|| download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5)));
        vec![first.join().unwrap(), second.join().unwrap()]
    });

    assert!(results[0].is_ok(), "the first download completes: {:?}", results[0]);
    let refusal = results[1].as_ref().expect_err("the second must be refused");
    assert!(refusal.contains("already running"), "{}", refusal);
    assert_eq!(fs::read(results[0].as_ref().unwrap()).unwrap(), body.as_bytes());
    // the lock is released afterwards
    download(&manifest, cache.path(), MAX_RUNTIME_ARCHIVE_BYTES, Duration::from_secs(5))
        .expect("a later download works again");
}

#[test]
fn the_install_slot_is_exclusive_and_released_by_drop() {
    let first = RuntimeJob::begin("install-test".into(), "busy".into()).expect("free");
    assert_eq!(
        RuntimeJob::begin("install-test".into(), "busy".into()).err().as_deref(),
        Some("busy")
    );
    // another job is independent
    assert!(RuntimeJob::begin("install-test-other".into(), "busy".into()).is_ok());
    drop(first);
    assert!(RuntimeJob::begin("install-test".into(), "busy".into()).is_ok());
}

#[test]
fn a_poisoned_job_list_does_not_wedge_installs() {
    let result = std::thread::spawn(|| {
        let _guard = RUNTIME_JOBS.lock().unwrap();
        panic!("a thread died while holding the job list");
    })
    .join();
    assert!(result.is_err());
    assert!(RUNTIME_JOBS.is_poisoned());
    let job = RuntimeJob::begin("poison-test".into(), "busy".into());
    assert!(job.is_ok(), "the poisoned lock is recovered, not propagated");
}

// ── archives ─────────────────────────────────────────────────────────────────────────────

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        writer.start_file(*name, options).expect("start entry");
        writer.write_all(data).expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn install_bytes(
    bytes: Vec<u8>,
    root: &Path,
    version: &str,
    verify: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    install_archive(
        std::io::Cursor::new(bytes),
        root,
        version,
        MAX_RUNTIME_EXTRACTED_BYTES,
        verify,
        &mut |_, _| {},
    )
}

#[test]
fn a_cut_off_archive_is_rejected_before_anything_is_written() {
    let root = Scratch::new("zip-cut");
    marker(&root.path().join("0.40.13"), "previous");
    let mut bytes = zip_bytes(&[("ollama.exe", &[7u8; 4096]), ("lib/a.dll", &[8u8; 4096])]);
    bytes.truncate(bytes.len() * 6 / 10);

    let error = install_bytes(bytes, root.path(), "0.40.13", &|_| Ok(())).expect_err("truncated zip");

    assert!(error.contains("Invalid runtime ZIP archive"), "{}", error);
    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
}

#[test]
fn something_that_is_not_a_zip_is_rejected() {
    let root = Scratch::new("zip-garbage");
    for bytes in [Vec::new(), b"PK".to_vec(), b"<html>404 not found</html>".to_vec(), vec![0u8; 100_000]] {
        let error = install_bytes(bytes, root.path(), "0.40.13", &|_| Ok(())).expect_err("not a zip");
        assert!(error.contains("Invalid runtime ZIP archive"), "{}", error);
    }
    assert_eq!(only_entries(root.path()), Vec::<String>::new());
}

#[test]
fn a_corrupted_entry_aborts_the_install_and_keeps_the_previous_one() {
    let root = Scratch::new("zip-crc");
    marker(&root.path().join("0.40.13"), "previous");
    let payload = vec![b'A'; 8192];
    let mut bytes = zip_bytes(&[("ollama.exe", &payload)]);
    // flip one byte inside the stored data: the directory is intact, the CRC no longer fits
    let at = bytes
        .windows(64)
        .position(|w| w.iter().all(|b| *b == b'A'))
        .expect("stored data present")
        + 1000;
    bytes[at] ^= 0xFF;

    let error = install_bytes(bytes, root.path(), "0.40.13", &|_| Ok(())).expect_err("corrupt data");

    assert!(error.contains("Failed to extract"), "{}", error);
    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new(), "staging removed");
}

#[test]
fn an_extraction_that_fails_half_way_cleans_up_like_a_full_disk_would() {
    // entry 1 is a file, entry 2 wants a folder of the same name: the second write fails after
    // the first one succeeded, the situation a full disk creates in the middle of unpacking
    let root = Scratch::new("zip-half");
    marker(&root.path().join("0.40.13"), "previous");
    let bytes = zip_bytes(&[("ollama.exe", b"binary"), ("lib", b"a file"), ("lib/blocked.dll", b"x")]);

    let error = install_bytes(bytes, root.path(), "0.40.13", &|_| Ok(())).expect_err("half way");

    assert!(error.contains("Failed to create"), "{}", error);
    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
}

#[test]
fn an_archive_that_fails_verification_never_replaces_anything() {
    let root = Scratch::new("zip-verify");
    marker(&root.path().join("0.40.13"), "previous");
    let bytes = zip_bytes(&[("ollama.exe", b"new")]);

    let error = install_bytes(bytes, root.path(), "0.40.13", &|_| Err("signature mismatch".into()))
        .expect_err("rejected");

    assert_eq!(error, "signature mismatch");
    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
}

#[test]
fn a_sidecar_that_claims_another_checksum_is_not_trusted() {
    let scratch = Scratch::new("sidecar");
    let archive = scratch.path().join("download.zip");
    fs::write(&archive, b"archive bytes").unwrap();
    let digest = sha_of(b"archive bytes");
    // the sidecar describes some other archive (its hash differs from the file's)
    fs::write(
        runtime_archive_metadata_path(&archive),
        serde_json::to_string(&RuntimeArchiveMetadata {
            version: "0.40.99".into(),
            sha256: "f".repeat(64),
            url: "https://example.invalid/x".into(),
            release_name: None,
            release_notes_summary: None,
        })
        .unwrap(),
    )
    .unwrap();

    // not the sidecar, not a pinned checksum, no version in the file name: refused offline
    let error = resolve_runtime_archive_manifest_for_install(&archive, &digest)
        .expect_err("unknown archive");
    assert!(error.contains("not in the pinned runtime manifest"), "{}", error);

    // and a sidecar that does match is what makes an unpinned archive installable
    fs::write(
        runtime_archive_metadata_path(&archive),
        serde_json::to_string(&RuntimeArchiveMetadata {
            version: "0.40.99".into(),
            sha256: digest.clone(),
            url: "https://github.com/ollama/ollama/releases/download/v0.40.99/ollama-windows-amd64.zip".into(),
            release_name: None,
            release_notes_summary: None,
        })
        .unwrap(),
    )
    .unwrap();
    let manifest = resolve_runtime_archive_manifest_for_install(&archive, &digest).expect("sidecar");
    assert_eq!(manifest.version, "0.40.99");
    // a pinned checksum always resolves to the pinned version, whatever the sidecar says
    let pinned = resolve_pinned_manifest(DEFAULT_RUNTIME_VERSION).unwrap();
    let from_pin = resolve_runtime_archive_manifest_for_install(&scratch.path().join("x.zip"), &pinned.sha256)
        .expect("pinned checksum");
    assert_eq!(from_pin.version, DEFAULT_RUNTIME_VERSION);
}

// ── crashes in the middle of an install ──────────────────────────────────────────────────

#[test]
fn a_runtime_that_died_between_unpacking_and_the_swap_never_counts_as_installed() {
    let root = Scratch::new("crash-staging");
    marker(&root.path().join("0.30.4"), "installed");
    // the process died after unpacking, before the swap: a complete-looking staging folder
    marker(&root.path().join(".staging-0.40.13-1730000000000000"), "unverified");
    fs::create_dir_all(root.path().join(".staging-0.40.13-1730000000000000").join("lib")).unwrap();

    let found = scan_installed_runtime_versions(root.path());
    assert_eq!(found.keys().cloned().collect::<Vec<_>>(), vec!["0.30.4".to_string()]);

    let mut settings = crate::state::Settings::default();
    settings.providers.ollama.runtime_target_version = "0.40.13".to_string();
    settings.providers.ollama.runtime_version = String::new();
    let list = build_version_list(&settings, &[], Some(root.path())).expect("list");
    let staged = list.iter().find(|v| v.version == "0.40.13").expect("pinned entry");
    assert!(!staged.installed, "a staging folder is not an install");
    assert!(list.iter().find(|v| v.version == "0.30.4").unwrap().installed);
    assert!(
        !list.iter().any(|v| v.version.contains("staging")),
        "leftover folders are not listed as versions"
    );

    // and selecting the target does not launch it
    let (binary, _) = select_runtime_binary_in(&settings, Some(root.path()))
        .expect("falls back to an installed runtime");
    assert_eq!(binary, root.path().join("0.30.4").join("ollama.exe"));
}

#[test]
fn a_crash_between_the_two_renames_is_healed_on_the_next_cleanup() {
    let root = Scratch::new("crash-renames");
    // the old install was moved aside, the new one never arrived: no 0.40.13 folder at all
    marker(&root.path().join(".old-0.40.13-1730000000000005"), "previous-good");
    marker(&root.path().join(".old-0.40.13-1730000000000001"), "older leftover");
    marker(&root.path().join(".staging-0.40.13-1730000000000009"), "unverified new");
    assert!(scan_installed_runtime_versions(root.path()).is_empty());

    cleanup_stale_runtime_dirs(root.path());

    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous-good", "newest .old wins");
    assert!(scan_installed_runtime_versions(root.path()).contains_key("0.40.13"));
    // the unverified staging folder is never promoted
    assert_eq!(read_marker(&root.path().join(".staging-0.40.13-1730000000000009")), "unverified new");
}

#[test]
fn healing_never_overwrites_an_install_and_ignores_foreign_folder_names() {
    let root = Scratch::new("crash-guard");
    marker(&root.path().join("0.40.7"), "installed");
    marker(&root.path().join(".old-0.40.7-1730000000000005"), "stale backup");
    for hostile in [".old-..\\evil-1730000000000000", ".old--5", ".old-0.40.13-abc", ".old-0.40.13", ".old-a b-1"] {
        // some of these names are not even creatable on Windows; that is fine
        let _ = fs::create_dir_all(root.path().join(hostile).join("lib"));
        let _ = fs::write(root.path().join(hostile).join("ollama.exe"), "x");
    }
    // an .old folder without a binary is not worth restoring
    fs::create_dir_all(root.path().join(".old-0.39.0-1730000000000000")).unwrap();

    let restored = recover_interrupted_swaps(root.path());

    assert!(restored.is_empty(), "{:?}", restored);
    assert_eq!(read_marker(&root.path().join("0.40.7")), "installed");
    assert!(!root.path().join("0.39.0").exists());
    assert!(!root.path().join("0.40.13").exists());
}

#[test]
fn healing_runs_before_old_leftovers_are_deleted_by_age() {
    // a leftover older than the age limit would be deleted: the runtime it holds must be
    // restored first, otherwise the crash costs the user their only working runtime
    let root = Scratch::new("crash-order");
    marker(&root.path().join(".old-0.40.13-1"), "previous-good");
    std::thread::sleep(Duration::from_millis(50));

    cleanup_stale_runtime_dirs_older_than(root.path(), Duration::from_millis(10));

    assert_eq!(read_marker(&root.path().join("0.40.13")), "previous-good");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
}

#[test]
fn staging_and_backup_names_are_unique_per_call() {
    let mut stamps: Vec<u64> = (0..2000).map(|_| unique_install_stamp()).collect();
    let before = stamps.len();
    stamps.sort_unstable();
    stamps.dedup();
    assert_eq!(stamps.len(), before, "two installs in the same millisecond must not collide");
    assert!(is_safe_runtime_version(&format!("0.40.13-{}", stamps[0])), "names stay parseable");
}

#[test]
fn two_installs_racing_for_one_version_leave_one_whole_runtime_and_no_leftovers() {
    // Defence in depth: the install job already refuses a second install, but if two ever
    // run, the target must hold exactly one complete runtime, never a mix of both.
    for round in 0..4 {
        let root = Scratch::new(&format!("race-{}", round));
        marker(&root.path().join("0.40.13"), "previous");
        let archive_a = zip_bytes(&[("ollama.exe", b"runtime-a"), ("lib/x.dll", b"dll-a"), ("lib/y.dll", b"dll-a")]);
        let archive_b = zip_bytes(&[("ollama.exe", b"runtime-b"), ("lib/x.dll", b"dll-b"), ("lib/y.dll", b"dll-b")]);

        let outcomes: Vec<Result<PathBuf, String>> = std::thread::scope(|scope| {
            let a = scope.spawn(|| install_bytes(archive_a.clone(), root.path(), "0.40.13", &|_| Ok(())));
            let b = scope.spawn(|| install_bytes(archive_b.clone(), root.path(), "0.40.13", &|_| Ok(())));
            vec![a.join().expect("no panic"), b.join().expect("no panic")]
        });

        assert!(outcomes.iter().any(|o| o.is_ok()), "{:?}", outcomes);
        let target = root.path().join("0.40.13");
        let exe = read_marker(&target);
        let letter = exe.trim_start_matches("runtime-").to_string();
        assert!(letter == "a" || letter == "b", "target holds {:?}", exe);
        for dll in ["x.dll", "y.dll"] {
            assert_eq!(
                fs::read_to_string(target.join("lib").join(dll)).unwrap(),
                format!("dll-{}", letter),
                "a mixed runtime ({} from the other archive)",
                dll
            );
        }
        // A racer that lost keeps the previous install in `.old-*` (data first); what must never
        // remain is an unpacked `.staging-*` folder.
        let leftovers = dot_entries(root.path());
        assert!(
            leftovers.iter().all(|name| name.starts_with(".old-")),
            "round {}: {:?}",
            round,
            leftovers
        );
        if outcomes.iter().all(|o| o.is_ok()) {
            assert!(leftovers.is_empty(), "both succeeded, round {}: {:?}", round, leftovers);
        }
    }
}

#[test]
fn the_version_that_is_serving_cannot_be_replaced_and_the_user_is_told_up_front() {
    let said = install_blocked_by_running_server("0.40.13", Some("0.40.13"), true).expect("blocked");
    assert!(said.contains("running right now") && said.contains("0.40.13"), "{}", said);
    // another version, an unreadable version, or an external server never block
    assert!(install_blocked_by_running_server("0.40.13", Some("0.30.4"), true).is_none());
    assert!(install_blocked_by_running_server("0.40.13", None, true).is_none());
    assert!(install_blocked_by_running_server("0.40.13", Some("0.40.13"), false).is_none());
}

#[cfg(target_os = "windows")]
#[test]
fn replacing_a_runtime_whose_exe_is_open_fails_cleanly_through_the_whole_pipeline() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = Scratch::new("in-use");
    marker(&root.path().join("0.40.13"), "serving");
    // like a running ollama.exe: an open handle that forbids rename and delete
    let running = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(root.path().join("0.40.13").join("ollama.exe"))
        .expect("hold the binary open");

    let error = install_bytes(zip_bytes(&[("ollama.exe", b"new")]), root.path(), "0.40.13", &|_| Ok(()))
        .expect_err("cannot swap while it runs");

    assert!(error.contains("is in use"), "{}", error);
    assert_eq!(dot_entries(root.path()), Vec::<String>::new(), "staging removed again");
    drop(running);
    assert_eq!(read_marker(&root.path().join("0.40.13")), "serving");
    install_bytes(zip_bytes(&[("ollama.exe", b"new")]), root.path(), "0.40.13", &|_| Ok(()))
        .expect("works once it stopped");
    assert_eq!(read_marker(&root.path().join("0.40.13")), "new");
}

// ───────────────────────── 3. start / version switch under failure ─────────────────────────

#[derive(Default)]
struct FakeWorld {
    reachable: bool,
    running_version: Option<String>,
    managed_alive: bool,
    spawn_age: Option<Duration>,
    port_in_use: bool,
    port_frees_after_stop: bool,
    /// What `launch` returns per binary (default: `Ready`).
    script: HashMap<PathBuf, LaunchOutcome>,
    /// Version a successfully launched binary serves.
    versions: HashMap<PathBuf, String>,
    launch_delay: Duration,
    await_outcome: Option<LaunchOutcome>,
    /// A hung Ollama of this app (pid file of a crashed session) holds the port; reclaiming
    /// kills it.
    stale_holder: bool,
    // what happened
    launches: Vec<PathBuf>,
    stops: usize,
    awaits: usize,
    reclaims: usize,
}

#[derive(Clone)]
struct FakeHost(Arc<Mutex<FakeWorld>>);

impl FakeHost {
    fn new(world: FakeWorld) -> Self {
        Self(Arc::new(Mutex::new(world)))
    }

    fn world(&self) -> std::sync::MutexGuard<'_, FakeWorld> {
        self.0.lock().unwrap()
    }
}

impl StartHost for FakeHost {
    fn endpoint_reachable(&self) -> bool {
        self.world().reachable
    }
    fn running_version(&self) -> Option<String> {
        self.world().running_version.clone()
    }
    fn managed_alive(&self) -> bool {
        self.world().managed_alive
    }
    fn managed_spawn_age(&self) -> Option<Duration> {
        self.world().spawn_age
    }
    fn port_in_use(&self) -> bool {
        self.world().port_in_use
    }
    fn endpoint_label(&self) -> String {
        "127.0.0.1:11434".to_string()
    }
    fn reclaim_stale_port(&mut self) -> bool {
        let mut world = self.world();
        world.reclaims += 1;
        if world.stale_holder {
            world.stale_holder = false;
            world.port_in_use = false;
        }
        !world.port_in_use
    }
    fn stop_managed_and_free_port(&mut self) -> bool {
        let mut world = self.world();
        world.stops += 1;
        world.managed_alive = false;
        world.reachable = false;
        world.running_version = None;
        if world.port_frees_after_stop {
            world.port_in_use = false;
        }
        !world.port_in_use
    }
    fn await_starting_server(&mut self) -> LaunchOutcome {
        let mut world = self.world();
        world.awaits += 1;
        world.await_outcome.clone().unwrap_or(LaunchOutcome::Failed {
            stage: "stopped",
            message: "not scripted".into(),
        })
    }
    fn launch(&mut self, binary: &Path) -> LaunchOutcome {
        let delay = self.world().launch_delay;
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        let mut world = self.world();
        world.launches.push(binary.to_path_buf());
        let outcome = world.script.get(binary).cloned().unwrap_or(LaunchOutcome::Ready {
            pid: 100 + world.launches.len() as u32,
            waited: Duration::from_millis(5),
        });
        if matches!(outcome, LaunchOutcome::Ready { .. } | LaunchOutcome::Pending { .. }) {
            world.managed_alive = true;
            world.reachable = matches!(outcome, LaunchOutcome::Ready { .. });
            world.running_version = world.versions.get(binary).cloned();
        }
        outcome
    }
}

fn bin(version: &str) -> PathBuf {
    PathBuf::from(format!("C:\\rt\\{}\\ollama.exe", version))
}

fn ready(pid: u32) -> LaunchOutcome {
    LaunchOutcome::Ready { pid, waited: Duration::from_millis(5) }
}

fn failed(stage: &'static str, message: &str) -> LaunchOutcome {
    LaunchOutcome::Failed { stage, message: message.to_string() }
}

/// The scenario "managed server on `running` is serving; the user selected `wanted`".
fn switching_world(running: &str) -> FakeWorld {
    FakeWorld {
        reachable: true,
        running_version: Some(running.to_string()),
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(600)),
        port_in_use: true,
        port_frees_after_stop: true,
        ..Default::default()
    }
}

fn no_previous(_: &str) -> Option<PathBuf> {
    None
}

fn previous_of(running: &str) -> Option<PathBuf> {
    Some(bin(running))
}

#[test]
fn a_start_that_finds_a_matching_server_launches_nothing() {
    let host = FakeHost::new(FakeWorld {
        reachable: true,
        running_version: Some("0.40.13".into()),
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(5)),
        port_in_use: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert_eq!(
        resolution,
        StartResolution::AlreadyRunning { running_version: "0.40.13".into() }
    );
    assert!(host.world().launches.is_empty() && host.world().stops == 0);
}

#[test]
fn a_foreign_server_is_left_alone_even_when_our_own_child_died() {
    // our child is dead, something else (another Ollama) answers on the port, other version
    let host = FakeHost::new(FakeWorld {
        reachable: true,
        running_version: Some("0.30.4".into()),
        managed_alive: false,
        port_in_use: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert_eq!(
        resolution,
        StartResolution::AlreadyRunning { running_version: "0.30.4".into() }
    );
    let world = host.world();
    assert!(world.launches.is_empty(), "never spawn on top of a foreign server");
    assert_eq!(world.stops, 0, "never stop what is not ours");
}

#[test]
fn a_slow_version_endpoint_never_triggers_a_switch() {
    // /api/tags answers, /api/version times out: the version is unknown, so no kill
    let host = FakeHost::new(FakeWorld {
        reachable: true,
        running_version: None,
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(600)),
        port_in_use: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert_eq!(
        resolution,
        StartResolution::AlreadyRunning { running_version: "0.40.13".into() }
    );
    assert_eq!(host.world().stops, 0);
}

#[test]
fn a_foreign_program_on_the_port_fails_fast_with_a_clear_message() {
    // nothing of ours runs, nothing answers like Ollama, but the port accepts connections
    let host = FakeHost::new(FakeWorld {
        reachable: false,
        managed_alive: false,
        port_in_use: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    match resolution {
        StartResolution::Failed { stage, message } => {
            assert_eq!(stage, "port_in_use");
            assert!(
                message.contains("127.0.0.1:11434") && message.contains("does not answer like Ollama"),
                "{}",
                message
            );
        }
        other => panic!("expected a clear failure, got {:?}", other),
    }
    assert!(host.world().launches.is_empty(), "a second ollama serve would only die on the port");
    assert_eq!(host.world().reclaims, 1, "a stale Ollama of ours is looked for before the port counts as foreign");
}

#[test]
fn a_hung_leftover_of_a_crashed_session_is_cleared_away_before_the_start() {
    // The previous session died and left an `ollama serve` that holds the port without
    // answering. Before the port is called foreign, the pid file's process is killed: as before.
    let host = FakeHost::new(FakeWorld {
        reachable: false,
        managed_alive: false,
        port_in_use: true,
        stale_holder: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Ready { .. })), "{:?}", resolution);
    assert_eq!(host.world().reclaims, 1);
    assert_eq!(host.world().launches, vec![bin("0.40.13")]);
}

#[test]
fn a_free_port_with_nothing_running_just_spawns() {
    let host = FakeHost::new(FakeWorld::default());
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Ready { .. })), "{:?}", resolution);
    assert_eq!(host.world().launches, vec![bin("0.40.13")]);
}

#[test]
fn a_switch_stops_the_old_server_before_it_starts_the_new_one() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().versions.insert(bin("0.40.13"), "0.40.13".into());
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Ready { .. })), "{:?}", resolution);
    let world = host.world();
    assert_eq!(world.stops, 1);
    assert_eq!(world.launches, vec![bin("0.40.13")]);
    assert_eq!(world.running_version.as_deref(), Some("0.40.13"));
}

#[test]
fn a_switch_that_fails_to_start_goes_back_to_the_previous_runtime() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().script.insert(bin("0.40.13"), failed("spawn_exited", "Ollama exited right after it was started (exit code 1)."));
    host.world().versions.insert(bin("0.30.4"), "0.30.4".into());

    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);

    match resolution {
        StartResolution::FellBack { outcome, binary, previous_version, message } => {
            assert!(matches!(outcome, LaunchOutcome::Ready { .. }));
            assert_eq!(binary, bin("0.30.4"));
            assert_eq!(previous_version, "0.30.4");
            assert!(message.contains("Could not start Ollama 0.40.13"), "{}", message);
            assert!(message.contains("exit code 1"), "{}", message);
            assert!(message.contains("previous runtime 0.30.4 was started again"), "{}", message);
        }
        other => panic!("the user must not be left without a runtime: {:?}", other),
    }
    let world = host.world();
    assert_eq!(world.launches, vec![bin("0.40.13"), bin("0.30.4")], "new first, then back");
    assert_eq!(world.stops, 1, "the old server was stopped once, for the switch");
    assert!(world.reachable && world.running_version.as_deref() == Some("0.30.4"));
}

#[test]
fn a_spawn_error_on_the_new_runtime_falls_back_too() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().script.insert(bin("0.40.13"), failed("spawn_failed", "Access is denied. (os error 5)"));
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert!(matches!(resolution, StartResolution::FellBack { .. }), "{:?}", resolution);
}

#[test]
fn a_failing_switch_without_a_known_previous_runtime_says_so_plainly() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().script.insert(bin("0.40.13"), failed("spawn_exited", "boom"));
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    match resolution {
        StartResolution::Failed { stage, message } => {
            assert_eq!(stage, "spawn_exited");
            assert!(message.contains("boom"), "{}", message);
        }
        other => panic!("{:?}", other),
    }
    assert_eq!(host.world().launches, vec![bin("0.40.13")], "nothing to go back to");
}

#[test]
fn when_the_previous_runtime_cannot_start_again_both_reasons_are_reported() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().script.insert(bin("0.40.13"), failed("spawn_exited", "exit code 1"));
    host.world().script.insert(bin("0.30.4"), failed("spawn_failed", "file not found"));
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    match resolution {
        StartResolution::Failed { stage, message } => {
            assert_eq!(stage, "version_switch_failed");
            assert!(message.contains("exit code 1") && message.contains("file not found"), "{}", message);
            assert!(message.contains("Start Ollama manually"), "{}", message);
        }
        other => panic!("{:?}", other),
    }
    assert_eq!(host.world().launches.len(), 2, "exactly one retry, no loop");
}

#[test]
fn a_switch_that_cannot_free_the_port_launches_nothing() {
    let mut world = switching_world("0.30.4");
    world.port_frees_after_stop = false; // the old server refuses to die
    let host = FakeHost::new(world);
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    match resolution {
        StartResolution::Failed { stage, message } => {
            assert_eq!(stage, "version_switch_failed");
            assert!(message.contains("still bound to 127.0.0.1:11434"), "{}", message);
        }
        other => panic!("{:?}", other),
    }
    assert!(host.world().launches.is_empty());
}

#[test]
fn a_new_runtime_that_is_merely_slow_is_neither_failure_nor_fallback() {
    let host = FakeHost::new(switching_world("0.30.4"));
    host.world().script.insert(
        bin("0.40.13"),
        LaunchOutcome::Pending { pid: 7, waited: Duration::from_secs(12) },
    );
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Pending { pid: 7, .. })), "{:?}", resolution);
    assert_eq!(host.world().launches, vec![bin("0.40.13")], "the previous runtime is not started on top");
}

#[test]
fn going_back_to_the_runtime_that_just_failed_is_not_attempted() {
    let host = FakeHost::new(switching_world("0.40.13"));
    host.world().script.insert(bin("0.40.13"), failed("spawn_exited", "boom"));
    // the selected and the previous binary are the same file (a damaged runtime)
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.30.4", &|_| Some(bin("0.40.13")));
    assert!(matches!(resolution, StartResolution::Failed { .. }), "{:?}", resolution);
    assert_eq!(host.world().launches.len(), 1);
}

#[test]
fn another_program_taking_the_port_during_launch_counts_as_running_not_as_failure() {
    let host = FakeHost::new(FakeWorld::default());
    host.world().script.insert(bin("0.40.13"), LaunchOutcome::TakenOver);
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert_eq!(resolution, StartResolution::Launched(LaunchOutcome::TakenOver));
}

#[test]
fn a_young_silent_server_of_ours_is_awaited_and_not_killed() {
    let host = FakeHost::new(FakeWorld {
        reachable: false,
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(20)),
        port_in_use: true,
        await_outcome: Some(ready(55)),
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &previous_of);
    assert_eq!(resolution, StartResolution::Launched(ready(55)));
    let world = host.world();
    assert_eq!(world.awaits, 1);
    assert!(world.launches.is_empty() && world.stops == 0, "a slow start must not be restarted from zero");
}

#[test]
fn a_silent_server_of_ours_that_stays_silent_is_pending_not_restarted() {
    let pending = LaunchOutcome::Pending { pid: 55, waited: Duration::from_secs(12) };
    let host = FakeHost::new(FakeWorld {
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(40)),
        port_in_use: true,
        await_outcome: Some(pending.clone()),
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert_eq!(resolution, StartResolution::Launched(pending));
    assert!(host.world().launches.is_empty());
}

#[test]
fn a_server_of_ours_that_never_answered_for_ages_is_replaced() {
    let host = FakeHost::new(FakeWorld {
        reachable: false,
        managed_alive: true,
        spawn_age: Some(STARTING_GRACE + Duration::from_secs(1)),
        port_in_use: true,
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Ready { .. })), "{:?}", resolution);
    assert_eq!(host.world().awaits, 0);
    assert_eq!(host.world().launches.len(), 1);
}

#[test]
fn an_awaited_server_that_died_is_replaced_by_a_fresh_start() {
    let host = FakeHost::new(FakeWorld {
        reachable: false,
        managed_alive: true,
        spawn_age: Some(Duration::from_secs(20)),
        port_in_use: true,
        await_outcome: Some(failed("spawn_exited", "died while we waited")),
        ..Default::default()
    });
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert!(matches!(resolution, StartResolution::Launched(LaunchOutcome::Ready { .. })), "{:?}", resolution);
    assert_eq!(host.world().launches, vec![bin("0.40.13")]);
}

#[test]
fn two_simultaneous_starts_launch_exactly_one_server() {
    let world = FakeWorld {
        launch_delay: Duration::from_millis(400),
        ..Default::default()
    };
    let host = FakeHost::new(world);
    host.world().versions.insert(bin("0.40.13"), "0.40.13".into());
    let barrier = std::sync::Barrier::new(2);

    let resolutions: Vec<StartResolution> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let mut host = host.clone();
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    resolve_start(&mut host, &bin("0.40.13"), "0.40.13", &previous_of)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("no panic")).collect()
    });

    assert_eq!(host.world().launches.len(), 1, "{:?}", resolutions);
    assert_eq!(host.world().stops, 0, "the second start must not stop the first one's server");
    assert!(resolutions.iter().any(|r| matches!(r, StartResolution::Launched(LaunchOutcome::Ready { .. }))));
    assert!(resolutions.contains(&StartResolution::AlreadyRunning { running_version: "0.40.13".into() }));
}

#[test]
fn a_poisoned_start_lock_does_not_block_every_later_start() {
    let _ = std::thread::spawn(|| {
        let _guard = START_LOCK.lock().unwrap();
        panic!("a start died while holding the lock");
    })
    .join();
    assert!(START_LOCK.is_poisoned());
    let host = FakeHost::new(FakeWorld::default());
    let resolution = resolve_start(&mut host.clone(), &bin("0.40.13"), "0.40.13", &no_previous);
    assert!(matches!(resolution, StartResolution::Launched(_)), "{:?}", resolution);
}

#[test]
fn spawn_plans_are_refined_by_the_process_table_and_the_port() {
    let free = || false;
    let taken = || true;
    assert_eq!(refine_spawn_plan(false, None, free), RuntimeStartPlan::Spawn);
    assert_eq!(refine_spawn_plan(false, None, taken), RuntimeStartPlan::PortBlocked);
    assert_eq!(
        refine_spawn_plan(true, Some(Duration::from_secs(5)), taken),
        RuntimeStartPlan::AwaitStarting
    );
    assert_eq!(
        refine_spawn_plan(true, Some(STARTING_GRACE), taken),
        RuntimeStartPlan::Spawn,
        "the grace period is over"
    );
    assert_eq!(refine_spawn_plan(true, None, taken), RuntimeStartPlan::Spawn, "age unknown: as before");
    // the port is only probed when nothing of ours runs
    assert_eq!(
        refine_spawn_plan(true, Some(Duration::from_secs(5)), || panic!("must not probe")),
        RuntimeStartPlan::AwaitStarting
    );
}

// ── the real wait loop against real child processes ──────────────────────────────────────

#[cfg(target_os = "windows")]
fn spawn_hidden(program: &str, args: &[&str]) -> std::process::Child {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("spawn helper process")
}

/// A process that lives for about `seconds` seconds (and is killed by the test).
#[cfg(target_os = "windows")]
fn sleeper(seconds: u32) -> std::process::Child {
    spawn_hidden("ping", &["-n", &seconds.to_string(), "127.0.0.1"])
}

#[cfg(target_os = "windows")]
#[test]
fn a_child_that_dies_at_once_ends_the_wait_at_once() {
    // `ollama serve` without a free port: exits with a code right after the start
    let slot = Mutex::new(Some(spawn_hidden("cmd", &["/C", "exit", "3"])));
    let started = Instant::now();

    let outcome = wait_for_server(&slot, Duration::from_secs(20), Duration::from_millis(50), &mut |_| false);

    assert_eq!(outcome, WaitOutcome::Exited(Some(3)));
    assert!(started.elapsed() < Duration::from_secs(5), "waited {:?} on a corpse", started.elapsed());
    assert!(slot.lock().unwrap().is_none(), "the dead child is removed from the slot");
}

#[cfg(target_os = "windows")]
#[test]
fn a_living_child_that_answers_ends_the_wait_with_ready() {
    let slot = Mutex::new(Some(sleeper(30)));
    let mut probes = 0;
    let outcome = wait_for_server(&slot, Duration::from_secs(10), Duration::from_millis(20), &mut |attempt| {
        probes += 1;
        attempt >= 3
    });
    assert_eq!(outcome, WaitOutcome::Ready);
    assert_eq!(probes, 4);
    let mut child = slot.lock().unwrap().take().unwrap();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
#[test]
fn a_living_child_that_stays_silent_times_out_and_stays_alive() {
    let slot = Mutex::new(Some(sleeper(30)));
    let started = Instant::now();
    let outcome = wait_for_server(&slot, Duration::from_millis(400), Duration::from_millis(50), &mut |_| false);
    assert_eq!(outcome, WaitOutcome::TimedOut);
    assert!(started.elapsed() >= Duration::from_millis(400));
    assert_eq!(managed_child_state(&slot), ChildState::Alive);
    let mut child = slot.lock().unwrap().take().unwrap();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
#[test]
fn a_child_that_dies_while_something_else_answers_is_not_reported_ready() {
    // the port was taken by another server: our own process died, the endpoint still answers
    let slot = Mutex::new(Some(spawn_hidden("cmd", &["/C", "exit", "1"])));
    std::thread::sleep(Duration::from_millis(800)); // the child is certainly gone now
    let outcome = wait_for_server(&slot, Duration::from_secs(10), Duration::from_millis(50), &mut |_| true);
    assert_eq!(outcome, WaitOutcome::Exited(Some(1)));
}

#[test]
fn a_child_stopped_by_someone_else_ends_the_wait() {
    let slot: Mutex<Option<std::process::Child>> = Mutex::new(None);
    let outcome = wait_for_server(&slot, Duration::from_secs(10), Duration::from_millis(20), &mut |_| true);
    assert_eq!(outcome, WaitOutcome::Gone);
}

// ── previous runtime lookup ──────────────────────────────────────────────────────────────

#[test]
fn the_previous_runtime_is_found_by_stored_path_or_by_its_version_folder() {
    let root = Scratch::new("previous");
    marker(&root.path().join("0.30.4"), "old");
    marker(&root.path().join("0.40.13"), "new");
    let mut settings = crate::state::Settings::default();

    // the stored pair belongs to the running version: that exact path
    settings.providers.ollama.runtime_path = root.path().join("0.30.4").join("ollama.exe").to_string_lossy().to_string();
    settings.providers.ollama.runtime_version = "0.30.4".to_string();
    assert_eq!(
        previous_runtime_binary(&settings, Some(root.path()), "0.30.4"),
        Some(root.path().join("0.30.4").join("ollama.exe"))
    );

    // right after an install the stored pair already names the NEW runtime: the running
    // version is found by its folder instead
    settings.providers.ollama.runtime_path = root.path().join("0.40.13").join("ollama.exe").to_string_lossy().to_string();
    settings.providers.ollama.runtime_version = "0.40.13".to_string();
    assert_eq!(
        previous_runtime_binary(&settings, Some(root.path()), "0.30.4"),
        Some(root.path().join("0.30.4").join("ollama.exe"))
    );

    // not installed, no root, or not a plain version: nothing to go back to
    assert_eq!(previous_runtime_binary(&settings, Some(root.path()), "0.20.0"), None);
    assert_eq!(previous_runtime_binary(&settings, None, "0.30.4"), None);
    assert_eq!(previous_runtime_binary(&settings, Some(root.path()), "..\\0.30.4"), None);
    assert_eq!(previous_runtime_binary(&settings, Some(root.path()), ""), None);
}

#[test]
fn a_lost_exit_code_is_still_a_readable_message() {
    assert!(describe_exit(Some(2)).contains("exit code 2"));
    assert!(describe_exit(None).contains("exited right after it was started"));
    assert!(runtime_label("").contains("selected runtime"));
    assert_eq!(runtime_label("0.40.13"), "Ollama 0.40.13");
}

// ── stale pid file: never kill a stranger ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn process_alive(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
}

#[cfg(target_os = "windows")]
fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    condition()
}

#[cfg(target_os = "windows")]
#[test]
fn a_stale_pid_file_never_kills_an_unrelated_process_that_reused_the_pid() {
    let dir = Scratch::new("pidfile-stranger");
    let lockfile = dir.path().join("ollama_managed.pid");
    // an innocent program that happens to own the pid now (a normal `ping.exe`)
    let mut stranger = sleeper(60);
    let pid = stranger.id();
    fs::write(&lockfile, pid.to_string()).unwrap();

    // control: the previous implementation (`taskkill /PID <n> /T /F`) takes it down
    // (verified on a second process so this test cannot hurt the first)
    let mut victim = sleeper(60);
    let _ = Command::new("taskkill")
        .args(["/PID", &victim.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    assert!(wait_until(Duration::from_secs(5), || victim.try_wait().map(|s| s.is_some()).unwrap_or(false)),
        "control: the old command kills whatever owns the pid");

    kill_stale_ollama_pid_in(&lockfile);

    assert!(stranger.try_wait().unwrap().is_none(), "the stranger must survive");
    assert!(process_alive(pid));
    assert!(!lockfile.exists(), "the stale file is removed either way");
    let _ = stranger.kill();
    let _ = stranger.wait();
}

#[cfg(target_os = "windows")]
#[test]
fn a_stale_pid_file_still_kills_a_real_leftover_ollama() {
    let dir = Scratch::new("pidfile-ollama");
    let lockfile = dir.path().join("ollama_managed.pid");
    // a process whose image really is called ollama.exe
    let system_root = std::env::var_os("SystemRoot").map(PathBuf::from).expect("SystemRoot");
    let fake = dir.path().join("ollama.exe");
    fs::copy(system_root.join("System32").join("PING.EXE"), &fake).expect("copy ping.exe");
    let mut leftover = Command::new(&fake)
        .args(["-n", "60", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("spawn ollama.exe stand-in");
    fs::write(&lockfile, leftover.id().to_string()).unwrap();

    kill_stale_ollama_pid_in(&lockfile);

    assert!(
        wait_until(Duration::from_secs(5), || leftover.try_wait().map(|s| s.is_some()).unwrap_or(false)),
        "the leftover Ollama must be killed"
    );
    assert!(!lockfile.exists());
}

#[test]
fn a_garbage_pid_file_is_removed_without_killing_anything() {
    let dir = Scratch::new("pidfile-garbage");
    for content in ["", "abc", "-1", "99999999999999999999", "12 34"] {
        let lockfile = dir.path().join("ollama_managed.pid");
        fs::write(&lockfile, content).unwrap();
        kill_stale_ollama_pid_in(&lockfile);
        assert!(!lockfile.exists(), "{:?}", content);
    }
    // a missing file is simply nothing to do
    kill_stale_ollama_pid_in(&dir.path().join("none.pid"));
}

// ───────────────────────── 5. managed runtime: start and detect agree ─────────────────────────

#[test]
fn start_and_detect_agree_for_every_combination_of_what_exists() {
    // The full table: (stored path exists?) x (managed installed?).
    // Whatever start would launch, detect must report as the current runtime.
    let root = Scratch::new("agree");
    marker(&root.path().join("0.40.13"), "managed");
    let stored_exe = root.path().join("stored").join("ollama.exe");
    fs::create_dir_all(stored_exe.parent().unwrap()).unwrap();
    fs::write(&stored_exe, "stored").unwrap();
    let gone_exe = root.path().join("gone").join("ollama.exe");

    let mut disagreements: Vec<String> = Vec::new();
    for stored in [None, Some(&stored_exe), Some(&gone_exe)] {
        for has_managed in [false, true] {
            let mut settings = crate::state::Settings::default();
            settings.providers.ollama.runtime_path =
                stored.map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            settings.providers.ollama.runtime_target_version = String::new();
            let managed = has_managed.then_some(root.path());
            let started = select_runtime_binary_in(&settings, managed).ok();
            let detected = detect_runtime_in(&settings, managed);
            let started_path = started.as_ref().map(|(p, _)| p.clone());
            let detected_path = detected.as_ref().map(|(_, p, _)| p.clone());
            // Nothing installed: start refuses on purpose with the "install it" message.
            let start_refused = started_path.is_none();
            if !start_refused && started_path != detected_path {
                disagreements.push(format!(
                    "stored={:?} managed={}: start={:?} detect={:?}",
                    stored.map(|p| p.display().to_string()),
                    has_managed,
                    started_path.map(|p| p.display().to_string()),
                    detected_path.map(|p| p.display().to_string()),
                ));
            }
        }
    }
    assert!(disagreements.is_empty(), "start and detect disagree:\n{}", disagreements.join("\n"));
}

// ───────────────────────── 7. signature probe and version probe have a time limit ─────────────────────────

/// The signed 0.40.x binary of the dev machine (any version present); tests skip without it.
#[cfg(target_os = "windows")]
fn signed_binary() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    ["0.40.7", "0.40.13", "0.30.4"].iter().find_map(|version| {
        let path = PathBuf::from(&local)
            .join("Trispr Flow")
            .join("ollama-runtime")
            .join(version)
            .join("ollama.exe");
        path.is_file().then_some(path)
    })
}

#[cfg(target_os = "windows")]
fn tampered_copy(dir: &Path) -> Option<PathBuf> {
    let binary = signed_binary()?;
    let mut bytes = fs::read(binary).expect("read real binary");
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    let copy = dir.join("ollama.exe");
    fs::write(&copy, &bytes).expect("write tampered copy");
    Some(copy)
}

/// PowerShell script that reports its own pid through the target path, then hangs.
#[cfg(target_os = "windows")]
const HANGING_PROBE: &str =
    "$pid | Set-Content -LiteralPath $env:TRISPR_SIGNATURE_TARGET; Start-Sleep -Seconds 60";

#[cfg(target_os = "windows")]
fn pid_is_gone(pid: u32) -> bool {
    wait_until(Duration::from_secs(5), || !process_alive(pid))
}

#[cfg(target_os = "windows")]
#[test]
fn a_hanging_signature_probe_ends_as_unverified_and_the_process_is_killed() {
    let scratch = Scratch::new("sig-hang");
    let pid_file = scratch.path().join("pid.txt");
    let started = Instant::now();

    let verdict = verify_authenticode_signer_timed(
        &windows_powershell_path(),
        &pid_file,
        HANGING_PROBE,
        Duration::from_secs(3),
    )
    .expect("a probe that cannot finish is not a rejection");

    match verdict {
        SignatureVerdict::Unverified(reason) => {
            assert!(reason.contains("did not finish within 3 s"), "{}", reason)
        }
        other => panic!("expected Unverified, got {:?}", other),
    }
    assert!(started.elapsed() < Duration::from_secs(15), "{:?}", started.elapsed());
    let pid: u32 = fs::read_to_string(&pid_file)
        .expect("the script reported its pid before it hung")
        .trim()
        .parse()
        .expect("pid");
    assert!(pid_is_gone(pid), "the hung PowerShell (pid {}) was left running", pid);
}

#[cfg(target_os = "windows")]
#[test]
fn a_signature_probe_that_times_out_is_fail_open_even_for_a_tampered_binary() {
    // Policy, written down on purpose (see `verify_authenticode_signer`): "no verdict" is not
    // "bad signature". The SHA-256 of the archive was verified before; a revocation lookup
    // that hangs is an environment problem. Switching to fail-closed is a change in
    // `verify_authenticode_signer_timed` and must flip this test.
    let scratch = Scratch::new("sig-policy");
    let Some(tampered) = tampered_copy(scratch.path()) else {
        return;
    };
    // the same file judged by a probe that finishes: rejected
    let judged = verify_authenticode_signer(&tampered).expect_err("tampered binary is rejected");
    assert!(judged.contains("HashMismatch"), "{}", judged);
    // judged by a probe that hangs: unverified, not rejected
    let hung = verify_authenticode_signer_timed(
        &windows_powershell_path(),
        &scratch.path().join("pid.txt"),
        HANGING_PROBE,
        Duration::from_secs(2),
    )
    .expect("fail-open");
    assert!(matches!(hung, SignatureVerdict::Unverified(_)), "{:?}", hung);
}

#[cfg(target_os = "windows")]
#[test]
fn a_probe_that_finishes_in_time_still_verifies_the_real_binary() {
    let Some(binary) = signed_binary() else {
        return;
    };
    let started = Instant::now();
    assert_eq!(
        verify_authenticode_signer(&binary).expect("genuine"),
        SignatureVerdict::Verified,
        "a real verdict must not be reported as Unverified"
    );
    assert!(started.elapsed() < SIGNATURE_PROBE_TIMEOUT);
}

#[cfg(target_os = "windows")]
#[test]
fn a_probe_that_cannot_run_or_prints_nothing_is_unverified_with_a_reason() {
    let scratch = Scratch::new("sig-reasons");
    let missing = scratch.path().join("no-such-powershell.exe");
    let verdict = verify_authenticode_signer_with(&missing, &scratch.path().join("x.exe")).unwrap();
    match verdict {
        SignatureVerdict::Unverified(reason) => assert!(reason.contains("could not run"), "{}", reason),
        other => panic!("{:?}", other),
    }
    let silent = verify_authenticode_signer_timed(
        &windows_powershell_path(),
        &scratch.path().join("x.exe"),
        "$null",
        Duration::from_secs(20),
    )
    .unwrap();
    match silent {
        SignatureVerdict::Unverified(reason) => assert!(reason.contains("no result"), "{}", reason),
        other => panic!("{:?}", other),
    }
}

#[cfg(target_os = "windows")]
#[test]
fn the_runner_captures_everything_a_chatty_child_prints_without_deadlock() {
    let scratch = Scratch::new("run-chatty");
    let big = scratch.path().join("big.txt");
    fs::write(&big, "x".repeat(2 * 1024 * 1024)).unwrap();
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "type", &big.to_string_lossy()]).creation_flags(CREATE_NO_WINDOW);

    let output = run_with_timeout(&mut cmd, Duration::from_secs(30)).expect("runs");

    assert_eq!(output.stdout.len(), 2 * 1024 * 1024);

    let mut errs = Command::new("cmd");
    errs.args(["/C", "echo oops 1>&2"]).creation_flags(CREATE_NO_WINDOW);
    let output = run_with_timeout(&mut errs, Duration::from_secs(30)).expect("runs");
    assert!(String::from_utf8_lossy(&output.stderr).contains("oops"));
    assert!(matches!(
        run_with_timeout(&mut Command::new("Z:\\no\\such\\program.exe"), Duration::from_secs(5)),
        Err(RunError::Spawn(_))
    ));
}

#[cfg(target_os = "windows")]
#[test]
fn the_runner_kills_a_child_that_outlives_its_limit() {
    let mut cmd = Command::new("ping");
    cmd.args(["-n", "60", "127.0.0.1"]).creation_flags(CREATE_NO_WINDOW);
    let started = Instant::now();
    let outcome = run_with_timeout(&mut cmd, Duration::from_millis(700));
    assert!(matches!(outcome, Err(RunError::TimedOut)));
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}

#[cfg(target_os = "windows")]
#[test]
fn a_binary_that_hangs_on_version_is_unknown_not_a_hang() {
    let scratch = Scratch::new("version-hang");
    let hang = scratch.path().join("hang-ollama.cmd");
    fs::write(&hang, "@echo off\r\nping -n 60 127.0.0.1 >nul\r\n").unwrap();
    let started = Instant::now();

    let version = probe_binary_version(&hang, Duration::from_secs(1));

    assert_eq!(version, "");
    assert!(started.elapsed() < Duration::from_secs(8), "{:?}", started.elapsed());

    // and a binary that answers in time still reports its version
    let quick = scratch.path().join("quick-ollama.cmd");
    fs::write(&quick, "@echo off\r\necho ollama version is 9.8.7\r\n").unwrap();
    assert_eq!(probe_binary_version(&quick, Duration::from_secs(20)), "9.8.7");
}

#[cfg(target_os = "windows")]
#[test]
fn a_binary_replaced_in_place_is_not_served_from_the_version_cache() {
    // e.g. the Ollama desktop app updating itself: same path, new version
    let scratch = Scratch::new("version-cache");
    let exe = scratch.path().join("ollama.cmd");
    fs::write(&exe, "@echo off\r\necho ollama version is 1.0.0\r\n").unwrap();
    assert_eq!(parse_ollama_version(&exe), "1.0.0");
    assert_eq!(parse_ollama_version(&exe), "1.0.0", "unchanged file: from the cache");

    fs::write(&exe, "@echo off\r\necho ollama version is 1.0.1-and-longer\r\n").unwrap();
    assert_eq!(parse_ollama_version(&exe), "1.0.1-and-longer");
}

// ───────────────────────── live: real Ollama binaries (ignored by default) ─────────────────────────
//
// Run: cargo test --manifest-path src-tauri/Cargo.toml --lib -- reliability_tests::live_ --ignored --nocapture --test-threads=1
//
// SAFETY: these start their OWN `ollama serve` on ports 11442-11444 with an empty temporary
// OLLAMA_MODELS and install into a temporary runtime root. They never touch the user's server
// (:11434), the user's runtime folders or the user's models, and they only kill processes they
// started themselves.

#[cfg(target_os = "windows")]
fn live_runtime_binary() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    ["0.40.13", "0.40.7", "0.30.4"].iter().find_map(|version| {
        let path = PathBuf::from(&local)
            .join("Trispr Flow")
            .join("ollama-runtime")
            .join(version)
            .join("ollama.exe");
        path.is_file().then_some(path)
    })
}

/// A server this test started. It is killed (with its children) when the guard drops, also when
/// an assertion fails first: a failing test must not leave a server behind.
#[cfg(target_os = "windows")]
struct LiveServer {
    slot: Mutex<Option<std::process::Child>>,
}

#[cfg(target_os = "windows")]
impl LiveServer {
    fn spawn(binary: &Path, port: u16, models: &Path) -> Self {
        let child = Command::new(binary)
            .arg("serve")
            .env("OLLAMA_HOST", format!("127.0.0.1:{}", port))
            .env("OLLAMA_MODELS", models)
            .env("OLLAMA_NO_CLOUD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("spawn ollama serve");
        Self {
            slot: Mutex::new(Some(child)),
        }
    }

    fn stop(&self) {
        if let Some(mut child) = self.slot.lock().unwrap().take() {
            kill_process_tree(&mut child);
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for LiveServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore]
fn live_a_real_server_is_awaited_and_a_second_one_on_its_port_dies_at_once() {
    let Some(binary) = live_runtime_binary() else {
        eprintln!("no Ollama runtime on this machine - skipping");
        return;
    };
    let port = 11442;
    let endpoint = format!("http://127.0.0.1:{}", port);
    let models = Scratch::new("live-models");
    let first = LiveServer::spawn(&binary, port, models.path());

    let started = Instant::now();
    let outcome = wait_for_server(&first.slot, Duration::from_secs(30), Duration::from_millis(250), &mut |_| {
        ping_ollama_quick_fresh(&endpoint).is_ok()
    });
    println!("first server: {:?} after {:?}", outcome, started.elapsed());
    assert_eq!(outcome, WaitOutcome::Ready);

    // what the start logic reads from a running server
    let running = query_running_ollama_version(&endpoint).expect("the server reports its version");
    println!("running version {}", running);
    assert_eq!(running, parse_ollama_version(&binary));
    assert!(endpoint_port_in_use(&endpoint));
    assert!(matches!(
        plan_runtime_start(true, Some(&running), &running, true),
        RuntimeStartPlan::AlreadyRunning { .. }
    ));

    // a second `ollama serve` on the same port: the old code waited the full 12 s for it
    let second = LiveServer::spawn(&binary, port, models.path());
    let started = Instant::now();
    let outcome = wait_for_server(&second.slot, Duration::from_secs(12), Duration::from_millis(250), &mut |_| false);
    println!("second server on the same port: {:?} after {:?}", outcome, started.elapsed());
    assert!(matches!(outcome, WaitOutcome::Exited(Some(code)) if code != 0), "{:?}", outcome);
    assert!(started.elapsed() < Duration::from_secs(8), "a dead start must not look like a slow one");
    assert!(describe_exit(Some(1)).contains("port"));

    // the first one is untouched by all this
    assert!(ping_ollama_quick_fresh(&endpoint).is_ok());
    first.stop();
    assert!(wait_until(Duration::from_secs(10), || !endpoint_port_in_use(&endpoint)), "port released after stop");
}

#[cfg(target_os = "windows")]
#[test]
#[ignore]
fn live_a_foreign_program_on_the_port_is_detected_before_anything_is_spawned() {
    let Some(binary) = live_runtime_binary() else {
        eprintln!("no Ollama runtime on this machine - skipping");
        return;
    };
    let port = 11443;
    let endpoint = format!("http://127.0.0.1:{}", port);
    // a program that accepts connections and never speaks HTTP
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind the foreign port");
    let held: Arc<Mutex<Vec<std::net::TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let keep = held.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            keep.lock().unwrap().push(stream);
        }
    });

    let reachable = runtime_endpoint_reachable(&endpoint);
    let in_use = endpoint_port_in_use(&endpoint);
    println!("reachable as Ollama: {}, port in use: {}", reachable, in_use);
    assert!(!reachable && in_use);
    assert_eq!(
        refine_spawn_plan(false, None, || endpoint_port_in_use(&endpoint)),
        RuntimeStartPlan::PortBlocked
    );

    // and what the old flow would have done: spawn anyway and watch it die on the bound port
    let models = Scratch::new("live-models-blocked");
    let server = LiveServer::spawn(&binary, port, models.path());
    let started = Instant::now();
    let outcome = wait_for_server(&server.slot, Duration::from_secs(12), Duration::from_millis(250), &mut |_| false);
    println!("ollama serve on the foreign port: {:?} after {:?}", outcome, started.elapsed());
    assert!(matches!(outcome, WaitOutcome::Exited(Some(code)) if code != 0), "{:?}", outcome);
}

#[cfg(target_os = "windows")]
#[test]
#[ignore]
fn live_a_real_archive_installs_through_the_whole_pipeline_and_a_running_version_cannot_be_replaced() {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let archive = PathBuf::from(&local)
        .join("Trispr Flow")
        .join("runtime-cache")
        .join("ollama-windows-amd64-v0.30.4.zip");
    if !archive.is_file() {
        eprintln!("{} not present - skipping", archive.display());
        return;
    }
    let version = "0.30.4";
    let root = Scratch::new("live-install-root");
    let port = 11445;
    let endpoint = format!("http://127.0.0.1:{}", port);
    let progress: &mut dyn FnMut(usize, usize) = &mut |_, _| {};
    let verify = |staged: &Path| {
        let verdict = verify_staged_runtime_verdict(staged, version)?;
        println!("signature verdict: {:?}", verdict);
        Ok(())
    };

    // 1. the real archive through hash -> unpack -> signature + version -> swap
    let mut file = open_archive_locked(&archive).expect("open archive");
    let digest = sha256_reader(&mut file).expect("hash");
    println!("archive sha256 {}", digest);
    if let Some(metadata) = load_runtime_archive_metadata(&archive) {
        assert!(metadata.sha256.eq_ignore_ascii_case(&digest), "cached archive matches its sidecar");
    }
    let started = Instant::now();
    let binary = install_archive(file, root.path(), version, MAX_RUNTIME_EXTRACTED_BYTES, &verify, progress)
        .expect("real install");
    println!("installed in {:.1}s -> {}", started.elapsed().as_secs_f32(), binary.display());
    assert_eq!(binary, root.path().join(version).join("ollama.exe"));
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
    assert_eq!(parse_ollama_version(&binary), version);
    assert!(scan_installed_runtime_versions(root.path()).contains_key(version));

    // 2. serve from it, then try to install the same version over the running one
    let models = Scratch::new("live-models-install");
    let server = LiveServer::spawn(&binary, port, models.path());
    let outcome = wait_for_server(&server.slot, Duration::from_secs(40), Duration::from_millis(250), &mut |_| {
        ping_ollama_quick_fresh(&endpoint).is_ok()
    });
    assert_eq!(outcome, WaitOutcome::Ready);
    let running = query_running_ollama_version(&endpoint).expect("version");
    assert_eq!(
        install_blocked_by_running_server(version, Some(&running), true).is_some(),
        running == version,
        "the up-front check says no exactly when the running version is the one to install"
    );

    let file = open_archive_locked(&archive).expect("open archive again");
    let started = Instant::now();
    let error = install_archive(file, root.path(), version, MAX_RUNTIME_EXTRACTED_BYTES, &verify, progress)
        .expect_err("the serving runtime cannot be replaced");
    println!("replace while serving: {} (after {:.1}s)", error, started.elapsed().as_secs_f32());
    assert!(error.contains("is in use"), "{}", error);
    assert_eq!(dot_entries(root.path()), Vec::<String>::new(), "staging removed after the refusal");
    assert!(ping_ollama_quick_fresh(&endpoint).is_ok(), "the serving runtime is untouched");
    assert!(binary.is_file());

    // 3. after it stopped, the same install goes through
    server.stop();
    assert!(wait_until(Duration::from_secs(10), || !endpoint_port_in_use(&endpoint)));
    std::thread::sleep(Duration::from_millis(500));
    let file = open_archive_locked(&archive).expect("open archive a third time");
    install_archive(file, root.path(), version, MAX_RUNTIME_EXTRACTED_BYTES, &verify, progress)
        .expect("works once it stopped");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new());
    assert_eq!(parse_ollama_version(&binary), version);
}

// ── a runtime that is being served from is never replaced (measured on a real server) ──────

/// Copies a real, harmless, long-running executable to `<dir>/ollama.exe` and starts it: a
/// process whose image is mapped from that file, like a running Ollama server.
#[cfg(target_os = "windows")]
fn running_stand_in(dir: &Path) -> std::process::Child {
    fs::create_dir_all(dir).unwrap();
    let system_root = std::env::var_os("SystemRoot").map(PathBuf::from).expect("SystemRoot");
    let exe = dir.join("ollama.exe");
    fs::copy(system_root.join("System32").join("PING.EXE"), &exe).expect("copy ping.exe");
    Command::new(&exe)
        .args(["-n", "120", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .expect("start the stand-in")
}

#[test]
fn files_that_nobody_runs_are_not_in_use() {
    let scratch = Scratch::new("in-use-no");
    assert!(!executable_in_use(&scratch.path().join("missing.exe")));
    assert!(!executable_in_use(scratch.path()));
    marker(&scratch.path().join("0.40.13"), "plain file");
    assert!(!executable_in_use(&scratch.path().join("0.40.13").join("ollama.exe")));
    // and the probe wrote nothing
    assert_eq!(read_marker(&scratch.path().join("0.40.13")), "plain file");
}

#[cfg(target_os = "windows")]
#[test]
fn a_running_ollama_exe_is_detected_and_its_folder_survives_an_install_attempt() {
    // The old guard (a handle held with share_mode(0)) did not reflect reality: Windows renames
    // and even deletes the folder of a running exe. The live run against a real 0.30.4 server
    // replaced its folder and left the server without files. This is the same situation with a
    // real mapped image.
    let root = Scratch::new("in-use-real");
    let target = root.path().join("0.40.13");
    let mut running = running_stand_in(&target);
    std::thread::sleep(Duration::from_millis(300));
    assert!(executable_in_use(&target.join("ollama.exe")), "the probe sees the running image");

    let error = install_bytes(
        zip_bytes(&[("ollama.exe", b"new"), ("lib/a.dll", b"dll")]),
        root.path(),
        "0.40.13",
        &|_| Ok(()),
    )
    .expect_err("a running runtime must not be replaced");

    assert!(error.contains("is in use and cannot be replaced"), "{}", error);
    assert!(target.join("ollama.exe").is_file(), "the running exe is still where it was");
    assert_eq!(dot_entries(root.path()), Vec::<String>::new(), "refused before anything was unpacked");
    assert!(running.try_wait().unwrap().is_none(), "the process was not disturbed");

    // swap_runtime_dir alone (the second line of defence) refuses too
    let staging = root.path().join(".staging-0.40.13-1");
    marker(&staging, "new");
    let backup = root.path().join(".old-0.40.13-1");
    let error = swap_runtime_dir(&staging, &target, &backup).expect_err("refused at the swap");
    assert!(error.contains("is in use"), "{}", error);
    assert!(target.join("ollama.exe").is_file() && !backup.exists());

    // once it stopped, replacing works
    kill_process_tree(&mut running);
    assert!(wait_until(Duration::from_secs(5), || !executable_in_use(&target.join("ollama.exe"))));
    fs::remove_dir_all(&staging).unwrap();
    install_bytes(zip_bytes(&[("ollama.exe", b"new")]), root.path(), "0.40.13", &|_| Ok(()))
        .expect("replaceable after it stopped");
    assert_eq!(read_marker(&target), "new");
}

// ───────────────────────── 6. folder scans stay bounded ─────────────────────────

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

#[test]
fn the_exe_is_found_at_normal_depth_regardless_of_case() {
    let root = Scratch::new("find-depth");
    let nested = root.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("Ollama.EXE"), "x").unwrap();
    assert_eq!(find_file_recursive(root.path(), "ollama.exe"), Some(nested.join("Ollama.EXE")));
    assert_eq!(find_file_recursive(root.path(), "other.exe"), None);
    assert_eq!(find_file_recursive(&root.path().join("missing"), "ollama.exe"), None);
}

#[test]
fn the_search_depth_is_capped() {
    let root = Scratch::new("find-cap");
    let mut deep = root.path().to_path_buf();
    for level in 0..(FIND_MAX_DEPTH + 2) {
        deep = deep.join(format!("d{}", level));
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("ollama.exe"), "x").unwrap();
    assert_eq!(find_file_recursive(root.path(), "ollama.exe"), None, "deeper than any runtime");

    let allowed = root.path().join("ok");
    let mut at_cap = allowed.clone();
    for level in 0..FIND_MAX_DEPTH {
        at_cap = at_cap.join(format!("d{}", level));
    }
    fs::create_dir_all(&at_cap).unwrap();
    fs::write(at_cap.join("ollama.exe"), "x").unwrap();
    assert!(find_file_recursive(&allowed, "ollama.exe").is_some());
}

#[cfg(target_os = "windows")]
#[test]
fn junction_cycles_in_a_runtime_folder_cannot_hang_the_scan() {
    // `mklink /J` needs no admin rights. Two junctions back to the root make the folder tree
    // infinite (and exponentially branching): an unbounded search would never come back, and
    // the UI scans these folders on every refresh.
    let root = Scratch::new("find-cycle");
    let runtime = root.path().join("0.40.13");
    fs::create_dir_all(&runtime).unwrap();
    junction(&runtime.join("loop-a"), root.path());
    junction(&runtime.join("loop-b"), &runtime);
    junction(&root.path().join("0.39.0"), &runtime);

    let started = Instant::now();
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = root.path().to_path_buf();
    std::thread::spawn(move || {
        let found = find_file_recursive(&path, "ollama.exe");
        let scanned = scan_installed_runtime_versions(&path);
        let _ = sender.send((found, scanned));
    });
    let (found, scanned) = receiver
        .recv_timeout(Duration::from_secs(20))
        .expect("the scan must come back even with cycles in the tree");
    println!("scan with junction cycles took {:?}", started.elapsed());

    assert_eq!(found, None);
    assert!(scanned.is_empty(), "no runtime without an exe: {:?}", scanned);

    // the real exe is still found next to the cycles
    fs::write(runtime.join("ollama.exe"), "x").unwrap();
    assert_eq!(find_file_recursive(&runtime, "ollama.exe"), Some(runtime.join("ollama.exe")));
}

#[cfg(target_os = "windows")]
#[test]
fn a_junction_to_another_folder_is_not_followed_but_the_folder_itself_may_be_one() {
    let root = Scratch::new("find-junction");
    let elsewhere = Scratch::new("find-elsewhere");
    marker(elsewhere.path(), "outside");
    let runtime = root.path().join("0.40.13");
    fs::create_dir_all(&runtime).unwrap();
    junction(&runtime.join("linked"), elsewhere.path());
    assert_eq!(find_file_recursive(&runtime, "ollama.exe"), None, "links inside are not followed");

    // a managed version folder that is itself a junction (runtime moved to another drive)
    let moved = root.path().join("0.40.7");
    junction(&moved, elsewhere.path());
    assert_eq!(
        find_file_recursive(&moved, "ollama.exe"),
        Some(moved.join("ollama.exe")),
        "the start folder itself may be a link"
    );
    assert!(scan_installed_runtime_versions(root.path()).contains_key("0.40.7"));
}

// ───────────────────────── 6. parsers of outside text survive anything ─────────────────────────

/// Deterministic hostile text: multi-byte characters, tag fragments, separators, control
/// characters, in random order. Every parser of outside data must survive all of it.
fn hostile_strings(count: usize) -> Vec<String> {
    const PIECES: &[&str] = &[
        "a", "Z", "0", ":", "/", "\\", "-", "_", ".", "<", ">", "</", "<transcript>", "</transcript>",
        "<think>", "</think>", "transcript", "ä", "ß", "日", "🦙", "\u{0}", "\u{feff}", " ", "\n", "\t",
        "sha256-", "sha256:", "hf.co/", "trispr-backup-", "O=", "\"", ",", " 0.40.13", "v", "ollama version is ",
        "client version is ", "hier ist ", "Sure", "=", "|",
    ];
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..count)
        .map(|_| {
            let length = 1 + (next() % 14) as usize;
            (0..length)
                .map(|_| PIECES[(next() % PIECES.len() as u64) as usize])
                .collect::<String>()
        })
        .collect()
}


#[test]
fn runtime_parsers_survive_hostile_text() {
    for text in hostile_strings(20_000) {
        let _ = parse_version_output(&text);
        let _ = summarize_release_notes(Some(&text));
        let _ = parse_sha256_from_text(&text, "ollama-windows-amd64.zip");
        let _ = parse_sha256_from_text(&format!("{} {}", text, text), &text);
        let _ = subject_has_organization(&text, OLLAMA_SIGNER_ORGANIZATION);
        let _ = evaluate_signature_report(&text);
        let _ = sanitize_model_name(&text);
        let _ = parse_runtime_version_tuple(&text);
        let _ = parse_stable_semver(&text);
        let _ = is_safe_runtime_version(&text);
        let _ = compare_runtime_versions_desc(&text, "0.40.13");
        let _ = infer_runtime_version_from_archive_path(Path::new(&text));
        let _ = managed_runtime_version_of(Path::new("rt"), &Path::new("rt").join(&text).join("ollama.exe"));
        let _ = checksum_entry_matches_file_name(&text, "ollama-windows-amd64.zip");
        let _ = select_major_line_versions(&[text.clone(), "v0.40.13".into()], 3);
        let _ = validate_runtime_download_url(&text, "0.40.13");
        let _ = endpoint_host_port(&text);
        let _ = format_wait(text.len() as u64);
    }
}

// A `StartHost` made of real processes on a private port (the production `RealHost` also needs
// the app handle for settings and events; everything it DECIDES lives in `resolve_start`).
#[cfg(target_os = "windows")]
struct LiveHost {
    port: u16,
    models: PathBuf,
    server: LiveServer,
    spawned_at: Option<Instant>,
    pid_file: Option<PathBuf>,
}

#[cfg(target_os = "windows")]
impl LiveHost {
    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

#[cfg(target_os = "windows")]
impl StartHost for LiveHost {
    fn endpoint_reachable(&self) -> bool {
        ping_ollama_quick_fresh(&self.endpoint()).is_ok()
    }
    fn running_version(&self) -> Option<String> {
        query_running_ollama_version(&self.endpoint())
    }
    fn managed_alive(&self) -> bool {
        managed_child_state(&self.server.slot) == ChildState::Alive
    }
    fn managed_spawn_age(&self) -> Option<Duration> {
        self.spawned_at.map(|at| at.elapsed())
    }
    fn port_in_use(&self) -> bool {
        endpoint_port_in_use(&self.endpoint())
    }
    fn endpoint_label(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
    fn stop_managed_and_free_port(&mut self) -> bool {
        self.server.stop();
        wait_until(Duration::from_secs(10), || !endpoint_port_in_use(&self.endpoint()))
    }
    fn reclaim_stale_port(&mut self) -> bool {
        // like production: the pid file of the crashed session names the leftover
        if let Some(pid_file) = &self.pid_file {
            kill_stale_ollama_pid_in(pid_file);
        }
        wait_until(Duration::from_secs(5), || !endpoint_port_in_use(&self.endpoint()))
    }
    fn await_starting_server(&mut self) -> LaunchOutcome {
        LaunchOutcome::Failed { stage: "stopped", message: "not used here".into() }
    }
    fn launch(&mut self, binary: &Path) -> LaunchOutcome {
        self.server.stop();
        let child = Command::new(binary)
            .arg("serve")
            .env("OLLAMA_HOST", format!("127.0.0.1:{}", self.port))
            .env("OLLAMA_MODELS", &self.models)
            .env("OLLAMA_NO_CLOUD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        let child = match child {
            Ok(child) => child,
            Err(e) => {
                return LaunchOutcome::Failed { stage: "spawn_failed", message: e.to_string() }
            }
        };
        let pid = child.id();
        *self.server.slot.lock().unwrap() = Some(child);
        self.spawned_at = Some(Instant::now());
        let endpoint = self.endpoint();
        let started = Instant::now();
        match wait_for_server(&self.server.slot, Duration::from_secs(30), Duration::from_millis(250), &mut |_| {
            ping_ollama_quick_fresh(&endpoint).is_ok()
        }) {
            WaitOutcome::Ready => LaunchOutcome::Ready { pid, waited: started.elapsed() },
            WaitOutcome::TimedOut => LaunchOutcome::Pending { pid, waited: started.elapsed() },
            WaitOutcome::Exited(code) => LaunchOutcome::Failed { stage: "spawn_exited", message: describe_exit(code) },
            WaitOutcome::Gone => LaunchOutcome::Failed { stage: "stopped", message: "stopped".into() },
        }
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore]
fn live_a_failing_version_switch_brings_the_previous_real_server_back() {
    let Some(real) = live_runtime_binary() else {
        eprintln!("no Ollama runtime on this machine - skipping");
        return;
    };
    let real_version = parse_ollama_version(&real);
    let scratch = Scratch::new("live-switch");
    // the "new runtime": an executable that cannot serve (exits at once with code 1)
    let broken = scratch.path().join("ollama-broken.cmd");
    fs::write(&broken, "@echo off\r\nexit /b 1\r\n").unwrap();
    let models = Scratch::new("live-switch-models");
    let port = 11446;
    let mut host = LiveHost {
        port,
        models: models.path().to_path_buf(),
        server: LiveServer { slot: Mutex::new(None) },
        spawned_at: None,
        pid_file: None,
    };

    // 1. the old runtime is serving (this is what the user has before the switch)
    let first = host.launch(&real);
    println!("old runtime: {:?}", first);
    assert!(matches!(first, LaunchOutcome::Ready { .. }), "{:?}", first);
    assert_eq!(host.running_version().as_deref(), Some(real_version.as_str()));

    // 2. the user selects "9.9.9", whose binary dies at once
    let started = Instant::now();
    let resolution = resolve_start(&mut host, &broken, "9.9.9", &|running| {
        assert_eq!(running, real_version, "the version that is being replaced");
        Some(real.clone())
    });
    println!("switch resolution after {:?}: {:?}", started.elapsed(), resolution);

    match resolution {
        StartResolution::FellBack { outcome, binary, previous_version, message } => {
            assert!(matches!(outcome, LaunchOutcome::Ready { .. }), "{:?}", outcome);
            assert_eq!(binary, real);
            assert_eq!(previous_version, real_version);
            assert!(message.contains("Could not start Ollama 9.9.9"), "{}", message);
            assert!(message.contains("exit code 1"), "{}", message);
            assert!(message.contains("started again"), "{}", message);
        }
        other => panic!("the user was left without a runtime: {:?}", other),
    }
    // 3. the user has a working Ollama again, the old version
    assert!(host.endpoint_reachable(), "a server answers again");
    assert_eq!(host.running_version().as_deref(), Some(real_version.as_str()));
    assert!(started.elapsed() < Duration::from_secs(45), "{:?}", started.elapsed());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore]
fn live_pid_file_reclaims_a_leftover_ollama_and_a_foreign_program_is_never_touched() {
    let Some(real) = live_runtime_binary() else {
        eprintln!("no Ollama runtime on this machine - skipping");
        return;
    };
    let scratch = Scratch::new("live-reclaim");
    let models = Scratch::new("live-reclaim-models");
    let port = 11447;
    let endpoint = format!("http://127.0.0.1:{}", port);

    // 1. A real `ollama serve` of "the crashed session": running, its pid in the pid file, but
    //    in no slot of this process.
    let leftover = LiveServer::spawn(&real, port, models.path());
    assert!(wait_until(Duration::from_secs(30), || ping_ollama_quick_fresh(&endpoint).is_ok()));
    let leftover_pid = leftover.slot.lock().unwrap().as_ref().unwrap().id();
    let pid_file = scratch.path().join("ollama_managed.pid");
    fs::write(&pid_file, leftover_pid.to_string()).unwrap();
    let mut host = LiveHost {
        port,
        models: models.path().to_path_buf(),
        server: LiveServer { slot: Mutex::new(None) },
        spawned_at: None,
        pid_file: Some(pid_file.clone()),
    };

    // 2. It still answers: the new session leaves it alone.
    let resolution = resolve_start(&mut host, &real, &parse_ollama_version(&real), &|_| None);
    println!("leftover that answers: {:?}", resolution);
    assert!(matches!(resolution, StartResolution::AlreadyRunning { .. }), "{:?}", resolution);

    // 3. A foreign program on another port, no pid file: reported, never killed.
    let stranger_port = port + 1;
    let stranger = std::net::TcpListener::bind(("127.0.0.1", stranger_port)).expect("bind stranger port");
    let held: Arc<Mutex<Vec<std::net::TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let keep = held.clone();
    std::thread::spawn(move || {
        for stream in stranger.incoming().flatten() {
            keep.lock().unwrap().push(stream);
        }
    });
    let mut other = LiveHost {
        port: stranger_port,
        models: models.path().to_path_buf(),
        server: LiveServer { slot: Mutex::new(None) },
        spawned_at: None,
        pid_file: None,
    };
    let resolution = resolve_start(&mut other, &real, &parse_ollama_version(&real), &|_| None);
    println!("foreign program on the port: {:?}", resolution);
    match resolution {
        StartResolution::Failed { stage, .. } => assert_eq!(stage, "port_in_use"),
        other => panic!("a foreign program must be reported, not killed: {:?}", other),
    }
    assert!(ping_ollama_quick_fresh(&endpoint).is_ok(), "the leftover was not touched by that");

    // 4. The pid file clears the real leftover away (this is what `reclaim_stale_port` runs
    //    before a hung server's port counts as foreign).
    assert!(host.reclaim_stale_port(), "the port is free after the pid-file kill");
    assert!(!endpoint_port_in_use(&endpoint));
    assert!(!pid_file.exists());
}
