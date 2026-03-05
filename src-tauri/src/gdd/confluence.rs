use base64::Engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tauri::AppHandle;

use crate::confluence::keyring as confluence_keyring;
use crate::modules::ConfluenceSettings;

const CONFLUENCE_API_TOKEN_SECRET_ID: &str = "confluence_api_token";
const CONFLUENCE_OAUTH_ACCESS_SECRET_ID: &str = "confluence_oauth_access_token";
const CONFLUENCE_OAUTH_REFRESH_SECRET_ID: &str = "confluence_oauth_refresh_token";
const ATLASSIAN_OAUTH_TOKEN_URL: &str = "https://auth.atlassian.com/oauth/token";
const ATLASSIAN_OAUTH_RESOURCES_URL: &str = "https://api.atlassian.com/oauth/token/accessible-resources";
const PUBLISH_UPDATE_MAX_ATTEMPTS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceSpace {
    pub id: String,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceConnectionResult {
    pub ok: bool,
    pub message: String,
    pub spaces_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceTargetSuggestionRequest {
    pub title: String,
    pub preset_id: Option<String>,
    pub space_key: Option<String>,
    pub parent_page_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceTargetSuggestion {
    pub space_key: String,
    pub parent_page_id: Option<String>,
    pub existing_page_id: Option<String>,
    pub confidence: f32,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluencePageTemplateResult {
    pub page_id: String,
    pub page_title: String,
    pub source_url: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluencePublishRequest {
    pub title: String,
    pub storage_body: String,
    pub space_key: String,
    pub parent_page_id: Option<String>,
    pub target_page_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluencePublishResult {
    pub page_id: String,
    pub page_url: String,
    pub created: bool,
    pub version: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceOauthResource {
    pub id: String,
    pub url: String,
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceOauthExchangeResult {
    pub message: String,
    pub selected_cloud_id: String,
    pub selected_site_url: String,
    pub selected_site_name: String,
    pub resources: Vec<ConfluenceOauthResource>,
    pub expires_in_seconds: u64,
    pub refresh_token_saved: bool,
}

#[derive(Debug, Clone)]
struct ConfluenceOauthClientConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

fn normalize_base_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Confluence site URL is empty.".to_string());
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Confluence site URL must start with http:// or https://".to_string());
    }
    Ok(trimmed.to_string())
}

fn normalize_api_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    }
}

pub fn routing_key_for(space_key: &str, title: &str) -> String {
    format!(
        "{}::{}",
        space_key.trim().to_lowercase(),
        title.trim().to_lowercase()
    )
}

fn oauth_client_config_from_env() -> Result<ConfluenceOauthClientConfig, String> {
    let client_id = std::env::var("TRISPR_CONFLUENCE_OAUTH_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("TRISPR_CONFLUENCE_OAUTH_CLIENT_SECRET").unwrap_or_default();
    let redirect_uri = std::env::var("TRISPR_CONFLUENCE_OAUTH_REDIRECT_URI").unwrap_or_default();

    if client_id.trim().is_empty()
        || client_secret.trim().is_empty()
        || redirect_uri.trim().is_empty()
    {
        return Err(
            "OAuth is not configured. Set TRISPR_CONFLUENCE_OAUTH_CLIENT_ID, TRISPR_CONFLUENCE_OAUTH_CLIENT_SECRET and TRISPR_CONFLUENCE_OAUTH_REDIRECT_URI."
                .to_string(),
        );
    }

    Ok(ConfluenceOauthClientConfig {
        client_id: client_id.trim().to_string(),
        client_secret: client_secret.trim().to_string(),
        redirect_uri: redirect_uri.trim().to_string(),
    })
}

fn error_body(response: ureq::Response) -> String {
    response
        .into_string()
        .map(|body| body.trim().to_string())
        .unwrap_or_else(|_| String::new())
}

fn map_ureq_error(context: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = error_body(response);
            if body.is_empty() {
                format!("{} (HTTP {})", context, code)
            } else {
                format!("{} (HTTP {}): {}", context, code, body)
            }
        }
        ureq::Error::Transport(transport) => format!("{}: {}", context, transport),
    }
}

fn http_status_from_error(error: &str) -> Option<u16> {
    let marker = "(HTTP ";
    let start = error.find(marker)? + marker.len();
    let digits = error[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u16>().ok()
}

fn is_publish_update_conflict(error: &str) -> bool {
    if matches!(http_status_from_error(error), Some(409)) {
        return true;
    }
    let error_lc = error.to_ascii_lowercase();
    error_lc.contains("conflict") && error_lc.contains("version")
}

fn should_retry_publish_update(attempt: usize, error: &str) -> bool {
    attempt < PUBLISH_UPDATE_MAX_ATTEMPTS && is_publish_update_conflict(error)
}

fn parse_json_response(response: ureq::Response, context: &str) -> Result<serde_json::Value, String> {
    response
        .into_json::<serde_json::Value>()
        .map_err(|err| format!("{}: failed to parse JSON response: {}", context, err))
}

fn refresh_oauth_access_token(app: &AppHandle) -> Result<bool, String> {
    let Some(refresh_token) = confluence_keyring::read_secret(app, CONFLUENCE_OAUTH_REFRESH_SECRET_ID)?
    else {
        return Ok(false);
    };

    let config = oauth_client_config_from_env()?;
    let body = json!({
        "grant_type": "refresh_token",
        "client_id": config.client_id,
        "client_secret": config.client_secret,
        "refresh_token": refresh_token,
    });

    let response = shared_agent()
        .post(ATLASSIAN_OAUTH_TOKEN_URL)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|error| map_ureq_error("Confluence OAuth token refresh failed", error))?;
    let token_json = parse_json_response(response, "Confluence OAuth token refresh failed")?;

    let access_token = token_json
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Confluence OAuth refresh did not return an access token.".to_string())?;

    confluence_keyring::store_secret(app, CONFLUENCE_OAUTH_ACCESS_SECRET_ID, access_token)?;

    if let Some(rotated_refresh) = token_json
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        confluence_keyring::store_secret(app, CONFLUENCE_OAUTH_REFRESH_SECRET_ID, rotated_refresh)?;
    }

    Ok(true)
}

fn auth_header(app: &AppHandle, settings: &ConfluenceSettings) -> Result<String, String> {
    let mode = settings.auth_mode.trim();
    if mode == "api_token" {
        let email = settings.api_user_email.trim();
        if email.is_empty() {
            return Err("Confluence API token mode requires account email.".to_string());
        }
        let token = confluence_keyring::read_secret(app, CONFLUENCE_API_TOKEN_SECRET_ID)?
            .ok_or_else(|| "No Confluence API token stored.".to_string())?;
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", email, token));
        return Ok(format!("Basic {}", basic));
    }

    let mut oauth = confluence_keyring::read_secret(app, CONFLUENCE_OAUTH_ACCESS_SECRET_ID)?;
    if oauth.is_none() {
        let refreshed = refresh_oauth_access_token(app)?;
        if refreshed {
            oauth = confluence_keyring::read_secret(app, CONFLUENCE_OAUTH_ACCESS_SECRET_ID)?;
        }
    }

    let oauth = oauth.ok_or_else(|| {
        "No Confluence OAuth access token stored. Complete OAuth sign-in first.".to_string()
    })?;
    Ok(format!("Bearer {}", oauth))
}

fn shared_agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(30))
        .build()
}

