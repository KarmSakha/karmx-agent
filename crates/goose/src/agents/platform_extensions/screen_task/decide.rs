//! The Jev side: criteria for each question and the one multi-Choice request.
//! Ported from awlevin/typesafe-computer-use — the criteria wording is what
//! keeps a small classifier honest.

use super::items::{Item, Screen};
use crate::typesafe::{Answer, Question};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

pub const STOP_KINDS: [&str; 2] = ["done", "none"];

/// What Jev decided for one step.
pub struct Decision {
    pub kind: String,
    pub item_index: Option<usize>,
    pub site: Option<String>,
    pub offscreen_key: Option<String>,
    pub confidence: f64,
}

impl Decision {
    pub fn stops(&self) -> bool {
        STOP_KINDS.contains(&self.kind.as_str())
    }
}

/// Actions Jev can pick that don't need an item — kept mutually exclusive so
/// confidence stays meaningful (overlapping options always read as doubt).
fn kind_criteria(has_offscreen: bool, has_field: bool) -> BTreeMap<String, String> {
    let mut actions = BTreeMap::from([
        (
            "click_item".to_string(),
            "Click one of the on-screen items (chosen in the item question).".to_string(),
        ),
        (
            "open_site".to_string(),
            "Navigate the browser to a website. The only way to go to a site: never click \
             the address bar, a URL, or a search box to get there."
                .to_string(),
        ),
        (
            "switch_to_browser".to_string(),
            "Bring the web browser to the front to continue with the page already open \
             there. Not for reaching a specific site: open_site does that on its own."
                .to_string(),
        ),
        (
            "press_enter".to_string(),
            "Press Return to submit the focused form or field.".to_string(),
        ),
        (
            "press_escape".to_string(),
            "Press Escape to dismiss a dialog, menu, or popup.".to_string(),
        ),
        (
            "scroll_down".to_string(),
            "Scroll down to reveal more of the page.".to_string(),
        ),
        ("scroll_up".to_string(), "Scroll up.".to_string()),
        (
            "wait".to_string(),
            "Nothing to do yet; the screen is still loading or changing.".to_string(),
        ),
        (
            "done".to_string(),
            "The goal is already achieved.".to_string(),
        ),
        (
            "none".to_string(),
            "Nothing on screen or in this list helps with the goal.".to_string(),
        ),
    ]);
    if has_offscreen {
        actions.insert(
            "press_offscreen".to_string(),
            "Activate a labelled control the app exposes but that is not visible on screen \
             (chosen in the offscreen question). Use when the needed control exists but is \
             scrolled out of view or not yet shown."
                .to_string(),
        );
    }
    if has_field {
        actions.insert(
            "type_text".to_string(),
            "Type the goal-relevant text into the focused text field.".to_string(),
        );
    }
    actions
}

fn item_criteria(items: &[Item]) -> BTreeMap<String, String> {
    items
        .iter()
        .map(|it| match &it.role {
            Some(role) => (
                it.index.to_string(),
                format!(
                    "{} '{}' — a real control from the app's accessibility tree",
                    role.replace("AX", "").to_lowercase(),
                    it.text
                ),
            ),
            None => (
                it.index.to_string(),
                format!("'{}' — text read from the screen", it.text),
            ),
        })
        .collect()
}

fn site_criteria() -> BTreeMap<String, String> {
    [
        ("github", "https://github.com/"),
        ("gmail", "https://mail.google.com/"),
        ("discord", "https://discord.com/app"),
        ("notion", "https://www.notion.so/"),
        ("slack", "https://app.slack.com/"),
        ("linear", "https://linear.app/"),
        ("typesafe_console", "https://console.typesafe.ai/"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

fn offscreen_criteria(screen: &Screen) -> BTreeMap<String, String> {
    screen
        .offscreen
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("off{i}"), format!("{} '{}'", p.role, p.label)))
        .collect()
}

/// One Jev request: `kind`, `item`, `site`, and `offscreen` when it applies.
pub async fn decide(state: &Value, items: &[Item], screen: &Screen) -> Option<Decision> {
    let jev = crate::typesafe::JevClient::shared()?;
    let mut questions = BTreeMap::from([
        (
            "kind".to_string(),
            Question {
                instructions: "You are driving this computer one action at a time. Which kind \
                     of action makes the most progress toward the goal right now? Do not \
                     repeat an action that was just taken unless the screen changed."
                    .to_string(),
                criteria: kind_criteria(!screen.offscreen.is_empty(), screen.field.is_some()),
            },
        ),
        (
            "site".to_string(),
            Question {
                instructions: "If a website must be opened to progress the goal, which one?"
                    .to_string(),
                criteria: site_criteria(),
            },
        ),
    ]);
    if !items.is_empty() {
        questions.insert(
            "item".to_string(),
            Question {
                instructions: "If clicking an on-screen item is the right move, which item? \
                     Items marked with a role are real controls from the accessibility tree; \
                     plain items are text read from the screen."
                    .to_string(),
                criteria: item_criteria(items),
            },
        );
    }
    if !screen.offscreen.is_empty() {
        questions.insert(
            "offscreen".to_string(),
            Question {
                instructions: "If activating a control that is not on screen is the right \
                     move, which control? These are real controls reachable without the \
                     mouse, but nothing on the capture points at them."
                    .to_string(),
                criteria: offscreen_criteria(screen),
            },
        );
    }

    let deadline = Instant::now() + std::time::Duration::from_secs(8);
    let answers = jev.decide_all(state, &questions, deadline).await?;
    let kind = answers.get("kind")?;
    let item = answers.get("item");
    let offscreen = answers.get("offscreen");

    let needs_item = kind.choice == "click_item" && item.is_some();
    let needs_offscreen = kind.choice == "press_offscreen" && offscreen.is_some();
    let confidence = if needs_item {
        kind.confidence.min(item.unwrap().confidence)
    } else if needs_offscreen {
        kind.confidence.min(offscreen.unwrap().confidence)
    } else {
        kind.confidence
    };

    Some(Decision {
        kind: kind.choice.clone(),
        item_index: if needs_item {
            item.and_then(|a: &Answer| a.choice.parse::<usize>().ok())
        } else {
            None
        },
        site: answers.get("site").map(|a| a.choice.clone()),
        offscreen_key: if needs_offscreen {
            offscreen.map(|a| a.choice.clone())
        } else {
            None
        },
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_options_stay_mutually_exclusive() {
        let kinds = kind_criteria(true, true);
        assert!(kinds.contains_key("press_offscreen"));
        assert!(kinds.contains_key("type_text"));
        assert!(kinds.contains_key("click_item"));
        assert!(kinds.contains_key("done"));
        assert!(kinds.contains_key("none"));
    }
}
