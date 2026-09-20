//! Semantic page access for the computer-control lane.
//!
//! `page_elements` returns the interactive elements of the current page as a
//! compact table so a model can act on "the login button" instead of guessing
//! coordinates. `browser_task` resolves such a description itself: TypeSafe Jev
//! picks the element when configured, a substring match is the local fallback,
//! and destructive-looking targets require `confirm: true` before we actuate.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::cdp::CdpClient;

/// One interactive element, positioned at its click center.
#[derive(Debug, Deserialize)]
pub struct PageElement {
    #[serde(rename = "i")]
    pub index: usize,
    pub tag: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub href: String,
    pub x: i64,
    pub y: i64,
    #[serde(default, rename = "inVp")]
    pub in_viewport: bool,
}

/// Elements worth clicking or typing into, each with a readable label and the
/// center point a pointer should use. Capped so the table stays compact.
const COLLECT_JS: &str = r#"(() => {
  const els = [...document.querySelectorAll(
    'a,button,input,select,textarea,[role="button"],[role="link"],[onclick],[tabindex]'
  )];
  const out = [];
  for (const el of els) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    const s = getComputedStyle(el);
    if (s.visibility === 'hidden' || s.display === 'none') continue;
    const labelledBy = (el.getAttribute('aria-labelledby') || '').split(/\s+/)
      .map(id => document.getElementById(id)?.textContent || '').join(' ').trim();
    const labels = [...(el.labels || [])].map(label => label.textContent || '').join(' ').trim();
    const text = (el.getAttribute('aria-label') || labelledBy || labels || el.innerText ||
      ((el.type === 'checkbox' || el.type === 'radio' || el.type === 'password') ? '' : el.value) ||
      el.getAttribute('title') || el.getAttribute('placeholder') || '')
      .trim().replace(/\s+/g, ' ').slice(0, 80);
    out.push({
      i: out.length, tag: el.tagName.toLowerCase(),
      kind: el.getAttribute('type') || el.getAttribute('role') || '',
      text, href: el.tagName === "A" ? el.href : "",
      x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2),
      inVp: r.top >= 0 && r.left >= 0 && r.bottom <= innerHeight && r.right <= innerWidth,
    });
    if (out.length >= 120) break;
  }
  return out;
})()"#;

/// Jev's total budget for element resolution; browser tasks should not stall.
const JEV_BUDGET: Duration = Duration::from_millis(2000);
const JEV_INSTRUCTIONS: &str =
    "Pick the page element the instruction refers to, or 'none' when none match.";

/// Reads the interactive elements of the page attached to `client`.
pub async fn get_page_elements(client: &CdpClient) -> Result<Vec<PageElement>, String> {
    let result = client
        .call(
            "Runtime.evaluate",
            json!({ "expression": COLLECT_JS, "returnByValue": true }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let value = result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|e| format!("element index unreadable: {e}"))
}

/// The `page_elements` output: one line per element with everything a model
/// needs to refer back to it or aim a coordinate at it.
pub fn format_table(elements: &[PageElement]) -> String {
    let mut lines = vec!["#i | tag | kind | text | x,y | viewport | href".to_string()];
    for el in elements {
        lines.push(format!(
            "{:>3} | {} | {} | {} | {},{} | {} | {}",
            el.index,
            el.tag,
            el.kind,
            el.text,
            el.x,
            el.y,
            if el.in_viewport { "yes" } else { "no" },
            el.href,
        ));
    }
    lines.join("\n")
}

/// Resolves a natural-language `target` to one element. Jev chooses between
/// the candidates when a key is configured; otherwise a case-insensitive
/// substring match is the local fallback. `None` means "ask the model to pick
/// from the table".
pub async fn resolve_target<'a>(
    target: &str,
    elements: &'a [PageElement],
) -> Option<&'a PageElement> {
    if let Some(jev) = crate::typesafe::JevClient::shared() {
        let options: BTreeMap<String, String> = elements
            .iter()
            .take(120)
            .map(|el| (el.index.to_string(), describe(el)))
            .chain(std::iter::once((
                "none".to_string(),
                "No listed element matches".to_string(),
            )))
            .collect();
        let state = json!({ "instruction": target });
        let questions = BTreeMap::from([(
            "target".to_string(),
            crate::typesafe::Question {
                instructions: JEV_INSTRUCTIONS.to_string(),
                criteria: options,
            },
        )]);
        let answers = jev
            .decide_all(&state, &questions, Instant::now() + JEV_BUDGET)
            .await?;
        let answer = answers.get("target")?;
        // An uncertain remote judgment must not silently become a guessed click.
        if answer.confidence < 0.5 || answer.choice == "none" {
            return None;
        }
        return answer
            .choice
            .parse::<usize>()
            .ok()
            .and_then(|index| elements.iter().find(|el| el.index == index));
    }
    local_match(target, elements)
}