fn api_base_url(settings: &ConfluenceSettings) -> Result<String, String> {
    if settings.auth_mode.trim() == "oauth" {
        let cloud_id = settings.oauth_cloud_id.trim();
        if cloud_id.is_empty() {
            return Err(
                "Confluence OAuth site is not selected yet. Run OAuth exchange and pick a workspace."
                    .to_string(),
            );
        }
        return Ok(format!("https://api.atlassian.com/ex/confluence/{}", cloud_id));
    }
    normalize_base_url(&settings.site_base_url)
}

fn request_url(settings: &ConfluenceSettings, path: &str) -> Result<String, String> {
    let base = api_base_url(settings)?;
    Ok(format!("{}{}", base, normalize_api_path(path)))
}

fn send_json_request(
    method: &str,
    url: &str,
    auth_header_value: &str,
    body: Option<serde_json::Value>,
) -> Result<ureq::Response, ureq::Error> {
    let request = shared_agent()
        .request(method, url)
        .set("Accept", "application/json")
        .set("Authorization", auth_header_value);

    match body {
        Some(payload) => request
            .set("Content-Type", "application/json")
            .send_json(payload),
        None => request.call(),
    }
}

fn request_json(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let url = request_url(settings, path)?;
    let auth = auth_header(app, settings)?;

    let first_try = send_json_request(method, &url, &auth, body.clone());
    match first_try {
        Ok(response) => return parse_json_response(response, "Confluence response parse failed"),
        Err(ureq::Error::Status(401, _)) if settings.auth_mode.trim() == "oauth" => {
            let refreshed = refresh_oauth_access_token(app)?;
            if !refreshed {
                return Err(
                    "Confluence OAuth token expired and refresh token is unavailable. Reconnect OAuth."
                        .to_string(),
                );
            }
            let retry_auth = auth_header(app, settings)?;
            let retry = send_json_request(method, &url, &retry_auth, body).map_err(|error| {
                map_ureq_error("Confluence request failed after OAuth refresh", error)
            })?;
            parse_json_response(retry, "Confluence response parse failed after OAuth refresh")
        }
        Err(error) => Err(map_ureq_error("Confluence request failed", error)),
    }
}

