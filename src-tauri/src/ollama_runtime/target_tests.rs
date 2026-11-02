//! The selected runtime target must never turn into a downgrade offer the user did not ask for:
//! an implicit target older than the runtime in use is lifted, an explicit choice stays.

use crate::ai_fallback::models::OllamaSettings;
use crate::state::{normalize_ai_fallback_fields, Settings};

/// Managed runtime `running` with the given target, as the settings file would hold it.
fn managed(running: &str, target: &str, explicit: bool) -> Settings {
    let mut settings = Settings::default();
    let ollama = &mut settings.providers.ollama;
    ollama.runtime_source = "per_user_zip".to_string();
    ollama.runtime_path = format!(r"C:\x\ollama-runtime\{}\ollama.exe", running);
    ollama.runtime_version = running.to_string();
    ollama.runtime_target_version = target.to_string();
    ollama.runtime_target_explicit = explicit;
    settings
}

/// What a save or the next start does with the settings.
fn normalized(mut settings: Settings) -> OllamaSettings {
    normalize_ai_fallback_fields(&mut settings);
    settings.providers.ollama
}

#[test]
fn the_state_found_on_the_machine_is_healed() {
    // settings.json of 2026-11-02: runtime 0.40.13 recorded, target still 0.30.4, nobody chose it.
    let ollama = normalized(managed("0.40.13", "0.30.4", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
    assert!(!ollama.runtime_target_explicit);
}

#[test]
fn an_explicit_older_target_stays() {
    let ollama = normalized(managed("0.40.13", "0.30.4", true));
    assert_eq!(ollama.runtime_target_version, "0.30.4");
    assert!(ollama.runtime_target_explicit);
}

#[test]
fn an_implicit_newer_target_stays_so_the_install_offer_remains() {
    let ollama = normalized(managed("0.30.4", "0.40.13", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
}

#[test]
fn an_equal_target_is_left_alone() {
    let ollama = normalized(managed("0.40.13", "0.40.13", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
}

#[test]
fn an_empty_target_becomes_the_recommended_runtime() {
    let ollama = normalized(managed("0.30.4", "", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
    assert!(!ollama.runtime_target_explicit);

    let whitespace = normalized(managed("0.30.4", "  ", true));
    assert_eq!(whitespace.runtime_target_version, "0.40.13");
    assert!(!whitespace.runtime_target_explicit, "an empty target cannot be a choice");
}

#[test]
fn the_legacy_default_is_replaced_unless_the_user_picked_it() {
    let implicit = normalized(managed("0.40.13", "0.20.2", false));
    assert_eq!(implicit.runtime_target_version, "0.40.13");

    let picked = normalized(managed("0.40.13", "0.20.2", true));
    assert_eq!(picked.runtime_target_version, "0.20.2", "an explicit pick is not a legacy default");

    // with an older runtime recorded the legacy default still becomes the recommended one
    let older = normalized(managed("0.30.4", "0.20.2", false));
    assert_eq!(older.runtime_target_version, "0.40.13");
}

#[test]
fn only_a_managed_runtime_has_a_target_to_heal() {
    for source in ["system", "manual"] {
        let mut settings = managed("0.50.1", "0.40.13", false);
        settings.providers.ollama.runtime_source = source.to_string();
        let ollama = normalized(settings);
        assert_eq!(ollama.runtime_target_version, "0.40.13", "source {}", source);
    }
}

#[test]
fn versions_that_are_not_plain_releases_never_trigger_a_change() {
    for running in ["", "unknown", "0.41.0-rc1", "0.41", "garbage"] {
        let ollama = normalized(managed(running, "0.30.4", false));
        assert_eq!(ollama.runtime_target_version, "0.30.4", "running {:?}", running);
    }
    // a v prefix on the recorded version is read like the plain version
    let ollama = normalized(managed("v0.40.13", "0.30.4", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
}

#[test]
fn comparison_is_numeric_not_textual() {
    // "0.9.0" sorts after "0.40.13" as text
    let ollama = normalized(managed("0.40.13", "0.9.0", false));
    assert_eq!(ollama.runtime_target_version, "0.40.13");
    let kept = normalized(managed("0.9.0", "0.40.13", false));
    assert_eq!(kept.runtime_target_version, "0.40.13");
}

#[test]
fn heal_reports_whether_it_changed_something() {
    let mut ollama = managed("0.40.13", "0.30.4", false).providers.ollama;
    assert!(ollama.heal_stale_runtime_target());
    assert!(!ollama.heal_stale_runtime_target(), "second call has nothing left to do");
    ollama.runtime_target_explicit = true;
    ollama.runtime_target_version = "0.30.4".to_string();
    assert!(!ollama.heal_stale_runtime_target());
}

#[test]
fn the_flag_survives_a_restart_the_settings_roundtrip() {
    // save -> file -> load (deserialize + normalize), twice
    for explicit in [false, true] {
        let mut settings = managed("0.40.13", "0.30.4", explicit);
        normalize_ai_fallback_fields(&mut settings);
        let json = serde_json::to_string(&settings).expect("serialize settings");
        assert!(json.contains("\"runtime_target_explicit\""), "the flag is persisted");

        let mut loaded: Settings = serde_json::from_str(&json).expect("load settings");
        normalize_ai_fallback_fields(&mut loaded);
        let again = serde_json::to_string(&loaded).unwrap();
        let mut reloaded: Settings = serde_json::from_str(&again).unwrap();
        normalize_ai_fallback_fields(&mut reloaded);

        let want = if explicit { "0.30.4" } else { "0.40.13" };
        assert_eq!(loaded.providers.ollama.runtime_target_version, want);
        assert_eq!(reloaded.providers.ollama.runtime_target_version, want);
        assert_eq!(reloaded.providers.ollama.runtime_target_explicit, explicit);
    }
}

#[test]
fn a_settings_file_from_before_the_flag_loads_as_not_explicit_and_is_healed() {
    let mut value = serde_json::to_value(managed("0.40.13", "0.30.4", true)).unwrap();
    value["providers"]["ollama"]
        .as_object_mut()
        .unwrap()
        .remove("runtime_target_explicit");
    let mut loaded: Settings = serde_json::from_value(value).expect("old file loads");
    assert!(!loaded.providers.ollama.runtime_target_explicit);
    normalize_ai_fallback_fields(&mut loaded);
    assert_eq!(loaded.providers.ollama.runtime_target_version, "0.40.13");
}

#[test]
fn a_frontend_snapshot_with_the_stale_target_is_healed_on_save() {
    // The frontend saves its whole copy of the settings. Backend: runtime switched to 0.40.13
    // and recorded; frontend copy: stale target. The save path runs the same normalization.
    let mut from_frontend = managed("0.40.13", "0.30.4", false);
    from_frontend.providers.ollama.runtime_target_explicit = false;
    let saved = normalized(from_frontend);
    assert_eq!(saved.runtime_target_version, "0.40.13");
}
