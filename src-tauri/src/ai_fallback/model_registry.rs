//! Registry lookups for locally installed models: is the local build still the one the
//! registry serves for this tag? Also validates Hugging Face refs before a pull.
//!
//! Only two hosts are ever contacted: `registry.ollama.ai` and `huggingface.co`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::Duration;

const OLLAMA_REGISTRY_HOST: &str = "https://registry.ollama.ai";
const HUGGINGFACE_HOST: &str = "https://huggingface.co";
const MANIFEST_ACCEPT: &str = "application/vnd.docker.distribution.manifest.v2+json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_CONFIG_BLOB_BYTES: u64 = 64 * 1024;
const MAX_MODELS_PER_CHECK: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRegistry {
    Ollama,
    HuggingFace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub registry: ModelRegistry,
    /// `library/gemma4` / `user/model` (Ollama) or `org/repo` (Hugging Face).
    pub path: String,
    pub tag: String,
}

impl ModelRef {
    pub fn manifest_url(&self) -> String {
        let host = match self.registry {
            ModelRegistry::Ollama => OLLAMA_REGISTRY_HOST,
            ModelRegistry::HuggingFace => HUGGINGFACE_HOST,
        };
        format!("{}/v2/{}/manifests/{}", host, self.path, self.tag)
    }
}

fn is_name_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Parses `gemma4:e4b`, `user/model:tag`, `registry.ollama.ai/library/x:tag` and
/// `hf.co/org/repo:quant` into a registry reference.
pub fn parse_model_ref(name: &str) -> Result<ModelRef, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err("Model name is empty or too long.".to_string());
    }
    let (registry, rest) = if let Some(rest) = name
        .strip_prefix("hf.co/")
        .or_else(|| name.strip_prefix("huggingface.co/"))
    {
        (ModelRegistry::HuggingFace, rest)
    } else if let Some(rest) = name.strip_prefix("registry.ollama.ai/") {
        (ModelRegistry::Ollama, rest)
    } else {
        (ModelRegistry::Ollama, name)
    };

    let (path_part, tag) = match rest.rsplit_once(':') {
        Some((path, tag)) => (path, tag),
        None => (rest, "latest"),
    };
    if !is_name_segment(tag) {
        return Err(format!("Invalid model tag in '{}'.", name));
    }
    let segments: Vec<&str> = path_part.split('/').collect();
    if segments.iter().any(|s| !is_name_segment(s)) {
        return Err(format!("Invalid model path in '{}'.", name));
    }
    let path = match registry {
        ModelRegistry::HuggingFace => {
            if segments.len() != 2 {
                return Err(format!("Hugging Face refs need org/repo: '{}'.", name));
            }
            segments.join("/")
        }
        ModelRegistry::Ollama => match segments.len() {
            1 => format!("library/{}", segments[0]),
            2 => segments.join("/"),
            3 if segments[0] == "library" => segments[1..].join("/"),
            _ => return Err(format!("Unsupported model path in '{}'.", name)),
        },
    };
    Ok(ModelRef {
        registry,
        path,
        tag: tag.to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct RegistryManifest {
    /// Hex SHA-256 of the manifest body. NOT stable against what a local Ollama lists:
    /// older runtimes recompute the digest from their on-disk manifest, so it can differ
    /// for byte-identical weights.
    pub digest: String,
    pub total_size_bytes: u64,
    /// Hex digest of the `image.model` layer: the weights themselves. "Is my copy
    /// current?" is decided on this.
    pub model_layer_digest: Option<String>,
}

pub enum ManifestLookupError {
    NotFound,
    Failed(String),
}

fn registry_agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(8))
        .build()
}

