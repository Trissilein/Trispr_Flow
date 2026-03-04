use crate::gdd::GddDraft;

pub fn render_markdown(draft: &GddDraft) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", draft.title));
    out.push_str(&format!("_Preset_: `{}`  \n", draft.preset_id));
    out.push_str(&format!("_Generated_: {}\n\n", draft.generated_at_iso));
    out.push_str(&format!("{}\n\n", draft.summary));

    for section in &draft.sections {
        out.push_str(&format!("## {}\n\n", section.title));
        out.push_str(&section.content);
        out.push_str("\n\n");
    }

    out
}

pub fn render_confluence_storage(draft: &GddDraft) -> String {
    let mut out = String::new();
    out.push_str(&format!("<h1>{}</h1>", html_escape(&draft.title)));
    out.push_str(&format!(
        "<p><strong>Preset:</strong> {}<br/><strong>Generated:</strong> {}</p>",
        html_escape(&draft.preset_id),
        html_escape(&draft.generated_at_iso)
    ));
    out.push_str(&format!("<p>{}</p>", html_escape(&draft.summary)));

    for section in &draft.sections {
        out.push_str(&format!("<h2>{}</h2>", html_escape(&section.title)));
        if section.content.lines().any(|line| line.trim_start().starts_with("- ")) {
            out.push_str("<ul>");
            for line in section.content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("- ") {
                    out.push_str(&format!("<li>{}</li>", html_escape(rest)));
                }
            }
            out.push_str("</ul>");
        } else {
            out.push_str(&format!("<p>{}</p>", html_escape(&section.content)));
        }
    }

    out
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
