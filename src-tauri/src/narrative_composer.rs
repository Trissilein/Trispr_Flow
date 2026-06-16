use serde::{Deserialize, Serialize};
use std::fs;
use tauri::AppHandle;
use tauri::Manager;
use tracing::info;

use crate::ai_fallback::models::RefinementOptions;
use crate::ai_fallback::prepare_refinement;
use crate::state::AppState;
use crate::video_ingest::{JobWorkdir, SourceItem, SourceKind};

// ---------------------------------------------------------------------------
// Scene types  (Hybrid JSON Scene Script)
// ---------------------------------------------------------------------------

/// A single typed slide in the narrative scene script.
/// LLM produces a JSON array of these; HTML narration is limited to inline
/// tags in `body` only — the LLM never writes GSAP or full HTML blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scene {
    /// Opening title card.
    Cover {
        heading: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subheading: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_ref: Option<String>,
    },
    /// Narrative text slide. `body` may contain allowed inline HTML only.
    Body {
        heading: String,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speaker_note: Option<String>,
    },
    /// Full-bleed image with optional caption.
    ImageFocus {
        image_ref: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    /// Chapter-break / divider slide.
    Section { title: String },
    /// Closing slide.
    Outro {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cta: Option<String>,
    },
}

impl Scene {
    pub fn slide_duration_seconds(&self) -> f32 {
        match self {
            Scene::Cover { .. } => 4.0,
            Scene::Section { .. } => 2.5,
            Scene::Outro { .. } => 4.0,
            Scene::ImageFocus { .. } => 5.0,
            Scene::Body { body, .. } => {
                let words = body.split_whitespace().count();
                // ~150 wpm reading speed, min 4s, max 12s
                ((words as f32 / 2.5).ceil()).clamp(4.0, 12.0)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inline HTML allowlist
// ---------------------------------------------------------------------------

/// Strip any HTML tag not in the allowed inline set from `body` fields.
/// Keeps text content intact; only removes disallowed tags themselves.
fn sanitize_inline_html(input: &str) -> String {
    use std::sync::LazyLock;
    // Allow: strong, em, b, i, br, span, mark (and their closing forms).
    // Everything else is stripped (tag removed, content preserved).
    static RE_TAG: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"<(/?)(\w+)([^>]*)>").unwrap());

    const ALLOWED: &[&str] = &["strong", "em", "b", "i", "br", "span", "mark"];

    RE_TAG
        .replace_all(input, |caps: &regex::Captures| {
            let tag = caps[2].to_ascii_lowercase();
            if ALLOWED.contains(&tag.as_str()) {
                caps[0].to_string()
            } else {
                String::new()
            }
        })
        .into_owned()
}

// ---------------------------------------------------------------------------
// LLM scene-script generation
// ---------------------------------------------------------------------------

const SCENE_SCRIPT_SYSTEM_PROMPT: &str = r#"You are a video narrative composer. Given source content and a brief, produce a JSON array of typed scene objects for a video slideshow.

Output ONLY a valid JSON array — no prose, no markdown fences, no explanation.

Scene types and their fields:
- {"type":"cover","heading":"...","subheading":"...","image_ref":"assets/filename.ext"}
- {"type":"body","heading":"...","body":"narrative text (inline HTML ok: <strong> <em> <br>)","speaker_note":"..."}
- {"type":"image_focus","image_ref":"assets/filename.ext","caption":"..."}
- {"type":"section","title":"..."}
- {"type":"outro","cta":"..."}

Rules:
1. Always start with a cover scene and end with an outro scene.
2. Use section scenes to divide major chapters (3+ body scenes per section).
3. body.body must be narrative text — not raw content dumps. Summarize and contextualize.
4. image_ref values must match exactly one of the asset filenames provided below.
5. Aim for 6–14 scenes total. Prefer depth over breadth.
6. Output ONLY the JSON array.
"#;

/// Call the configured AI provider to generate a scene script from source items.
///
/// Reuses `prepare_refinement` (same provider/model/auth path as transcript
/// refinement). Returns a parsed `Vec<Scene>` or an error string.
pub fn generate_scene_script(
    app: &AppHandle,
    items: &[SourceItem],
    brief: &str,
) -> Result<Vec<Scene>, String> {
    let settings = {
        let state = app.state::<AppState>();
        let guard = state
            .settings
            .read()
            .map_err(|e| format!("settings lock poisoned: {e}"))?;
        guard.clone()
    };

    let setup = prepare_refinement(app, &settings)?;

    // Build the input text: brief + asset list + all extracted content.
    let asset_paths: Vec<String> = items
        .iter()
        .filter_map(|i| i.asset_path.as_ref())
        .cloned()
        .collect();
    let content_text: String = items
        .iter()
        .filter(|i| matches!(i.kind, SourceKind::Content | SourceKind::Hybrid))
        .filter_map(|i| i.extracted_text.as_ref())
        .map(|t| t.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let mut input = String::new();
    if !brief.trim().is_empty() {
        input.push_str("## Brief\n");
        input.push_str(brief.trim());
        input.push_str("\n\n");
    }
    if !asset_paths.is_empty() {
        input.push_str("## Available asset files\n");
        for p in &asset_paths {
            input.push_str(p);
            input.push('\n');
        }
        input.push('\n');
    }
    input.push_str("## Source content\n");
    input.push_str(&content_text);

    let options = RefinementOptions {
        temperature: 0.3,
        max_tokens: 4096,
        low_latency_mode: false,
        language: None,
        custom_prompt: Some(SCENE_SCRIPT_SYSTEM_PROMPT.to_string()),
        enforce_language_guard: false,
        prompt_profile: "custom".to_string(),
    };

    info!(
        "[narrative] calling {} model={} input_chars={}",
        setup.provider.id(),
        setup.model,
        input.len()
    );

    let result = setup
        .provider
        .refine_transcript(&input, &setup.model, &options, &setup.api_key)
        .map_err(|e| format!("Scene script LLM call failed: {e}"))?;

    parse_scene_script(&result.text)
}

/// Extract and parse the JSON array from LLM output.
/// LLM may include prose before/after; we extract the first `[...]` block.
fn parse_scene_script(raw: &str) -> Result<Vec<Scene>, String> {
    // Strip markdown code fences if present.
    let stripped = {
        let s = raw.trim();
        let s = s.strip_prefix("```json").unwrap_or(s);
        let s = s.strip_prefix("```").unwrap_or(s);
        let s = s.strip_suffix("```").unwrap_or(s);
        s.trim()
    };

    // Find the first `[` … last `]` span.
    let start = stripped.find('[').ok_or_else(|| {
        format!(
            "No JSON array found in LLM output:\n{}",
            &raw[..raw.len().min(200)]
        )
    })?;
    let end = stripped
        .rfind(']')
        .ok_or_else(|| "JSON array not closed in LLM output".to_string())?;

    if end < start {
        return Err("Malformed JSON array in LLM output".to_string());
    }

    let json_slice = &stripped[start..=end];
    let mut scenes: Vec<Scene> = serde_json::from_str(json_slice).map_err(|e| {
        format!(
            "Failed to parse scene script JSON: {e}\nJSON was:\n{}",
            &json_slice[..json_slice.len().min(400)]
        )
    })?;

    // Sanitize inline HTML in body fields.
    for scene in &mut scenes {
        if let Scene::Body { body, .. } = scene {
            *body = sanitize_inline_html(body);
        }
    }

    if scenes.is_empty() {
        return Err("LLM returned an empty scene array".to_string());
    }

    info!("[narrative] parsed {} scenes from LLM output", scenes.len());
    Ok(scenes)
}

// ---------------------------------------------------------------------------
// HTML composition from scene script
// ---------------------------------------------------------------------------

/// Generate the hyperframes project directory from a `Vec<Scene>`.
/// Replaces `compose_hyperframes_project` for the narrative path.
pub fn compose_narrative_project(
    scenes: &[Scene],
    resolution: &str,
    workdir: &JobWorkdir,
) -> Result<(), String> {
    let (width, height) = parse_resolution(resolution)?;

    let total_duration: f32 = scenes.iter().map(|s| s.slide_duration_seconds()).sum();
    let scenes_json =
        serde_json::to_string(scenes).map_err(|e| format!("serialize scenes: {e}"))?;

    let template = include_str!("../assets/video_templates/narrative.html.tmpl");
    let rendered = template
        .replace("{{SCENES_JSON}}", &scenes_json)
        .replace("{{WIDTH}}", &width.to_string())
        .replace("{{HEIGHT}}", &height.to_string())
        .replace("{{DURATION}}", &format!("{:.1}", total_duration));

    fs::write(workdir.root.join("index.html"), rendered)
        .map_err(|e| format!("write index.html: {e}"))?;

    fs::write(
        workdir.root.join("hyperframes.json"),
        r#"{
  "$schema": "https://hyperframes.heygen.com/schema/hyperframes.json",
  "registry": "https://raw.githubusercontent.com/heygen-com/hyperframes/main/registry",
  "paths": {
    "blocks": "compositions",
    "components": "compositions/components",
    "assets": "assets"
  }
}
"#,
    )
    .map_err(|e| format!("write hyperframes.json: {e}"))?;

    let meta = serde_json::json!({
        "id": workdir.job_id,
        "name": workdir.job_id,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    });
    fs::write(
        workdir.root.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".to_string()),
    )
    .map_err(|e| format!("write meta.json: {e}"))?;

    info!(
        "[narrative] composed {} scenes at {:?} ({}x{}, {:.1}s)",
        scenes.len(),
        workdir.root,
        width,
        height,
        total_duration
    );
    Ok(())
}

fn parse_resolution(res: &str) -> Result<(u32, u32), String> {
    let (w, h) = res
        .split_once('x')
        .ok_or_else(|| format!("invalid resolution '{res}': missing 'x'"))?;
    let width: u32 = w
        .parse()
        .map_err(|_| format!("invalid width in resolution '{res}'"))?;
    let height: u32 = h
        .parse()
        .map_err(|_| format!("invalid height in resolution '{res}'"))?;
    Ok((width, height))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scene_script_basic() {
        let raw = r#"[
            {"type":"cover","heading":"My Video","subheading":"A narrative"},
            {"type":"body","heading":"Key Point","body":"This is <strong>important</strong>."},
            {"type":"outro","cta":"Learn more"}
        ]"#;
        let scenes = parse_scene_script(raw).unwrap();
        assert_eq!(scenes.len(), 3);
        assert!(matches!(scenes[0], Scene::Cover { .. }));
        assert!(matches!(scenes[2], Scene::Outro { .. }));
    }

    #[test]
    fn parse_scene_script_strips_markdown_fence() {
        let raw = "```json\n[{\"type\":\"section\",\"title\":\"Chapter 1\"}]\n```";
        let scenes = parse_scene_script(raw).unwrap();
        assert_eq!(scenes.len(), 1);
        assert!(matches!(scenes[0], Scene::Section { .. }));
    }

    #[test]
    fn parse_scene_script_extracts_from_prose() {
        let raw = "Sure, here is your scene script:\n[{\"type\":\"outro\",\"cta\":\"Done\"}]\nHope that helps!";
        let scenes = parse_scene_script(raw).unwrap();
        assert_eq!(scenes.len(), 1);
    }

    #[test]
    fn parse_scene_script_empty_array_errors() {
        let err = parse_scene_script("[]").unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn sanitize_inline_html_allows_strong_em() {
        let input = "<strong>bold</strong> and <em>italic</em>";
        assert_eq!(sanitize_inline_html(input), input);
    }

    #[test]
    fn sanitize_inline_html_strips_script_div() {
        let input = "<div><script>alert(1)</script>text</div>";
        let out = sanitize_inline_html(input);
        assert!(!out.contains("<div>"), "got: {out}");
        assert!(!out.contains("<script>"), "got: {out}");
        assert!(out.contains("text"), "got: {out}");
    }

    #[test]
    fn body_scene_duration_clamps() {
        let short = Scene::Body {
            heading: "h".into(),
            body: "one two".into(),
            speaker_note: None,
        };
        assert!((short.slide_duration_seconds() - 4.0).abs() < 0.1);

        let long_body = "word ".repeat(200);
        let long = Scene::Body {
            heading: "h".into(),
            body: long_body,
            speaker_note: None,
        };
        assert!((long.slide_duration_seconds() - 12.0).abs() < 0.1);
    }
}
