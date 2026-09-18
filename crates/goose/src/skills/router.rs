//! Ranks installed skills against the user's message without a model call.
//! The per-turn suggestion and `load_skill(query:)` share this BM25 ranker;
//! the TypeSafe Jev router falls back to it when the request or the key is
//! unavailable.

use goose_sdk_types::custom_requests::SourceEntry;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Frontmatter `disable-model-invocation: true` keeps a skill out of the
/// listing and out of routing; it stays loadable through `/skill` or an
/// explicit `load_skill(name:)` call.
pub(crate) fn model_invocable(skill: &SourceEntry) -> bool {
    skill.properties.get("disable-model-invocation") != Some(&Value::Bool(true))
}

/// Optional frontmatter `when_to_use` text, carried through `properties`.
fn when_to_use(skill: &SourceEntry) -> &str {
    skill
        .properties
        .get("when_to_use")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

const K1: f32 = 1.2;
const B: f32 = 0.75;
/// Below this a suggestion is more noise than signal; a bare "hi" ranks nothing.
const MIN_SCORE: f32 = 0.5;
/// A match must reach at least this fraction of the best score to be kept.
const MIN_RELATIVE_SCORE: f32 = 0.5;
/// `load_skill(query:)` returns at most this many matches.
pub(crate) const QUERY_SUGGESTIONS: usize = 8;

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn document_terms(skill: &SourceEntry) -> Vec<String> {
    tokenize(&format!(
        "{} {} {}",
        skill.name.replace(['-', '_'], " "),
        skill.description,
        when_to_use(skill)
    ))
}

pub(crate) struct RankedSkill<'a> {
    pub skill: &'a SourceEntry,
    pub score: f32,
}

/// BM25 over name + description + when_to_use. Returns the best `max` skills
/// above `MIN_SCORE`, highest score first.
pub(crate) fn rank_skills<'a>(
    query: &str,
    skills: &[&'a SourceEntry],
    max: usize,
) -> Vec<RankedSkill<'a>> {
    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return Vec::new();
    }

    let skills: Vec<&SourceEntry> = skills
        .iter()
        .copied()
        .filter(|skill| model_invocable(skill))
        .collect();
    let docs: Vec<Vec<String>> = skills.iter().map(|s| document_terms(s)).collect();
    let n_docs = docs.len() as f32;
    let avg_len = docs.iter().map(|d| d.len() as f32).sum::<f32>() / n_docs.max(1.0);

    let mut doc_freq: HashMap<&str, usize> = HashMap::new();
    for doc in &docs {
        for term in doc.iter().collect::<HashSet<_>>() {
            *doc_freq.entry(term.as_str()).or_default() += 1;
        }
    }

    let mut scored: Vec<RankedSkill> = Vec::new();
    for (skill, doc) in skills.iter().zip(&docs) {
        let doc_len = doc.len() as f32;
        let mut term_freq: HashMap<&str, usize> = HashMap::new();
        for term in doc {
            *term_freq.entry(term.as_str()).or_default() += 1;
        }
        let mut score = 0.0f32;
        for term in &query_terms {
            let term = term.as_str();
            let Some(&n) = doc_freq.get(term) else {
                continue;
            };
            let f = *term_freq.get(term).unwrap_or(&0) as f32;
            if f == 0.0 {
                continue;
            }
            let idf = (1.0 + (n_docs - n as f32 + 0.5) / (n as f32 + 0.5)).ln();
            score += idf * (f * (K1 + 1.0)) / (f + K1 * (1.0 - B + B * doc_len / avg_len.max(1.0)));
        }
        if score >= MIN_SCORE {
            scored.push(RankedSkill { skill, score });
        }
    }
    scored.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.skill.name.cmp(&b.skill.name))
    });
    if let Some(best) = scored.first() {
        let floor = best.score * MIN_RELATIVE_SCORE;
        scored.retain(|hit| hit.score >= floor);
    }
    scored.truncate(max);
    scored
}

/// How many BM25 candidates Jev is asked to choose between (plus "none").
const JEV_CANDIDATES: usize = 5;
/// Total budget for the Jev rerank; a suggestion must never stall a turn.
const JEV_BUDGET: Duration = Duration::from_millis(1500);
const JEV_INSTRUCTIONS: &str =
    "Pick the installed skill that best matches the user's request, or 'none'.";

/// The best skill to suggest for `query`: BM25 ranking, reranked by TypeSafe
/// Jev over the top candidates when a key is configured. Returns
/// `"name — description"`; `None` when nothing is worth suggesting.
pub(crate) async fn suggest(query: &str, skills: &[&SourceEntry]) -> Option<String> {
    let ranked = rank_skills(query, skills, JEV_CANDIDATES);
    let best = ranked.first()?;
    if let Some(jev) = crate::typesafe::JevClient::shared() {
        let options: BTreeMap<String, String> = ranked
            .iter()
            .map(|hit| {
                (
                    hit.skill.name.clone(),
                    hit.skill.description.chars().take(200).collect(),
                )
            })
            .chain(std::iter::once((
                "none".to_string(),
                "No installed skill is relevant".to_string(),
            )))
            .collect();
        let state = serde_json::json!({ "request": query });
        let deadline = Instant::now() + JEV_BUDGET;
        if let Some(choice) = jev
            .decision(JEV_INSTRUCTIONS, &state, &options, deadline)
            .await
        {
            if choice == "none" {
                return None;
            }
            if let Some(hit) = ranked.iter().find(|hit| hit.skill.name == choice) {
                return Some(format_skill(hit.skill));
            }
            // Jev named something outside the candidate set: keep the BM25 best.
        }
    }
    Some(format_skill(best.skill))
}

fn format_skill(skill: &SourceEntry) -> String {
    let description = skill.description.chars().take(120).collect::<String>();
    format!("{} — {}", skill.name, description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_sdk_types::custom_requests::SourceType;

    fn skill(name: &str, description: &str) -> SourceEntry {
        SourceEntry {
            source_type: SourceType::Skill,
            name: name.to_string(),
            description: description.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn ranks_matching_skill_and_ignores_greetings() {
        let pptx = skill(
            "pptx",
            "Create PowerPoint presentations and slide decks with pptxgenjs",
        );
        let pdf = skill("pdf", "Generate and extract PDF documents");
        let skills = [&pptx, &pdf];

        let ranked = rank_skills("make a pptx deck about quarterly results", &skills, 8);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].skill.name, "pptx");

        assert!(rank_skills("hi", &skills, 8).is_empty());
        assert!(rank_skills("", &skills, 8).is_empty());
    }

    #[test]
    fn honors_disable_model_invocation() {
        let mut gated = skill("internal", "internal only");
        gated
            .properties
            .insert("disable-model-invocation".to_string(), Value::Bool(true));
        assert!(!model_invocable(&gated));
        assert!(model_invocable(&skill("open", "available")));
    }

    #[test]
    fn when_to_use_contributes_to_ranking() {
        let mut alert = skill("alert-manager", "Set SEO alerts");
        alert.properties.insert(
            "when_to_use".to_string(),
            Value::String("ranking drops and traffic monitoring".to_string()),
        );
        let other = skill("unrelated", "bake bread loaves");
        let skills = [&alert, &other];

        let ranked = rank_skills("notify me when ranking drops", &skills, 8);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].skill.name, "alert-manager");
    }
}
