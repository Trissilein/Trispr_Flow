#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedFact {
    pub section_id: String,
    pub statement: String,
    pub evidence: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractionOutput {
    pub chunk_count: usize,
    pub facts: Vec<ExtractedFact>,
}

fn split_chunks(text: &str, max_chunk_chars: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if current.len() + line.len() + 1 > max_chunk_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

fn classify_section(statement: &str) -> &'static str {
    let lower = statement.to_lowercase();
    if lower.contains("player") || lower.contains("experience") || lower.contains("persona") {
        return "player_experience";
    }
    if lower.contains("loop") || lower.contains("repeat") || lower.contains("cycle") {
        return "core_loop";
    }
    if lower.contains("mechanic") || lower.contains("combat") || lower.contains("control") {
        return "mechanics";
    }
    if lower.contains("scope") || lower.contains("level") || lower.contains("mission") {
        return "content_scope";
    }
    if lower.contains("economy")
        || lower.contains("progression")
        || lower.contains("currency")
        || lower.contains("reward")
    {
        return "economy_progression";
    }
    if lower.contains("engine")
        || lower.contains("platform")
        || lower.contains("performance")
        || lower.contains("memory")
    {
        return "technical_constraints";
    }
    if lower.contains("roadmap") || lower.contains("milestone") || lower.contains("sprint") {
        return "production_plan";
    }
    if lower.contains("risk") || lower.contains("open") || lower.contains("question") {
        return "open_questions";
    }
    "vision"
}

pub fn extract_facts(transcript: &str, max_chunk_chars: usize) -> ExtractionOutput {
    let chunks = split_chunks(transcript, max_chunk_chars.max(400));
    let mut facts = Vec::new();

    for chunk in &chunks {
        for sentence in chunk.split(['.', '!', '?', '\n']) {
            let statement = sentence.trim();
            if statement.len() < 16 {
                continue;
            }
            let section_id = classify_section(statement).to_string();
            facts.push(ExtractedFact {
                section_id,
                statement: statement.to_string(),
                evidence: statement.to_string(),
            });
        }
    }

    ExtractionOutput {
        chunk_count: chunks.len(),
        facts,
    }
}