/// GET with a size cap and the registry status mapping (404/400 = not found; for Hugging
/// Face also 401/403, which it answers for repos that do not exist, are private or gated).
fn http_get_capped(
    url: &str,
    registry: ModelRegistry,
    accept: &str,
    cap_bytes: u64,
) -> Result<Vec<u8>, ManifestLookupError> {
    let response = registry_agent()
        .get(url)
        .set("Accept", accept)
        .set("User-Agent", "TrisprFlow/ModelRegistry")
        .call();
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) | Err(ureq::Error::Status(400, _)) => {
            return Err(ManifestLookupError::NotFound)
        }
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _))
            if registry == ModelRegistry::HuggingFace =>
        {
            return Err(ManifestLookupError::NotFound)
        }
        Err(error) => return Err(ManifestLookupError::Failed(error.to_string())),
    };
    let mut body = Vec::new();
    response
        .into_reader()
        .take(cap_bytes)
        .read_to_end(&mut body)
        .map_err(|e| ManifestLookupError::Failed(e.to_string()))?;
    Ok(body)
}

/// `sha256:<64 hex>` (lower-cased) or None. Digests end up in blob URLs, so nothing else passes.
fn well_formed_sha256_digest(raw: &str) -> Option<String> {
    let hex = raw.trim().strip_prefix("sha256:")?;
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("sha256:{}", hex.to_ascii_lowercase()))
    } else {
        None
    }
}

pub fn fetch_registry_manifest(model: &ModelRef) -> Result<RegistryManifest, ManifestLookupError> {
    fetch_registry_manifest_at(&model.manifest_url(), model.registry)
}

fn fetch_registry_manifest_at(
    url: &str,
    registry: ModelRegistry,
) -> Result<RegistryManifest, ManifestLookupError> {
    let body = http_get_capped(url, registry, MANIFEST_ACCEPT, MAX_MANIFEST_BYTES)?;
    // A 200 that is not a manifest (a captive-portal login page, a proxy error page, an answer
    // cut off at the size cap) must not be digested and compared: its digest differs from every
    // local one, so every model would read "outdated" and invite a pointless re-download.
    let json = serde_json::from_slice::<serde_json::Value>(&body).map_err(|e| {
        ManifestLookupError::Failed(format!(
            "The registry answered with something that is not a manifest ({}).",
            e
        ))
    })?;
    let Some(layers) = json.get("layers").and_then(|v| v.as_array()) else {
        return Err(ManifestLookupError::Failed(
            "The registry answer has no layer list.".to_string(),
        ));
    };
    let digest = format!("{:x}", Sha256::digest(&body));
    // Sizes come from the network: add them saturating, a lying value must not wrap or panic.
    let total_size_bytes = layers
        .iter()
        .filter_map(|layer| layer.get("size").and_then(|v| v.as_u64()))
        .fold(0u64, |total, size| total.saturating_add(size));
    let model_layer_digest = layers
        .iter()
        .find(|layer| {
            layer
                .get("mediaType")
                .and_then(|v| v.as_str())
                .map(|t| t.ends_with("image.model"))
                .unwrap_or(false)
        })
        .and_then(|layer| layer.get("digest").and_then(|v| v.as_str()))
        .map(|d| d.trim_start_matches("sha256:").to_ascii_lowercase());
    Ok(RegistryManifest {
        digest,
        total_size_bytes,
        model_layer_digest,
    })
}

fn config_digest_of(manifest: &serde_json::Value) -> Option<String> {
    manifest
        .get("config")
        .and_then(|config| config.get("digest"))
        .and_then(|digest| digest.as_str())
        .and_then(well_formed_sha256_digest)
}

/// `"requires"` of a registry config blob: the minimum Ollama version that can run the model.
pub fn parse_requires_from_config_blob(body: &[u8]) -> Option<String> {
    let json = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let requires = json.get("requires")?.as_str()?.trim();
    if requires.is_empty() || requires.len() > 32 {
        None
    } else {
        Some(requires.to_string())
    }
}

/// Minimum Ollama version a library model needs, read from the registry without downloading
/// the model: manifest -> `config.digest` -> config blob -> `"requires"`.
/// `Ok(None)`: Hugging Face ref, or the config declares no requirement.
pub fn fetch_model_requires(model: &ModelRef) -> Result<Option<String>, ManifestLookupError> {
    fetch_model_requires_at(OLLAMA_REGISTRY_HOST, model)
}

