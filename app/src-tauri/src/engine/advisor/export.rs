//! Dossier export: the hand-to-another-AI artifact. Dual format — structured
//! header, the cited dossier prose, a verbatim evidence appendix with stable
//! anchors, and the license/attribution block (required by pack licenses).

use super::types::Advisory;

pub fn to_markdown(advisory: &Advisory, conversation_title: &str, problem: Option<&str>) -> String {
    let mut out = String::new();

    out.push_str("---\n");
    out.push_str("generator: Compendium (problem → method advisor)\n");
    out.push_str(&format!("conversation: {conversation_title}\n"));
    out.push_str(&format!("tier: {}\n", advisory.tier));
    if !advisory.recommendations.is_empty() {
        out.push_str("recommendations:\n");
        for r in &advisory.recommendations {
            out.push_str(&format!(
                "  - technique: {}\n    pack: {}\n    stage: {}\n    complexity: {}\n    confidence: {} ({:.2})\n",
                r.slug, r.pack_id, r.stage_id, r.complexity, r.confidence_label, r.confidence
            ));
        }
    }
    if !advisory.failure_modes.is_empty() {
        out.push_str("failure_modes:\n");
        for f in &advisory.failure_modes {
            out.push_str(&format!("  - {} ({})\n", f.id, f.name));
        }
    }
    out.push_str("---\n\n");

    if let Some(p) = problem {
        out.push_str("## Problem statement (verbatim)\n\n");
        out.push_str(p);
        out.push_str("\n\n");
    }
    if !advisory.diagnosis_md.is_empty() {
        out.push_str("## Diagnosis summary\n\n");
        out.push_str(&advisory.diagnosis_md);
        out.push_str("\n\n");
    }

    if !advisory.answer_md.is_empty() {
        out.push_str(&advisory.answer_md);
        out.push_str("\n\n");
    }

    if let Some(gaps) = &advisory.gaps {
        out.push_str("## Coverage gaps\n\n");
        out.push_str(gaps);
        out.push_str("\n\n");
    }

    if !advisory.evidence.is_empty() {
        out.push_str("## Evidence appendix (verbatim excerpts)\n\n");
        out.push_str(
            "Each excerpt carries a stable anchor `pack:chunk:id` for traceability.\n\n",
        );
        for e in &advisory.evidence {
            out.push_str(&format!(
                "### `{}` — {} ({})\n\n",
                e.doc_key,
                e.heading_path,
                e.technique_slug.as_deref().unwrap_or("untyped"),
            ));
            out.push_str(&e.text);
            out.push_str("\n\n");
        }
    }

    if !advisory.attribution_html.is_empty() {
        out.push_str("---\n\n");
        out.push_str("**Attribution & license**\n\n");
        for a in &advisory.attribution_html {
            out.push_str(&format!("- {}\n", strip_tags(a)));
        }
        out.push_str(
            "\nThis dossier contains content modified from the sources above; \
non-commercial use only where the source license requires it.\n",
        );
    }

    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}
