use super::CompletionCache;
use super::input::{InputResult, ModelCommandOptions};
use console::{Key, Term, measure_text_width, style};
use goose::config::Config;
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;
use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Write};
use std::sync::{Arc, RwLock};

const MAX_VISIBLE_ROWS: usize = 12;
const EFFORTS: [ThinkingEffort; 5] = [
    ThinkingEffort::Off,
    ThinkingEffort::Low,
    ThinkingEffort::Medium,
    ThinkingEffort::High,
    ThinkingEffort::Max,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Lead,
    Sidekick,
}

#[derive(Debug, Clone)]
struct ModelRow {
    provider: String,
    model: String,
    reasoning: bool,
    effort: ThinkingEffort,
}

/// Interactive `/model` overlay: type to search, ↑↓ to move, → to cycle
/// thinking effort, tab to edit the sidekick, enter/← to confirm, esc to cancel.
pub fn run(cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    let mut rows = rows_from_cache(cache);
    if rows.is_empty() {
        return None;
    }
    run_overlay(&mut rows, cache)
}

fn rows_from_cache(cache: &Arc<RwLock<CompletionCache>>) -> Vec<ModelRow> {
    let cache = cache.read().unwrap();
    let current_provider = if cache.current_session_provider.is_empty() {
        Config::global().get_goose_provider().unwrap_or_default()
    } else {
        cache.current_session_provider.clone()
    };
    let current_model = cache.current_session_model.clone();
    let default_effort = Config::global()
        .get_goose_thinking_effort()
        .unwrap_or(ThinkingEffort::Medium);

    collect_model_rows(
        &current_provider,
        &current_model,
        goose::config::providers::configured_models(Config::global()),
        &cache.provider_names,
        &cache.provider_models,
        default_effort,
    )
}

fn collect_model_rows(
    current_provider: &str,
    current_model: &str,
    configured: impl IntoIterator<Item = (String, String)>,
    provider_names: &[String],
    provider_models: &HashMap<String, Vec<String>>,
    default_effort: ThinkingEffort,
) -> Vec<ModelRow> {
    let mut rows: Vec<ModelRow> = Vec::new();
    let mut seen = HashSet::new();

    let mut push = |provider: String, model: String| {
        if provider.is_empty() || model.is_empty() {
            return;
        }
        if !seen.insert((provider.clone(), model.clone())) {
            return;
        }
        let reasoning = ModelConfig::new(&model).is_reasoning_model();
        rows.push(ModelRow {
            provider,
            model,
            reasoning,
            effort: default_effort,
        });
    };

    if !current_provider.is_empty() && !current_model.is_empty() {
        push(current_provider.to_string(), current_model.to_string());
    }

    for (provider, model) in configured {
        push(provider, model);
    }

    let mut providers: Vec<String> = provider_names.to_vec();
    providers.sort_by_key(|p| usize::from(p != current_provider));
    for provider in providers {
        if let Some(models) = provider_models.get(&provider) {
            for model in models {
                push(provider.clone(), model.clone());
            }
        }
    }

    rows
}

