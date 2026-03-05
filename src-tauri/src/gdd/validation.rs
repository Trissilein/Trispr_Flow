use crate::gdd::{GddDraft, ValidateGddDraftResult};

pub fn validate_draft(draft: &GddDraft) -> ValidateGddDraftResult {
    let mut errors = Vec::new();

    if draft.title.trim().is_empty() {
        errors.push("Draft title is empty.".to_string());
    }
    if draft.sections.is_empty() {
        errors.push("Draft has no sections.".to_string());
    }

    for section in &draft.sections {
        if section.id.trim().is_empty() {
            errors.push("A section has an empty id.".to_string());
        }
        if section.title.trim().is_empty() {
            errors.push(format!("Section '{}' has an empty title.", section.id));
        }
        if section.content.trim().is_empty() {
            errors.push(format!("Section '{}' has empty content.", section.id));
        }
    }

    ValidateGddDraftResult {
        valid: errors.is_empty(),
        errors,
    }
}
