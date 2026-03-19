use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPresetSection {
    pub id: String,
    pub title: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_clone: bool,
    pub base_preset_id: Option<String>,
    pub detail_level: String,
    pub tone: String,
    pub keywords: Vec<String>,
    pub sections: Vec<GddPresetSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPresetClone {
    pub id: String,
    pub name: String,
    pub detail_level: String,
    pub tone: String,
    pub keywords: Vec<String>,
    pub section_order: Vec<String>,
    pub required_sections: Vec<String>,
}

fn universal_sections() -> Vec<GddPresetSection> {
    vec![
        GddPresetSection {
            id: "vision".to_string(),
            title: "Vision".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "player_experience".to_string(),
            title: "Player Experience".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "core_loop".to_string(),
            title: "Core Loop".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "mechanics".to_string(),
            title: "Mechanics".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "content_scope".to_string(),
            title: "Content Scope".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "economy_progression".to_string(),
            title: "Economy & Progression".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "technical_constraints".to_string(),
            title: "Technical Constraints".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "production_plan".to_string(),
            title: "Production Plan".to_string(),
            required: true,
        },
        GddPresetSection {
            id: "open_questions".to_string(),
            title: "Open Questions".to_string(),
            required: true,
        },
    ]
}

pub fn universal_preset() -> GddPreset {
    GddPreset {
        id: "universal_strict".to_string(),
        name: "Universal Strict GDD".to_string(),
        description: "Strict baseline preset for complete game design documents.".to_string(),
        is_clone: false,
        base_preset_id: None,
        detail_level: "normal".to_string(),
        tone: "product_spec".to_string(),
        keywords: vec![
            "game design".to_string(),
            "mechanics".to_string(),
            "level".to_string(),
            "combat".to_string(),
            "ui".to_string(),
            "player".to_string(),
            "quest".to_string(),
            "progression".to_string(),
            "economy".to_string(),
            "balancing".to_string(),
            "prototype".to_string(),
            "gdd".to_string(),
            "gameplay".to_string(),
            "narrative".to_string(),
        ],
        sections: universal_sections(),
    }
}

fn preset_from_clone(clone: &GddPresetClone) -> GddPreset {
    let base = universal_preset();
    let mut section_map = base
        .sections
        .iter()
        .map(|section| (section.id.clone(), section.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let mut sections = Vec::new();
    for section_id in &clone.section_order {
        if let Some(section) = section_map.remove(section_id) {
            sections.push(GddPresetSection {
                required: clone.required_sections.contains(section_id),
                ..section
            });
        }
    }
    for (_section_id, section) in section_map {
        sections.push(GddPresetSection {
            required: clone.required_sections.contains(&section.id),
            ..section
        });
    }

    GddPreset {
        id: clone.id.clone(),
        name: clone.name.clone(),
        description: format!("Clone preset derived from {}", base.name),
        is_clone: true,
        base_preset_id: Some(base.id),
        detail_level: clone.detail_level.clone(),
        tone: clone.tone.clone(),
        keywords: clone.keywords.clone(),
        sections,
    }
}

pub fn list_presets(clones: &[GddPresetClone]) -> Vec<GddPreset> {
    let mut presets = vec![universal_preset()];
    let clone_presets = clones.iter().map(preset_from_clone);
    presets.extend(clone_presets);
    presets
}

pub fn preset_by_id(preset_id: &str, presets: &[GddPreset]) -> Option<GddPreset> {
    presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
}