fn run_overlay(rows: &mut [ModelRow], cache: &Arc<RwLock<CompletionCache>>) -> Option<InputResult> {
    let (current_provider, current_model) = {
        let cache = cache.read().unwrap();
        (
            cache.current_session_provider.clone(),
            cache.current_session_model.clone(),
        )
    };
    let mut sidekick_provider = goose::agents::local_fusion::sidekick_provider();
    let mut sidekick_model = goose::agents::local_fusion::sidekick_model_uid();

    let mut query = String::new();
    let mut focus = Focus::Lead;
    let mut selected = rows
        .iter()
        .position(|r| r.provider == current_provider && r.model == current_model)
        .unwrap_or(0);

    let mut term = Term::stderr();
    let _ = term.hide_cursor();
    struct CursorGuard;
    impl Drop for CursorGuard {
        fn drop(&mut self) {
            let _ = Term::stderr().show_cursor();
        }
    }
    let _guard = CursorGuard;

    let mut drawn = 0usize;
    loop {
        let list = visible_rows(rows, focus, &query);
        if list.is_empty() {
            selected = 0;
        } else if selected >= list.len() {
            selected = list.len() - 1;
        }

        if drawn > 0 {
            let _ = term.clear_last_lines(drawn);
        }
        drawn = match draw(
            &mut term,
            OverlayView {
                rows,
                list: &list,
                selected,
                focus,
                query: &query,
                current_provider: &current_provider,
                current_model: &current_model,
                sidekick_provider: sidekick_provider.as_deref(),
                sidekick_model: sidekick_model.as_deref(),
            },
        ) {
            Ok(n) => n,
            Err(_) => return None,
        };

        let key = match term.read_key() {
            Ok(k) => k,
            Err(_) => {
                let _ = term.clear_last_lines(drawn);
                return None;
            }
        };

        match key {
            Key::Escape | Key::Char('\u{3}') => {
                let _ = term.clear_last_lines(drawn);
                return None;
            }
            Key::ArrowUp => {
                if !list.is_empty() {
                    selected = selected.saturating_sub(1);
                }
            }
            Key::ArrowDown => {
                if !list.is_empty() && selected + 1 < list.len() {
                    selected += 1;
                }
            }
            Key::ArrowRight => {
                if focus == Focus::Lead {
                    if let Some(ListEntry::Model(idx)) = list.get(selected) {
                        if rows[*idx].reasoning {
                            rows[*idx].effort = cycle_effort(rows[*idx].effort);
                        }
                    }
                }
            }
            Key::Enter | Key::ArrowLeft => {
                if list.is_empty() {
                    continue;
                }
                match confirm(
                    rows,
                    &list,
                    selected,
                    focus,
                    &mut sidekick_provider,
                    &mut sidekick_model,
                ) {
                    Some(result) => {
                        let _ = term.clear_last_lines(drawn);
                        return Some(result);
                    }
                    None if focus == Focus::Sidekick => {
                        focus = Focus::Lead;
                        query.clear();
                        selected = rows
                            .iter()
                            .position(|r| {
                                r.provider == current_provider && r.model == current_model
                            })
                            .unwrap_or(0);
                    }
                    None => {
                        let _ = term.clear_last_lines(drawn);
                        return None;
                    }
                }
            }
            Key::Tab => {
                query.clear();
                focus = match focus {
                    Focus::Lead => Focus::Sidekick,
                    Focus::Sidekick => Focus::Lead,
                };
                selected = 0;
            }
            Key::Backspace => {
                query.pop();
                selected = 0;
            }
            Key::Char(c) if !c.is_control() => {
                query.push(c);
                selected = 0;
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListEntry {
    SameAsLead,
    Model(usize),
}

fn visible_rows(rows: &[ModelRow], focus: Focus, query: &str) -> Vec<ListEntry> {
    let mut list = Vec::new();
    if focus == Focus::Sidekick && query_matches("same as lead", query) {
        list.push(ListEntry::SameAsLead);
    }
    for (idx, row) in rows.iter().enumerate() {
        if row_matches(row, query) {
            list.push(ListEntry::Model(idx));
        }
    }
    list
}

fn row_matches(row: &ModelRow, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_ascii_lowercase();
    row.model.to_ascii_lowercase().contains(&q)
        || row.provider.to_ascii_lowercase().contains(&q)
        || friendly_model_name(&row.model)
            .to_ascii_lowercase()
            .contains(&q)
}

fn query_matches(haystack: &str, query: &str) -> bool {
    query.is_empty()
        || haystack
            .to_ascii_lowercase()
            .contains(&query.to_ascii_lowercase())
}

fn confirm(
    rows: &[ModelRow],
    list: &[ListEntry],
    selected: usize,
    focus: Focus,
    sidekick_provider: &mut Option<String>,
    sidekick_model: &mut Option<String>,
) -> Option<InputResult> {
    let entry = list.get(selected)?;
    match (focus, entry) {
        (Focus::Sidekick, ListEntry::SameAsLead) => {
            persist_sidekick(None, None);
            *sidekick_provider = None;
            *sidekick_model = None;
            None
        }
        (Focus::Sidekick, ListEntry::Model(idx)) => {
            let row = &rows[*idx];
            persist_sidekick(Some(&row.provider), Some(&row.model));
            *sidekick_provider = Some(row.provider.clone());
            *sidekick_model = Some(row.model.clone());
            None
        }
        (Focus::Lead, ListEntry::Model(idx)) => {
            let row = &rows[*idx];
            Some(InputResult::Model(ModelCommandOptions {
                provider: Some(row.provider.clone()),
                model: Some(row.model.clone()),
                thinking: row.reasoning.then_some(row.effort),
            }))
        }
        (Focus::Lead, ListEntry::SameAsLead) => None,
    }
}

fn persist_sidekick(provider: Option<&str>, model: Option<&str>) {
    let config = Config::global();
    match (provider, model) {
        (None, None) => {
            for key in ["KARMX_SIDEKICK_PROVIDER", "KARMX_SIDEKICK_MODEL"] {
                let _ = config.delete(key);
            }
        }
        (Some(p), Some(m)) => {
            let _ = config.set_param("KARMX_SIDEKICK_PROVIDER", p);
            let _ = config.set_param("KARMX_SIDEKICK_MODEL", m);
        }
        _ => {}
    }
}

fn cycle_effort(current: ThinkingEffort) -> ThinkingEffort {
    let idx = EFFORTS.iter().position(|e| *e == current).unwrap_or(2);
    EFFORTS[(idx + 1) % EFFORTS.len()]
}

fn effort_bar(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Off => "    ",
        ThinkingEffort::Low => "▪   ",
        ThinkingEffort::Medium => "▪▪  ",
        ThinkingEffort::High => "▪▪▪ ",
        ThinkingEffort::Max => "▪▪▪▪",
    }
}

fn effort_label(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Off => "Off",
        ThinkingEffort::Low => "Low",
        ThinkingEffort::Medium => "Medium",
        ThinkingEffort::High => "High",
        ThinkingEffort::Max => "Max",
    }
}

pub(super) fn friendly_model_name(id: &str) -> String {
    const ACRONYMS: &[&str] = &["gpt", "glm", "swe", "aws", "api", "o1", "o3", "o4"];
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if ACRONYMS.contains(&lower.as_str()) {
                part.to_ascii_uppercase()
            } else if part
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'x'))
            {
                part.to_string()
            } else {
                let mut chars = part.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct OverlayView<'a> {
    rows: &'a [ModelRow],
    list: &'a [ListEntry],
    selected: usize,
    focus: Focus,
    query: &'a str,
    current_provider: &'a str,
    current_model: &'a str,
    sidekick_provider: Option<&'a str>,
    sidekick_model: Option<&'a str>,
}

fn draw(term: &mut Term, view: OverlayView<'_>) -> std::io::Result<usize> {
    let width = term
        .size_checked()
        .map(|(_, cols)| cols as usize)
        .unwrap_or(80);
    let max_rows = visible_row_budget(term);
    let (window_start, window_end) = window(view.list.len(), view.selected, max_rows);

    let mut lines: Vec<String> = Vec::new();
    lines.push(String::new());
    lines.push(header_line(width, &view));
    lines.push(format!(
        "  {}",
        style("─".repeat(width.saturating_sub(4))).dim()
    ));

    let search = if view.query.is_empty() {
        style("type to search").dim().to_string()
    } else {
        format!("{}{}", style("/ ").dim(), view.query)
    };
    lines.push(format!("  {search}"));

    if view.list.is_empty() {
        lines.push(format!("  {}", style("no matching models").dim()));
    } else {
        for (visible_idx, entry) in view.list[window_start..window_end].iter().enumerate() {
            let abs = window_start + visible_idx;
            lines.push(row_line(
                width,
                *entry,
                view.rows,
                abs == view.selected,
                view.current_provider,
                view.current_model,
            ));
        }
        let below = view.list.len().saturating_sub(window_end);
        if below > 0 {
            lines.push(format!("  {}", style(format!("{below} more below")).dim()));
        }
    }

    lines.push(format!(
        "  {}",
        style("─".repeat(width.saturating_sub(4))).dim()
    ));
    lines.push(format!(
        "  {}",
        style("↑↓ select  → reasoning  ←/enter confirm  tab sidekick  esc cancel").dim()
    ));

    for line in &lines {
        writeln!(term, "{line}")?;
    }
    term.flush()?;
    Ok(lines.len())
}

fn visible_row_budget(term: &Term) -> usize {
    let rows = term
        .size_checked()
        .map(|(rows, _)| rows as usize)
        .unwrap_or(24);
    rows.saturating_sub(10).clamp(4, MAX_VISIBLE_ROWS)
}

fn window(len: usize, selected: usize, max_rows: usize) -> (usize, usize) {
    if len <= max_rows {
        return (0, len);
    }
    let mut start = selected.saturating_sub(max_rows / 2);
    if start + max_rows > len {
        start = len - max_rows;
    }
    (start, start + max_rows)
}

fn header_line(width: usize, view: &OverlayView<'_>) -> String {
    let lead_name = if view.current_model.is_empty() {
        "none".to_string()
    } else {
        friendly_model_name(view.current_model)
    };
    let lead_effort = view
        .rows
        .iter()
        .find(|r| r.provider == view.current_provider && r.model == view.current_model)
        .filter(|r| r.reasoning)
        .map(|r| format!(" {}", effort_label(r.effort)))
        .unwrap_or_default();

    let sidekick_name = match (view.sidekick_provider, view.sidekick_model) {
        (None, None) => "same as lead".to_string(),
        (_, Some(model)) => friendly_model_name(model),
        (Some(provider), None) => provider.to_string(),
    };

    let lead = format!("Load {lead_name}{lead_effort}");
    let sidekick = format!("Sidekick {sidekick_name}");
    let lead_styled = if view.focus == Focus::Lead {
        style(lead).cyan().bold().to_string()
    } else {
        style(lead).dim().to_string()
    };
    let sidekick_styled = if view.focus == Focus::Sidekick {
        style(sidekick).cyan().bold().to_string()
    } else {
        style(sidekick).dim().to_string()
    };

    let gap = 4;
    let used = measure_text_width(&lead_styled) + measure_text_width(&sidekick_styled) + 4 + gap;
    let pad = width.saturating_sub(used);
    format!(
        "  {lead_styled}{}{sidekick_styled}",
        " ".repeat(pad.max(gap))
    )
}

fn row_line(
    width: usize,
    entry: ListEntry,
    rows: &[ModelRow],
    selected: bool,
    current_provider: &str,
    current_model: &str,
) -> String {
    let marker = if selected { "▸" } else { " " };
    let (name, provider, effort, is_current) = match entry {
        ListEntry::SameAsLead => (
            "same as lead".to_string(),
            String::new(),
            None,
            current_model.is_empty(),
        ),
        ListEntry::Model(idx) => {
            let row = &rows[idx];
            let effort = row.reasoning.then_some(row.effort);
            let is_current = row.provider == current_provider && row.model == current_model;
            (
                friendly_model_name(&row.model),
                row.provider.clone(),
                effort,
                is_current,
            )
        }
    };

    let current_dot = if is_current { "● " } else { "  " };
    let effort_text = match effort {
        Some(e) => format!("{} {}", effort_bar(e), effort_label(e)),
        None => String::new(),
    };

    let prefix = format!("  {marker} {current_dot}");
    let suffix = if provider.is_empty() {
        effort_text
    } else if effort_text.is_empty() {
        provider
    } else {
        format!("{provider}  {effort_text}")
    };
    let prefix_width = measure_text_width(&prefix);
    let suffix_width = measure_text_width(&suffix);
    let name_budget = width.saturating_sub(prefix_width + suffix_width + 3).max(8);
    let name = truncate_to(name, name_budget);
    let pad = width
        .saturating_sub(prefix_width + measure_text_width(&name) + suffix_width + 2)
        .max(1);

    let body = format!("{prefix}{name}{}{suffix}", " ".repeat(pad));
    if selected {
        style(body).cyan().to_string()
    } else {
        body
    }
}

fn truncate_to(text: String, budget: usize) -> String {
    if measure_text_width(&text) <= budget {
        return text;
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next = format!("{out}{ch}");
        if measure_text_width(&next) + 1 > budget {
            break;
        }
        out.push(ch);
    }
    format!("{out}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_names_title_case_and_keep_acronyms() {
        assert_eq!(friendly_model_name("claude-sonnet-4"), "Claude Sonnet 4");
        assert_eq!(friendly_model_name("gpt-4o"), "GPT 4o");
        assert_eq!(friendly_model_name("glm-4.5"), "GLM 4.5");
        assert_eq!(friendly_model_name("o3-mini"), "O3 Mini");
    }

    #[test]
    fn cycle_effort_wraps_from_max_to_off() {
        assert_eq!(cycle_effort(ThinkingEffort::High), ThinkingEffort::Max);
        assert_eq!(cycle_effort(ThinkingEffort::Max), ThinkingEffort::Off);
        assert_eq!(cycle_effort(ThinkingEffort::Off), ThinkingEffort::Low);
    }

    #[test]
    fn filter_matches_friendly_name_and_provider() {
        let row = ModelRow {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            reasoning: true,
            effort: ThinkingEffort::Medium,
        };
        assert!(row_matches(&row, "sonnet"));
        assert!(row_matches(&row, "ANTHROPIC"));
        assert!(row_matches(&row, "Claude"));
        assert!(!row_matches(&row, "gpt"));
    }

    #[test]
    fn sidekick_list_starts_with_same_as_lead() {
        let rows = vec![ModelRow {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            reasoning: false,
            effort: ThinkingEffort::Medium,
        }];
        let lead = visible_rows(&rows, Focus::Lead, "");
        assert_eq!(lead, vec![ListEntry::Model(0)]);
        let sidekick = visible_rows(&rows, Focus::Sidekick, "");
        assert_eq!(sidekick, vec![ListEntry::SameAsLead, ListEntry::Model(0)]);
    }

    #[test]
    fn window_keeps_selection_in_view() {
        assert_eq!(window(20, 0, 5), (0, 5));
        assert_eq!(window(20, 19, 5), (15, 20));
        assert_eq!(window(3, 1, 12), (0, 3));
    }

    #[test]
    fn two_registered_models_are_listed_current_first_without_duplicates() {
        let mut provider_models = HashMap::new();
        provider_models.insert(
            "anthropic".to_string(),
            vec!["claude-sonnet-4".to_string(), "claude-haiku-4".to_string()],
        );
        provider_models.insert("openai".to_string(), vec!["gpt-4o".to_string()]);

        let rows = collect_model_rows(
            "anthropic",
            "claude-sonnet-4",
            vec![
                ("anthropic".to_string(), "claude-sonnet-4".to_string()),
                ("openai".to_string(), "gpt-4o".to_string()),
            ],
            &["openai".to_string(), "anthropic".to_string()],
            &provider_models,
            ThinkingEffort::High,
        );

        let pairs: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.provider.as_str(), row.model.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("anthropic", "claude-sonnet-4"),
                ("openai", "gpt-4o"),
                ("anthropic", "claude-haiku-4"),
            ]
        );
        assert_eq!(rows[0].effort, ThinkingEffort::High);
        assert!(
            rows.iter()
                .any(|row| row.provider == "openai" && row.model == "gpt-4o")
        );
        assert_eq!(rows.len(), 3);
    }
}