fn get_json(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    path: &str,
) -> Result<serde_json::Value, String> {
    request_json(app, settings, "GET", path, None)
}

fn post_json(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    request_json(app, settings, "POST", path, Some(body))
}

fn put_json(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    request_json(app, settings, "PUT", path, Some(body))
}

pub fn list_spaces(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
) -> Result<Vec<ConfluenceSpace>, String> {
    let json = get_json(app, settings, "/wiki/rest/api/space?limit=200")?;
    let spaces = json
        .get("results")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?.to_string();
            let key = row.get("key")?.as_str()?.to_string();
            let name = row.get("name")?.as_str()?.to_string();
            Some(ConfluenceSpace { id, key, name })
        })
        .collect::<Vec<_>>();
    Ok(spaces)
}

pub fn test_connection(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
) -> Result<ConfluenceConnectionResult, String> {
    let spaces = list_spaces(app, settings)?;
    Ok(ConfluenceConnectionResult {
        ok: true,
        message: "Confluence connection verified.".to_string(),
        spaces_count: spaces.len(),
    })
}

fn search_existing_page(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
    space_key: &str,
    title: &str,
) -> Result<Option<(String, String)>, String> {
    let escaped_title = title.replace('"', "\\\"");
    let cql = format!(
        "space=\"{}\" and type=page and title=\"{}\"",
        space_key, escaped_title
    );
    let encoded = url::form_urlencoded::byte_serialize(cql.as_bytes()).collect::<String>();
    let path = format!("/wiki/rest/api/content/search?cql={}&limit=1", encoded);
    let json = get_json(app, settings, &path)?;

    let row = json
        .get("results")
        .and_then(|value| value.as_array())
        .and_then(|rows| rows.first());

    let Some(row) = row else {
        return Ok(None);
    };

    let page_id = row
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let page_title = row
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    if page_id.is_empty() {
        return Ok(None);
    }

    Ok(Some((page_id, page_title)))
}

fn extract_page_id_from_url(source_url: &str) -> Option<String> {
    let trimmed = source_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        if let Some((_, value)) = url
            .query_pairs()
            .find(|(key, value)| key == "pageId" && !value.is_empty())
        {
            return Some(value.to_string());
        }

        let segments = url
            .path_segments()
            .map(|value| value.collect::<Vec<_>>())
            .unwrap_or_default();
        for window in segments.windows(2) {
            if window[0] == "pages" && window[1].chars().all(|ch| ch.is_ascii_digit()) {
                return Some(window[1].to_string());
            }
        }
    }

    let regex = Regex::new(r"(?i)(?:pageId=|/pages/)(\d+)").expect("valid confluence page regex");
    regex
        .captures(trimmed)
        .and_then(|caps| caps.get(1))
        .map(|match_| match_.as_str().to_string())
}