/// Word-overlap fallback when Jev is off: the element whose label shares the
/// most target words. Matching is whitespace-insensitive so "login" hits
/// "Log in". One long distinctive word suffices; short common words need two
/// hits or the suggestion is more noise than signal.
fn local_match<'a>(target: &str, elements: &'a [PageElement]) -> Option<&'a PageElement> {
    let words: Vec<String> = target
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect();
    elements
        .iter()
        .map(|el| {
            let label = describe(el).to_lowercase();
            let nospace: String = label.chars().filter(|c| !c.is_whitespace()).collect();
            let mut hits = 0usize;
            let mut longest = 0usize;
            for word in &words {
                if label.contains(word.as_str()) || nospace.contains(word.as_str()) {
                    hits += 1;
                    longest = longest.max(word.len());
                }
            }
            (el, hits, longest)
        })
        .filter(|(_, hits, longest)| *hits >= 2 || (*hits >= 1 && *longest >= 6))
        .max_by_key(|(_, hits, _)| *hits)
        .map(|(el, _, _)| el)
}

fn describe(el: &PageElement) -> String {
    let mut parts = vec![el.tag.clone()];
    if !el.kind.is_empty() {
        parts.push(el.kind.clone());
    }
    if !el.text.is_empty() {
        parts.push(format!("\"{}\"", el.text));
    }
    parts.join(" ")
}

/// Whether actuating this element could have real-world effects the model
/// should confirm first: payments, deletion, submission, authentication.
/// `text` is the payload for a `type` action when there is one.
pub fn is_risky(el: &PageElement, action: &str, text: Option<&str>) -> bool {
    const RISKY_WORDS: &[&str] = &[
        "buy",
        "pay",
        "purchase",
        "order",
        "checkout",
        "delete",
        "remove",
        "cancel",
        "unsubscribe",
        "send",
        "submit",
        "post",
        "publish",
        "confirm",
        "sign in",
        "signin",
        "log in",
        "login",
        "password",
        "card",
        "transfer",
    ];
    let haystack = format!(
        "{} {} {} {}",
        el.tag,
        el.kind,
        el.text,
        text.unwrap_or_default()
    )
    .to_lowercase();
    let mut risky = RISKY_WORDS.iter().any(|w| haystack.contains(w));
    if matches!(action, "type") && matches!(el.kind.as_str(), "password" | "email" | "tel") {
        risky = true;
    }
    if el.kind == "submit" {
        risky = true;
    }
    risky
}

/// Scrolls an element into the viewport and returns its fresh center; the
/// position collected earlier goes stale the moment the page moves.
pub async fn scroll_into_view(client: &CdpClient, index: usize) -> Result<(i64, i64), String> {
    let js = format!(
        r#"(() => {{
          const els = [...document.querySelectorAll(
            'a,button,input,select,textarea,[role="button"],[role="link"],[onclick],[tabindex]'
          )].filter(el => {{
            const r = el.getBoundingClientRect();
            if (r.width < 2 || r.height < 2) return false;
            const s = getComputedStyle(el);
            return s.visibility !== 'hidden' && s.display !== 'none';
          }});
          const el = els[{index}];
          if (!el) return null;
          el.scrollIntoView({{block: 'center', inline: 'center'}});
          const r = el.getBoundingClientRect();
          return {{x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2)}};
        }})()"#
    );
    let result = client
        .call(
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let value = result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(Value::Null);
    let x = value
        .get("x")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("element {index} no longer on the page"))?;
    let y = value
        .get("y")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("element {index} no longer on the page"))?;
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(index: usize, tag: &str, kind: &str, text: &str) -> PageElement {
        PageElement {
            index,
            tag: tag.to_string(),
            kind: kind.to_string(),
            text: text.to_string(),
            href: String::new(),
            x: 0,
            y: 0,
            in_viewport: true,
        }
    }

    #[test]
    fn table_lists_every_element() {
        let elements = vec![el(0, "a", "", "Home"), el(7, "input", "text", "Search")];
        let table = format_table(&elements);
        assert!(table.contains("a |  | Home"));
        assert!(table.contains("7 | input | text | Search"));
    }

    #[test]
    fn local_match_needs_word_signal() {
        let elements = vec![
            el(0, "a", "", "Learn more"),
            el(1, "a", "", "Pricing plans"),
            el(2, "button", "submit", "Log in with SSO"),
        ];
        // "login" matches "log in" through whitespace-insensitive matching
        assert_eq!(
            local_match("log in with sso", &elements).map(|e| e.index),
            Some(2)
        );
        // one long distinctive word is enough
        assert_eq!(local_match("pricing", &elements).map(|e| e.index), Some(1));
        // nothing relevant resolves to nothing
        assert!(local_match("delete my account", &elements).is_none());
        assert!(local_match("totally unrelated", &elements).is_none());
    }

    #[test]
    fn risky_flags_destructive_and_auth_targets() {
        assert!(is_risky(
            &el(0, "button", "submit", "Complete purchase"),
            "click",
            None
        ));
        assert!(is_risky(
            &el(1, "input", "password", ""),
            "type",
            Some("hunter2")
        ));
        assert!(is_risky(&el(2, "a", "", "Delete account"), "click", None));
        assert!(!is_risky(&el(3, "a", "", "Read more"), "click", None));
        assert!(!is_risky(
            &el(4, "input", "text", "Search"),
            "type",
            Some("rust")
        ));
    }
}