fn fetch_model_requires_at(
    host: &str,
    model: &ModelRef,
) -> Result<Option<String>, ManifestLookupError> {
    if model.registry != ModelRegistry::Ollama {
        return Ok(None);
    }
    let manifest_url = format!("{}/v2/{}/manifests/{}", host, model.path, model.tag);
    let manifest = http_get_capped(
        &manifest_url,
        model.registry,
        MANIFEST_ACCEPT,
        MAX_MANIFEST_BYTES,
    )?;
    let manifest = serde_json::from_slice::<serde_json::Value>(&manifest)
        .map_err(|e| ManifestLookupError::Failed(format!("Unreadable manifest: {}", e)))?;
    let Some(config_digest) = config_digest_of(&manifest) else {
        return Ok(None);
    };
    let blob_url = format!("{}/v2/{}/blobs/{}", host, model.path, config_digest);
    let blob = http_get_capped(
        &blob_url,
        model.registry,
        "application/json",
        MAX_CONFIG_BLOB_BYTES,
    )
    .map_err(|error| match error {
        ManifestLookupError::NotFound => {
            ManifestLookupError::Failed("The registry has no config blob for this model.".into())
        }
        other => other,
    })?;
    Ok(parse_requires_from_config_blob(&blob))
}

/// Pull precheck for Hugging Face refs (the Ollama library precheck only knows library tags).
pub fn precheck_model_ref(model: &str) -> Result<(), String> {
    let parsed = parse_model_ref(model)?;
    if parsed.registry != ModelRegistry::HuggingFace {
        return super::provider::precheck_ollama_registry_model_tag(model);
    }
    match fetch_registry_manifest(&parsed) {
        Ok(_) => Ok(()),
        Err(ManifestLookupError::NotFound) => Err(format!(
            "Hugging Face model '{}' was not found, is private or gated, or has no GGUF with that quantization tag.",
            model
        )),
        Err(ManifestLookupError::Failed(reason)) => Err(format!(
            "Could not reach Hugging Face to check '{}': {}",
            model, reason
        )),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelUpdateCheckInput {
    pub name: String,
    pub digest: String,
    /// Hex digests of the local model's blobs (from `/api/show`); filled in by the command.
    #[serde(default)]
    pub local_layers: Vec<String>,
}

/// Blob digests referenced by a local model's Modelfile (`FROM`/`DRAFT`/`ADAPTER` paths
/// end in `blobs/sha256-<64 hex>`).
pub fn parse_modelfile_blob_digests(modelfile: &str) -> Vec<String> {
    const MARKER: &str = "sha256-";
    let mut found = Vec::new();
    let mut rest = modelfile;
    while let Some(idx) = rest.find(MARKER) {
        let after = &rest[idx + MARKER.len()..];
        let hex: String = after.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() == 64 {
            let hex = hex.to_ascii_lowercase();
            if !found.contains(&hex) {
                found.push(hex);
            }
        }
        rest = after;
    }
    found
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelUpdateStatus {
    pub name: String,
    /// "current" | "outdated" | "local" (not in a public registry) | "unknown"
    pub status: String,
    pub remote_digest: Option<String>,
    pub remote_size_bytes: Option<u64>,
    pub detail: Option<String>,
}

fn digests_match(local: &str, remote: &str) -> bool {
    let local = local.trim().trim_start_matches("sha256:").to_ascii_lowercase();
    let remote = remote.trim().trim_start_matches("sha256:").to_ascii_lowercase();
    if local.len() < 12 || remote.len() < 12 {
        return false;
    }
    let n = local.len().min(remote.len());
    // Bytes, not `&str` slices: the digest text comes from a server and may hold non-ASCII
    // characters, and a slice that ends inside one panics.
    local.as_bytes()[..n] == remote.as_bytes()[..n]
}

pub fn check_model_updates(inputs: Vec<ModelUpdateCheckInput>) -> Vec<ModelUpdateStatus> {
    check_model_updates_with(&fetch_registry_manifest, inputs)
}

/// `check_model_updates` with the manifest lookup as a parameter (tests stand in for the
/// registry).
fn check_model_updates_with(
    fetch: &dyn Fn(&ModelRef) -> Result<RegistryManifest, ManifestLookupError>,
    inputs: Vec<ModelUpdateCheckInput>,
) -> Vec<ModelUpdateStatus> {
    inputs
        .into_iter()
        .take(MAX_MODELS_PER_CHECK)
        .map(|input| {
            let unknown = |detail: String| ModelUpdateStatus {
                name: input.name.clone(),
                status: "unknown".to_string(),
                remote_digest: None,
                remote_size_bytes: None,
                detail: Some(detail),
            };
            let model = match parse_model_ref(&input.name) {
                Ok(model) => model,
                Err(error) => return unknown(error),
            };
            match fetch(&model) {
                Ok(manifest) => {
                    // Same weights = current, regardless of how this Ollama version
                    // computes the listed manifest digest.
                    let same_weights = manifest
                        .model_layer_digest
                        .as_ref()
                        .map(|layer| input.local_layers.iter().any(|local| local == layer))
                        .unwrap_or(false);
                    let current = same_weights || digests_match(&input.digest, &manifest.digest);
                    ModelUpdateStatus {
                        name: input.name.clone(),
                        status: if current { "current" } else { "outdated" }.to_string(),
                        remote_digest: Some(manifest.digest),
                        remote_size_bytes: Some(manifest.total_size_bytes),
                        detail: if same_weights {
                            Some("Same weights as the registry build.".to_string())
                        } else {
                            None
                        },
                    }
                }
                Err(ManifestLookupError::NotFound) => ModelUpdateStatus {
                    name: input.name.clone(),
                    status: "local".to_string(),
                    remote_digest: None,
                    remote_size_bytes: None,
                    detail: Some("Not available in a public registry.".to_string()),
                },
                Err(ManifestLookupError::Failed(reason)) => unknown(reason),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ollama_library_and_user_refs() {
        let lib = parse_model_ref("gemma4:e4b").unwrap();
        assert_eq!(lib.registry, ModelRegistry::Ollama);
        assert_eq!(lib.path, "library/gemma4");
        assert_eq!(lib.tag, "e4b");
        assert_eq!(
            lib.manifest_url(),
            "https://registry.ollama.ai/v2/library/gemma4/manifests/e4b"
        );

        let latest = parse_model_ref("llama3").unwrap();
        assert_eq!(latest.tag, "latest");

        let user = parse_model_ref("someone/custom-model:q4").unwrap();
        assert_eq!(user.path, "someone/custom-model");

        let prefixed = parse_model_ref("registry.ollama.ai/library/qwen3.5:4b").unwrap();
        assert_eq!(prefixed.path, "library/qwen3.5");
    }

    #[test]
    fn parses_hugging_face_refs_with_quant_tag() {
        let hf = parse_model_ref("hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M").unwrap();
        assert_eq!(hf.registry, ModelRegistry::HuggingFace);
        assert_eq!(hf.path, "unsloth/gemma-4-E4B-it-GGUF");
        assert_eq!(hf.tag, "Q4_K_M");
        assert_eq!(
            hf.manifest_url(),
            "https://huggingface.co/v2/unsloth/gemma-4-E4B-it-GGUF/manifests/Q4_K_M"
        );
        assert!(parse_model_ref("hf.co/onlyone:Q4").is_err());
    }

    #[test]
    fn rejects_traversal_and_foreign_hosts() {
        assert!(parse_model_ref("../etc/passwd").is_err());
        assert!(parse_model_ref("evil.example/ns/model:tag").is_err());
        assert!(parse_model_ref("a/b/c/d:tag").is_err());
        assert!(parse_model_ref("model:ta g").is_err());
        assert!(parse_model_ref("").is_err());
    }

    /// Hits the real registries. Run: cargo test --lib live_registry -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_registry_distinguishes_current_outdated_and_local_models() {
        let statuses = check_model_updates(vec![
            ModelUpdateCheckInput {
                name: "gemma4:e4b".to_string(),
                digest: "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c".to_string(),
                local_layers: vec![],
            },
            ModelUpdateCheckInput {
                name: "gemma4:e4b-it-q4_K_M".to_string(),
                digest: "c6eb396dbd5992bbe3f5cdb947e8bbc0ee413d7c17e2beaae69f5d569cf982eb".to_string(),
                local_layers: vec![],
            },
            ModelUpdateCheckInput {
                name: "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M".to_string(),
                digest: "487eacf5e94b".to_string(),
                local_layers: vec![],
            },
            ModelUpdateCheckInput {
                name: "my-private-model:latest".to_string(),
                digest: "abcdefabcdef".to_string(),
                local_layers: vec![],
            },
            // Same weights as the registry build, but a manifest digest an older runtime computed.
            ModelUpdateCheckInput {
                name: "gemma4:e4b".to_string(),
                digest: "ccd2e5fbe4af".to_string(),
                local_layers: vec![
                    "370c2879f17648d337b0ab4e208fdce348e24d1060064e93d7645daafd56d39c".to_string(),
                ],
            },
        ]);
        for s in &statuses {
            println!("{} -> {} remote={:?} size={:?}", s.name, s.status, s.remote_digest, s.remote_size_bytes);
        }
        assert_eq!(statuses[0].status, "current");
        assert_eq!(statuses[1].status, "outdated");
        assert_eq!(statuses[2].status, "current");
        assert_eq!(statuses[3].status, "local");
        assert_eq!(statuses[4].status, "current", "same weights must not read as outdated");
        assert!(precheck_model_ref("hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M").is_ok());
        assert!(precheck_model_ref("hf.co/unsloth/gemma-4-E4B-it-GGUF:NoSuchQuant").is_err());
        // A repo that does not exist answers 401 on Hugging Face: must read as "not found".
        let missing = precheck_model_ref("hf.co/tiiuae/does-not-exist-gguf-xyz:Q4_K_M")
            .expect_err("missing repo");
        assert!(missing.contains("not found"), "got: {}", missing);
    }

    // ── requires (runtime compatibility) ─────────────────────────────────────────

    const CONFIG_DIGEST: &str =
        "sha256:7ca1ae564b24fefd47da34cc579b906b75024860ed4e5bfbfa6d6cbd9777cee5";

    fn manifest_with_config(config_digest: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 2,
            "config": { "digest": config_digest, "size": 241 },
            "layers": [{ "mediaType": "application/vnd.ollama.image.model", "digest": "sha256:ab", "size": 1 }]
        })
    }

    fn registry_mock() -> crate::ai_fallback::test_support::MockServer {
        use crate::ai_fallback::test_support::{MockReply, MockServer};
        MockServer::start(|request| match request.path.as_str() {
            "/v2/library/gemma4/manifests/e4b" => MockReply::ok(manifest_with_config(CONFIG_DIGEST)),
            p if p == format!("/v2/library/gemma4/blobs/{}", CONFIG_DIGEST) => MockReply::ok(
                serde_json::json!({ "model_format": "gguf", "file_type": "Q4_K_M", "requires": "0.30.9" }),
            ),
            "/v2/library/plain/manifests/latest" => MockReply::ok(manifest_with_config(CONFIG_DIGEST)),
            p if p == format!("/v2/library/plain/blobs/{}", CONFIG_DIGEST) => {
                MockReply::ok(serde_json::json!({ "model_format": "gguf" }))
            }
            "/v2/library/noconfig/manifests/latest" => {
                MockReply::ok(serde_json::json!({ "schemaVersion": 2, "layers": [] }))
            }
            "/v2/library/badconfig/manifests/latest" => {
                MockReply::ok(manifest_with_config("sha256:../../../etc/passwd"))
            }
            "/v2/library/noblob/manifests/latest" => MockReply::ok(manifest_with_config(CONFIG_DIGEST)),
            _ => MockReply::error(404, "not found"),
        })
    }

    #[test]
    fn requires_comes_from_the_config_blob_behind_the_manifest() {
        let registry = registry_mock();
        let model = parse_model_ref("gemma4:e4b").unwrap();

        let requires = fetch_model_requires_at(&registry.base_url, &model)
            .ok()
            .expect("lookup succeeds");

        assert_eq!(requires.as_deref(), Some("0.30.9"));
        let paths: Vec<String> = registry.requests().into_iter().map(|r| r.path).collect();
        assert_eq!(
            paths,
            vec![
                "/v2/library/gemma4/manifests/e4b".to_string(),
                format!("/v2/library/gemma4/blobs/{}", CONFIG_DIGEST),
            ],
            "manifest first, then exactly the config blob it names"
        );
    }

    #[test]
    fn requires_is_none_when_the_config_declares_nothing() {
        let registry = registry_mock();
        // config blob without "requires"
        let plain = parse_model_ref("plain").unwrap();
        assert_eq!(
            fetch_model_requires_at(&registry.base_url, &plain).ok().expect("ok"),
            None
        );
        // manifest without a config entry: no blob request at all
        let noconfig = parse_model_ref("noconfig").unwrap();
        assert_eq!(
            fetch_model_requires_at(&registry.base_url, &noconfig).ok().expect("ok"),
            None
        );
        // a digest that is not sha256:<64 hex> is never put into a URL
        let badconfig = parse_model_ref("badconfig").unwrap();
        assert_eq!(
            fetch_model_requires_at(&registry.base_url, &badconfig).ok().expect("ok"),
            None
        );
        assert!(registry
            .requests()
            .iter()
            .all(|request| !request.path.contains("/blobs/") || request.path.contains("plain")));
    }

    #[test]
    fn requires_lookup_distinguishes_unknown_models_from_broken_registries() {
        let registry = registry_mock();
        let missing = parse_model_ref("nosuchmodel:1b").unwrap();
        assert!(matches!(
            fetch_model_requires_at(&registry.base_url, &missing),
            Err(ManifestLookupError::NotFound)
        ));
        // manifest exists but its config blob does not: a registry fault, not "unknown model"
        let noblob = parse_model_ref("noblob").unwrap();
        match fetch_model_requires_at(&registry.base_url, &noblob) {
            Err(ManifestLookupError::Failed(reason)) => assert!(reason.contains("config blob"), "{}", reason),
            _ => panic!("expected a Failed lookup"),
        }
    }

    #[test]
    fn hugging_face_refs_have_no_requirement_and_cause_no_request() {
        let registry = registry_mock();
        let hf = parse_model_ref("hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M").unwrap();
        assert_eq!(
            fetch_model_requires_at(&registry.base_url, &hf).ok().expect("ok"),
            None
        );
        assert!(registry.requests().is_empty());
    }

    #[test]
    fn config_blob_requires_is_read_defensively() {
        assert_eq!(
            parse_requires_from_config_blob(br#"{"requires":" 0.40.13 "}"#).as_deref(),
            Some("0.40.13")
        );
        assert_eq!(parse_requires_from_config_blob(br#"{"requires":""}"#), None);
        assert_eq!(parse_requires_from_config_blob(br#"{"requires":7}"#), None);
        assert_eq!(parse_requires_from_config_blob(br#"{"model_format":"gguf"}"#), None);
        assert_eq!(parse_requires_from_config_blob(b"<html>nope</html>"), None);
        let huge = format!("{{\"requires\":\"{}\"}}", "9".repeat(64));
        assert_eq!(parse_requires_from_config_blob(huge.as_bytes()), None);
    }

    #[test]
    fn digests_are_only_accepted_in_sha256_hex_form() {
        assert_eq!(
            well_formed_sha256_digest(CONFIG_DIGEST).as_deref(),
            Some(CONFIG_DIGEST)
        );
        assert!(well_formed_sha256_digest(&CONFIG_DIGEST.to_uppercase().replace("SHA256", "sha256")).is_some());
        for bad in ["", "sha256:", "sha256:abc", "md5:7ca1", "../../x", &format!("{}0", CONFIG_DIGEST)] {
            assert_eq!(well_formed_sha256_digest(bad), None, "{}", bad);
        }
    }

    #[test]
    fn extracts_blob_digests_from_a_modelfile() {
        let modelfile = "FROM D:\\Ollama\\models\\blobs\\sha256-370c2879f17648d337b0ab4e208fdce348e24d1060064e93d7645daafd56d39c\nDRAFT D:\\Ollama\\models\\blobs\\sha256-05612e54110f8f68bade01dfcc546cdc6914d11dde0b650a65f263f0f87bbbd2\nFROM D:\\x\\sha256-tooshort\nTEMPLATE hi";
        let digests = parse_modelfile_blob_digests(modelfile);
        assert_eq!(digests.len(), 2);
        assert_eq!(
            digests[0],
            "370c2879f17648d337b0ab4e208fdce348e24d1060064e93d7645daafd56d39c"
        );
    }

    // ── manifests: what a registry (or whatever answers in its place) may say ────

    use crate::ai_fallback::test_support::{MockReply, MockServer};

    fn manifest_url_on(base: &str, model: &ModelRef) -> String {
        format!("{}/v2/{}/manifests/{}", base, model.path, model.tag)
    }

    fn fetch_from(base: &str, name: &str) -> Result<RegistryManifest, ManifestLookupError> {
        let model = parse_model_ref(name).unwrap();
        fetch_registry_manifest_at(&manifest_url_on(base, &model), model.registry)
    }

    fn failed_reason(result: Result<RegistryManifest, ManifestLookupError>) -> String {
        match result {
            Err(ManifestLookupError::Failed(reason)) => reason,
            Err(ManifestLookupError::NotFound) => panic!("expected Failed, got NotFound"),
            Ok(manifest) => panic!("expected Failed, got a manifest: {:?}", manifest),
        }
    }

    #[test]
    fn manifest_sizes_that_overflow_do_not_panic() {
        let server = MockServer::start(|_| {
            MockReply::ok(serde_json::json!({
                "layers": [
                    { "mediaType": "application/vnd.ollama.image.model", "digest": "sha256:ab", "size": u64::MAX },
                    { "mediaType": "application/vnd.ollama.image.projector", "digest": "sha256:cd", "size": u64::MAX },
                    { "mediaType": "application/vnd.ollama.image.params", "digest": "sha256:ef", "size": 7 }
                ]
            }))
        });
        let manifest = fetch_from(&server.base_url, "gemma4:e4b").ok().expect("manifest");
        assert_eq!(manifest.total_size_bytes, u64::MAX, "a lying size saturates instead of wrapping or panicking");
        assert_eq!(manifest.model_layer_digest.as_deref(), Some("ab"));
    }

    #[test]
    fn a_200_that_is_not_a_manifest_is_a_failed_lookup_never_a_digest_to_compare() {
        for body in [
            "<html><body>Please log in to the hotel Wi-Fi</body></html>",
            "",
            "null",
            "{\"errors\":[{\"code\":\"MANIFEST_UNKNOWN\"}]}",
            "{\"layers\":\"not a list\"}",
        ] {
            let served = body.to_string();
            let server = MockServer::start(move |_| MockReply::text(200, &served));
            let reason = failed_reason(fetch_from(&server.base_url, "gemma4:e4b"));
            assert!(
                reason.contains("not a manifest") || reason.contains("no layer list"),
                "{:?}: {}",
                body,
                reason
            );
        }
    }

    #[test]
    fn a_manifest_cut_off_by_the_size_cap_is_a_failed_lookup() {
        let filler = "x".repeat((MAX_MANIFEST_BYTES as usize) + 100);
        let body = format!("{{\"layers\":[],\"pad\":\"{}\"}}", filler);
        let server = MockServer::start(move |_| MockReply::text(200, &body));
        failed_reason(fetch_from(&server.base_url, "gemma4:e4b"));
    }

    #[test]
    fn registry_hiccups_read_as_unknown_not_as_outdated() {
        // 429 / 5xx / captive portal for every model: nothing may be called outdated
        for reply in [
            MockReply::error(429, "slow down"),
            MockReply::error(503, "down"),
            MockReply::text(200, "<html>portal</html>"),
        ] {
            let status = reply.status;
            let body = reply.body.clone();
            let server = MockServer::start(move |_| MockReply::text(status, &body));
            let base = server.base_url.clone();
            let fetch = move |model: &ModelRef| {
                fetch_registry_manifest_at(&manifest_url_on(&base, model), model.registry)
            };
            let statuses = check_model_updates_with(
                &fetch,
                vec![ModelUpdateCheckInput {
                    name: "gemma4:e4b".into(),
                    digest: "ccd2e5fbe4af".into(),
                    local_layers: vec![],
                }],
            );
            assert_eq!(statuses[0].status, "unknown", "{:?}", statuses[0]);
            assert!(statuses[0].detail.is_some());
            assert!(statuses[0].remote_digest.is_none());
        }
    }

    #[test]
    fn a_real_manifest_is_still_compared() {
        let body = serde_json::json!({
            "layers": [{ "mediaType": "application/vnd.ollama.image.model", "digest": "sha256:AB12", "size": 5 }]
        })
        .to_string();
        let served = body.clone();
        let server = MockServer::start(move |_| MockReply::text(200, &served));
        let base = server.base_url.clone();
        let fetch = move |model: &ModelRef| {
            fetch_registry_manifest_at(&manifest_url_on(&base, model), model.registry)
        };
        let same_weights = check_model_updates_with(
            &fetch,
            vec![ModelUpdateCheckInput {
                name: "gemma4:e4b".into(),
                digest: "0000000000000".into(),
                local_layers: vec!["ab12".into()],
            }],
        );
        assert_eq!(same_weights[0].status, "current");
        let other = check_model_updates_with(
            &fetch,
            vec![ModelUpdateCheckInput {
                name: "gemma4:e4b".into(),
                digest: "0000000000000".into(),
                local_layers: vec![],
            }],
        );
        assert_eq!(other[0].status, "outdated");
        assert_eq!(other[0].remote_digest.as_deref().map(str::len), Some(64));
    }

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
    fn registry_parsers_survive_hostile_text() {
        for text in hostile_strings(20_000) {
            let _ = parse_model_ref(&text);
            let _ = parse_modelfile_blob_digests(&text);
            let _ = digests_match(&text, "dc35e8d9c6061baa");
            let _ = digests_match(&text, &text);
            let _ = parse_requires_from_config_blob(text.as_bytes());
            let _ = well_formed_sha256_digest(&text);
            let _ = check_model_updates_with(
                &|_| Err(ManifestLookupError::NotFound),
                vec![ModelUpdateCheckInput {
                    name: text.clone(),
                    digest: text.clone(),
                    local_layers: vec![text.clone()],
                }],
            );
        }
    }

    #[test]
    fn digest_comparison_never_panics_on_non_ascii_text() {
        // a local server (or a damaged listing) can put anything into the digest field
        let ascii = "a".repeat(17);
        // 18 bytes: byte 17 lies inside the two-byte "ä", which used to be a panic
        let accented = format!("{}ä", "a".repeat(16));
        assert!(!digests_match(&accented, &ascii));
        assert!(!digests_match(&ascii, &accented));
        for hostile in ["日本語日本語日本語日本語", "🦙🦙🦙🦙🦙🦙🦙🦙", "ÄÖÜÄÖÜÄÖÜÄÖÜ"] {
            assert!(!digests_match(hostile, "dc35e8d9c6061baa"));
            assert!(!digests_match("dc35e8d9c6061baa", hostile));
            assert!(digests_match(hostile, hostile), "identical text is identical");
        }
    }

    #[test]
    fn digest_comparison_ignores_prefix_and_case() {
        let full = "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c";
        assert!(digests_match(full, "dc35e8d9c606"));
        assert!(digests_match("sha256:DC35E8D9C6061BAA", full));
        assert!(!digests_match("c6eb396dbd59", full));
        assert!(!digests_match("abc", "abc"));
    }
}