fn confluence_storage_to_text(storage_html: &str) -> String {
    let mut text = storage_html
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("</p>", "\n")
        .replace("</li>", "\n")
        .replace("</tr>", "\n")
        .replace("</h1>", "\n")
        .replace("</h2>", "\n")
        .replace("</h3>", "\n")
        .replace("</h4>", "\n")
        .replace("</h5>", "\n")
        .replace("</h6>", "\n");
    let tags = Regex::new(r"<[^>]+>").expect("valid html strip regex");
    text = tags.replace_all(&text, " ").into_owned();
    text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let cleaned = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(cleaned);
    }
    out
}

pub fn load_page_template_from_url(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    source_url: &str,
) -> Result<ConfluencePageTemplateResult, String> {
    let page_id = extract_page_id_from_url(source_url)
        .ok_or_else(|| "Could not extract Confluence page id from URL.".to_string())?;
    let path = format!(
        "/wiki/rest/api/content/{}?expand=title,body.storage",
        page_id
    );
    let json = get_json(app, settings, &path)?;

    let page_title = json
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Confluence Template")
        .to_string();

    let storage = json
        .get("body")
        .and_then(|value| value.get("storage"))
        .and_then(|value| value.get("value"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Confluence page has no storage body content.".to_string())?;
    let text = confluence_storage_to_text(storage);
    if text.trim().is_empty() {
        return Err("Confluence page content is empty after text extraction.".to_string());
    }

    Ok(ConfluencePageTemplateResult {
        page_id,
        page_title,
        source_url: source_url.trim().to_string(),
        text,
    })
}

pub fn suggest_target(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
    request: &ConfluenceTargetSuggestionRequest,
) -> Result<ConfluenceTargetSuggestion, String> {
    let space_key = request
        .space_key
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let key = settings.default_space_key.trim();
            if key.is_empty() {
                None
            } else {
                Some(key.to_string())
            }
        })
        .ok_or_else(|| "No Confluence space key configured for target suggestion.".to_string())?;

    let route_key = routing_key_for(&space_key, &request.title);
    if let Some(memory_page_id) = settings
        .routing_memory
        .get(&route_key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    {
        let parent_page_id = request
            .parent_page_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let parent = settings.default_parent_page_id.trim();
                if parent.is_empty() {
                    None
                } else {
                    Some(parent.to_string())
                }
            });

        return Ok(ConfluenceTargetSuggestion {
            space_key,
            parent_page_id,
            existing_page_id: Some(memory_page_id),
            confidence: 0.93,
            reasoning:
                "Using saved routing memory for this space/title pair from a previous publish."
                    .to_string(),
        });
    }

    let existing = search_existing_page(app, settings, &space_key, &request.title)?;

    let parent_page_id = request
        .parent_page_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let parent = settings.default_parent_page_id.trim();
            if parent.is_empty() {
                None
            } else {
                Some(parent.to_string())
            }
        });

    let (existing_page_id, confidence, reasoning) = if let Some((page_id, page_title)) = existing {
        (
            Some(page_id),
            0.86,
            format!(
                "Found existing page with matching title in space {}: {}",
                space_key, page_title
            ),
        )
    } else {
        (
            None,
            0.62,
            "No exact page match found. Suggest creating under configured parent.".to_string(),
        )
    };

    Ok(ConfluenceTargetSuggestion {
        space_key,
        parent_page_id,
        existing_page_id,
        confidence,
        reasoning,
    })
}

fn fetch_page_version(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
    page_id: &str,
) -> Result<u64, String> {
    let path = format!("/wiki/rest/api/content/{}?expand=version", page_id);
    let json = get_json(app, settings, &path)?;
    let version = json
        .get("version")
        .and_then(|value| value.get("number"))
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "Failed to read Confluence page version.".to_string())?;
    Ok(version)
}

