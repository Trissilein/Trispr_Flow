use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::warn;
use zip::ZipArchive;

const MAX_TEMPLATE_CHARS: usize = 24_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddTemplateSourceResult {
    pub source_kind: String,
    pub source_label: String,
    pub source_ref: String,
    pub text: String,
    pub original_chars: usize,
    pub truncated: bool,
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn cleanup_markup_text(input: &str) -> String {
    let mut text = input
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("</tr>", "\n")
        .replace("</h1>", "\n")
        .replace("</h2>", "\n")
        .replace("</h3>", "\n")
        .replace("</h4>", "\n")
        .replace("</h5>", "\n")
        .replace("</h6>", "\n");

    let re_tags = Regex::new(r"<[^>]+>").expect("valid html tag regex");
    text = re_tags.replace_all(&text, " ").into_owned();
    text = decode_entities(&text);

    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let normalized_line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized_line = normalized_line.trim();
        if normalized_line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(normalized_line);
    }
    out
}

fn finalize_result(source_kind: &str, source_label: String, source_ref: String, text: String) -> GddTemplateSourceResult {
    let cleaned = cleanup_markup_text(&text);
    let original_chars = cleaned.chars().count();
    let mut truncated = false;

    let final_text = if original_chars > MAX_TEMPLATE_CHARS {
        truncated = true;
        cleaned.chars().take(MAX_TEMPLATE_CHARS).collect::<String>()
    } else {
        cleaned
    };

    GddTemplateSourceResult {
        source_kind: source_kind.to_string(),
        source_label,
        source_ref,
        text: final_text,
        original_chars,
        truncated,
    }
}

fn file_name_for_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "template".to_string())
}

fn load_plain_text_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("Failed to read text file: {}", error))
}

fn load_docx_text(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| format!("Failed to open DOCX file: {}", error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("Failed to read DOCX archive: {}", error))?;
    let mut xml = String::new();
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|error| format!("DOCX content missing word/document.xml: {}", error))?;
    entry
        .read_to_string(&mut xml)
        .map_err(|error| format!("Failed to read DOCX document.xml: {}", error))?;

    let xml = xml
        .replace("</w:p>", "\n")
        .replace("</w:tr>", "\n")
        .replace("<w:tab/>", "\t")
        .replace("<w:br/>", "\n")
        .replace("<w:br />", "\n");
    Ok(xml)
}

fn load_pdf_text(path: &Path) -> Result<String, String> {
    let document = lopdf::Document::load(path).map_err(|error| format!("Failed to open PDF file: {}", error))?;
    let mut pages = document.get_pages().keys().copied().collect::<Vec<_>>();
    pages.sort_unstable();

    if pages.is_empty() {
        return Err("PDF contains no pages.".to_string());
    }

    let mut text = String::new();
    for page in pages {
        match document.extract_text(&[page]) {
            Ok(chunk) => {
                if !chunk.trim().is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&chunk);
                }
            }
            Err(error) => {
                warn!("Failed to extract text for PDF page {}: {}", page, error);
            }
        }
    }

    if text.trim().is_empty() {
        return Err("Could not extract readable text from PDF.".to_string());
    }
    Ok(text)
}

pub fn load_template_from_file(file_path: &str) -> Result<GddTemplateSourceResult, String> {
    let path = PathBuf::from(file_path.trim());
    if path.as_os_str().is_empty() {
        return Err("Template file path is empty.".to_string());
    }
    if !path.exists() {
        return Err("Template file does not exist.".to_string());
    }
    if !path.is_file() {
        return Err("Template path must point to a file.".to_string());
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();

    let text = match extension.as_str() {
        "txt" | "md" | "markdown" | "json" => load_plain_text_file(&path)?,
        "docx" => load_docx_text(&path)?,
        "pdf" => load_pdf_text(&path)?,
        other => {
            return Err(format!(
                "Unsupported template file type '{}'. Use .pdf, .docx, .md or .txt.",
                other
            ))
        }
    };

    let label = file_name_for_label(&path);
    Ok(finalize_result(
        "file",
        label,
        path.to_string_lossy().to_string(),
        text,
    ))
}

pub fn from_confluence_page(
    source_ref: String,
    page_title: String,
    text: String,
) -> GddTemplateSourceResult {
    finalize_result("confluence", page_title, source_ref, text)
}