pub fn publish(
    app: &tauri::AppHandle,
    settings: &ConfluenceSettings,
    request: &ConfluencePublishRequest,
) -> Result<ConfluencePublishResult, String> {
    let space_key = request.space_key.trim();
    if space_key.is_empty() {
        return Err("Confluence publish requires a space key.".to_string());
    }
    let title = request.title.trim();
    if title.is_empty() {
        return Err("Confluence publish requires a title.".to_string());
    }

    let target_page_id = if let Some(page_id) = request
        .target_page_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(page_id)
    } else {
        search_existing_page(app, settings, space_key, title)?.map(|(page_id, _)| page_id)
    };

    let base_url = normalize_base_url(&settings.site_base_url)?;

    if let Some(page_id) = target_page_id {
        let path = format!("/wiki/rest/api/content/{}", page_id);
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            let current_version = fetch_page_version(app, settings, &page_id)?;
            let body = json!({
                "id": page_id,
                "type": "page",
                "title": title,
                "version": { "number": current_version + 1 },
                "body": {
                    "storage": {
                        "value": request.storage_body,
                        "representation": "storage"
                    }
                }
            });
            match put_json(app, settings, &path, body) {
                Ok(response) => {
                    let final_id = response
                        .get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let final_version = response
                        .get("version")
                        .and_then(|value| value.get("number"))
                        .and_then(|value| value.as_u64())
                        .unwrap_or(current_version + 1);

                    return Ok(ConfluencePublishResult {
                        page_id: final_id.clone(),
                        page_url: format!("{}/wiki/pages/viewpage.action?pageId={}", base_url, final_id),
                        created: false,
                        version: final_version,
                        message: "Confluence page updated.".to_string(),
                    });
                }
                Err(error) if should_retry_publish_update(attempt, &error) => continue,
                Err(error) if is_publish_update_conflict(&error) => {
                    return Err(format!(
                        "Confluence update conflict persisted after retry. {}",
                        error
                    ))
                }
                Err(error) => return Err(error),
            }
        }
    }

    let ancestors = request
        .parent_page_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|parent_id| vec![json!({ "id": parent_id })])
        .unwrap_or_default();

    let body = json!({
        "type": "page",
        "title": title,
        "space": { "key": space_key },
        "ancestors": ancestors,
        "body": {
            "storage": {
                "value": request.storage_body,
                "representation": "storage"
            }
        }
    });

    let response = post_json(app, settings, "/wiki/rest/api/content", body)?;
    let page_id = response
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Confluence did not return a page id.".to_string())?
        .to_string();
    let version = response
        .get("version")
        .and_then(|value| value.get("number"))
        .and_then(|value| value.as_u64())
        .unwrap_or(1);

    Ok(ConfluencePublishResult {
        page_id: page_id.clone(),
        page_url: format!("{}/wiki/pages/viewpage.action?pageId={}", base_url, page_id),
        created: true,
        version,
        message: "Confluence page created.".to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluenceOauthStartResult {
    pub auth_url: String,
    pub message: String,
}

fn exchange_oauth_code(code: &str) -> Result<(String, Option<String>, u64), String> {
    let code = code.trim();
    if code.is_empty() {
        return Err("OAuth code is empty.".to_string());
    }

    let config = oauth_client_config_from_env()?;
    let body = json!({
        "grant_type": "authorization_code",
        "client_id": config.client_id,
        "client_secret": config.client_secret,
        "code": code,
        "redirect_uri": config.redirect_uri,
    });

    let response = shared_agent()
        .post(ATLASSIAN_OAUTH_TOKEN_URL)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|error| map_ureq_error("Confluence OAuth code exchange failed", error))?;
    let token_json = parse_json_response(response, "Confluence OAuth code exchange failed")?;

    let access_token = token_json
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Confluence OAuth exchange did not return an access token.".to_string())?
        .to_string();
    let refresh_token = token_json
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let expires_in = token_json
        .get("expires_in")
        .and_then(|value| value.as_u64())
        .unwrap_or(3600);

    Ok((access_token, refresh_token, expires_in))
}

fn fetch_accessible_resources(access_token: &str) -> Result<Vec<ConfluenceOauthResource>, String> {
    let response = shared_agent()
        .get(ATLASSIAN_OAUTH_RESOURCES_URL)
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {}", access_token))
        .call()
        .map_err(|error| map_ureq_error("Confluence accessible-resources request failed", error))?;
    let resources_json = parse_json_response(response, "Confluence accessible-resources failed")?;
    let resources = resources_json
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|resource| {
            let id = resource.get("id")?.as_str()?.trim().to_string();
            let url = resource.get("url")?.as_str()?.trim().trim_end_matches('/').to_string();
            let name = resource
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Confluence Site")
                .to_string();
            if id.is_empty() || url.is_empty() {
                return None;
            }
            let scopes = resource
                .get("scopes")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|scope| scope.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            Some(ConfluenceOauthResource {
                id,
                url,
                name,
                scopes,
            })
        })
        .collect::<Vec<_>>();
    Ok(resources)
}

fn select_resource(
    resources: &[ConfluenceOauthResource],
    preferred_site_base_url: &str,
) -> Option<ConfluenceOauthResource> {
    if resources.is_empty() {
        return None;
    }

    if let Ok(preferred) = normalize_base_url(preferred_site_base_url) {
        if let Some(found) = resources.iter().find(|resource| {
            normalize_base_url(&resource.url)
                .ok()
                .map(|normalized| normalized == preferred)
                .unwrap_or(false)
        }) {
            return Some(found.clone());
        }
    }

    resources
        .iter()
        .find(|resource| {
            let has_read = resource.scopes.iter().any(|scope| {
                scope == "read:confluence-content.all" || scope == "read:confluence-content.summary"
            });
            let has_write = resource
                .scopes
                .iter()
                .any(|scope| scope == "write:confluence-content");
            has_read && has_write
        })
        .cloned()
        .or_else(|| resources.first().cloned())
}

pub fn oauth_start() -> Result<ConfluenceOauthStartResult, String> {
    let config = oauth_client_config_from_env()?;

    let scope = "read:confluence-content.all write:confluence-content read:confluence-space.summary";
    let auth_url = format!(
        "https://auth.atlassian.com/authorize?audience=api.atlassian.com&client_id={}&scope={}&redirect_uri={}&state=trispr_flow&response_type=code&prompt=consent",
        url::form_urlencoded::byte_serialize(config.client_id.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(scope.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(config.redirect_uri.as_bytes()).collect::<String>()
    );

    Ok(ConfluenceOauthStartResult {
        auth_url,
        message: "Open the URL in browser, then exchange the returned code.".to_string(),
    })
}

pub fn oauth_exchange(
    app: &AppHandle,
    settings: &ConfluenceSettings,
    code: &str,
) -> Result<ConfluenceOauthExchangeResult, String> {
    let (access_token, refresh_token, expires_in) = exchange_oauth_code(code)?;
    confluence_keyring::store_secret(app, CONFLUENCE_OAUTH_ACCESS_SECRET_ID, &access_token)?;
    if let Some(refresh) = &refresh_token {
        confluence_keyring::store_secret(app, CONFLUENCE_OAUTH_REFRESH_SECRET_ID, refresh)?;
    }

    let resources = fetch_accessible_resources(&access_token)?;
    let selected = select_resource(&resources, &settings.site_base_url).ok_or_else(|| {
        "OAuth succeeded but no Confluence Cloud site is accessible for this account.".to_string()
    })?;

    Ok(ConfluenceOauthExchangeResult {
        message: "OAuth exchange completed.".to_string(),
        selected_cloud_id: selected.id.clone(),
        selected_site_url: selected.url.clone(),
        selected_site_name: selected.name.clone(),
        resources,
        expires_in_seconds: expires_in,
        refresh_token_saved: refresh_token.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        http_status_from_error, is_publish_update_conflict, should_retry_publish_update,
        PUBLISH_UPDATE_MAX_ATTEMPTS,
    };

    #[test]
    fn extracts_http_status_from_error_text() {
        let error = "Confluence request failed (HTTP 409): {\"message\":\"Conflict\"}";
        assert_eq!(http_status_from_error(error), Some(409));
    }

    #[test]
    fn detects_publish_conflict_from_http_409() {
        let error = "Confluence request failed (HTTP 409): version conflict";
        assert!(is_publish_update_conflict(error));
    }

    #[test]
    fn detects_publish_conflict_from_text_hints() {
        let error = "Confluence request failed: Version conflict while updating content";
        assert!(is_publish_update_conflict(error));
    }

    #[test]
    fn retries_only_once_for_conflict() {
        let error = "Confluence request failed (HTTP 409): version conflict";
        assert!(should_retry_publish_update(1, error));
        assert!(!should_retry_publish_update(PUBLISH_UPDATE_MAX_ATTEMPTS, error));
    }

    #[test]
    fn does_not_retry_for_non_conflict_errors() {
        let error = "Confluence request failed (HTTP 500): internal error";
        assert!(!should_retry_publish_update(1, error));
    }
}
