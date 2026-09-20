//! TypeSafe Jev client: typed judgments used where code needs semantic
//! understanding (skill routing, target resolution). Jev answers a `choice`
//! question with the selected option and its probability distribution rather
//! than generating text.
//!
//! The client is optional: `JevClient::from_env` returns `None` without
//! `TYPESAFE_API_KEY` (env var or `<config dir>/typesafe.env`), and every call
//! degrades to `None` on timeout, transport or server errors — callers keep
//! their local fallback path. Every decision is logged to
//! `<state dir>/logs/typesafe.log` so routing can be audited.

use reqwest::Client;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-1.13.0";
const QUESTION_ID: &str = "decision";
const RETRY_DELAY: Duration = Duration::from_millis(800);
const RETRY_JITTER_MS: u64 = 200;
const LOG_FILE: &str = "logs/typesafe.log";
/// Identical questions keep their first answer briefly: moim parts are
/// recomputed on every inference of a turn and must render byte-identically,
/// and repeat calls within a turn should not cost another API round trip.
const DECISION_CACHE_TTL: Duration = Duration::from_secs(60);
const DECISION_CACHE_CAP: usize = 64;

static CLIENT: OnceLock<Option<JevClient>> = OnceLock::new();

pub struct JevClient {
    http: Client,
    api_key: String,
    decisions: Mutex<HashMap<u64, (Instant, Option<String>)>>,
}

impl JevClient {
    /// Builds a client when `TYPESAFE_API_KEY` is set (environment first, then
    /// `<config dir>/typesafe.env`). `None` turns every Jev call site off.
    pub fn from_env() -> Option<Self> {
        api_key().map(|api_key| Self {
            http: Client::new(),
            api_key,
            decisions: Mutex::new(HashMap::new()),
        })
    }

    /// The process-wide client, resolved once. Prefer this over `from_env` so
    /// the decision cache persists across call sites.
    pub fn shared() -> Option<&'static Self> {
        CLIENT.get_or_init(Self::from_env).as_ref()
    }

    /// Asks Jev several Choice questions in one request — answers arrive in the
    /// same parallel pass, so a decision made of multiple picks costs one call.
    /// Returns question_id -> answer on 200; `None` on transport failure.
    pub async fn decide_all(
        &self,
        state: &Value,
        questions: &BTreeMap<String, Question>,
        deadline: Instant,
    ) -> Option<BTreeMap<String, Answer>> {
        if questions.is_empty() {
            return None;
        }
        let questions_json = questions
            .iter()
            .map(|(id, q)| {
                (
                    id.clone(),
                    json!({"type": "choice", "instructions": q.instructions, "criteria": q.criteria}),
                )
            })
            .collect::<serde_json::Map<String, Value>>();
        let body = json!({
            "state": state,
            "model": MODEL,
            "questions": Value::Object(questions_json),
        });
        let started = Instant::now();
        let (answers, failure) = self.call_with_retry_all(&body, deadline).await;
        if answers.is_empty() {
            log_decision("batch", &failure, started.elapsed());
        } else {
            for (id, answer) in &answers {
                let mut outcome = Outcome::ok(200, answer.choice.clone());
                outcome.confidence = Some(answer.confidence);
                log_decision(id, &outcome, started.elapsed());
            }
        }
        (!answers.is_empty()).then_some(answers)
    }

    /// Asks Jev to pick one of `options` (option -> rubric) for `state` under
    /// `instructions`, bounded by `deadline`. `None` on transport error,
    /// timeout, non-200 status, or a response with no choice.
    pub async fn decision(
        &self,
        instructions: &str,
        state: &Value,
        options: &BTreeMap<String, String>,
        deadline: Instant,
    ) -> Option<String> {
        if options.is_empty() {
            return None;
        }
        let key = cache_key(instructions, state, options);
        if let Some(hit) = cached_decision(&self.decisions, key) {
            return hit;
        }
        let body = json!({
            "state": state,
            "model": MODEL,
            "questions": {
                QUESTION_ID: {
                    "type": "choice",
                    "instructions": instructions,
                    "criteria": options,
                }
            }
        });
        let started = Instant::now();
        let outcome = self.call_with_retry(&body, deadline).await;
        log_decision(instructions, &outcome, started.elapsed());
        if outcome.error.is_none() {
            store_decision(&self.decisions, key, outcome.choice.clone());
        }
        outcome.choice
    }

    /// One request plus a single retry for 429/5xx after ~800ms of jittered
    /// backoff. Timeout and connection failures return immediately — the local
    /// fallback path is cheaper than a second doomed attempt.
    async fn call_with_retry(&self, body: &Value, deadline: Instant) -> Outcome {
        let mut retried = false;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Outcome::error("deadline");
            };
            match tokio::time::timeout(remaining, self.post(body)).await {
                Err(_) => return Outcome::error("timeout"),
                Ok(Err(error)) => return Outcome::error(error.to_string()),
                Ok(Ok((status, payload))) => {
                    if (status == 429 || status >= 500) && !retried {
                        retried = true;
                        if !backoff(deadline).await {
                            return Outcome::status(status);
                        }
                        continue;
                    }
                    if status != 200 {
                        return Outcome::status(status);
                    }
                    return match extract_choice(&payload) {
                        Some(choice) => {
                            let mut result = Outcome::ok(status, choice);
                            result.confidence = payload
                                .pointer("/answers/decision/confidence")
                                .and_then(Value::as_f64);
                            result
                        }
                        None => Outcome {
                            confidence: None,
                            choice: None,
                            status: Some(status),
                            error: Some("no choice".to_string()),
                        },
                    };
                }
            }
        }
    }

    /// Same retry policy as `call_with_retry`, but returns the whole answers
    /// map (question_id -> choice+confidence) for a multi-question request,
    /// plus the failure that produced an empty map so callers can log it.
    async fn call_with_retry_all(
        &self,
        body: &Value,
        deadline: Instant,
    ) -> (BTreeMap<String, Answer>, Outcome) {
        let mut retried = false;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return (BTreeMap::new(), Outcome::error("deadline"));
            };
            match tokio::time::timeout(remaining, self.post(body)).await {
                Err(_) => return (BTreeMap::new(), Outcome::error("timeout")),
                Ok(Err(error)) => {
                    return (BTreeMap::new(), Outcome::error(error.to_string()));
                }
                Ok(Ok((status, payload))) => {
                    if (status == 429 || status >= 500) && !retried {
                        retried = true;
                        if backoff(deadline).await {
                            continue;
                        }
                        return (BTreeMap::new(), Outcome::status(status));
                    }
                    if status != 200 {
                        return (BTreeMap::new(), Outcome::status(status));
                    }
                    let answers = extract_answers(&payload);
                    if answers.is_empty() {
                        return (answers, Outcome::error("no answers"));
                    }
                    return (answers, Outcome::ok(status, String::new()));
                }
            }
        }
    }

    async fn post(&self, body: &Value) -> Result<(u16, Value), reqwest::Error> {
        let response = self
            .http
            .post(ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await?;
        let status = response.status().as_u16();
        let payload = response.json::<Value>().await.unwrap_or_default();
        Ok((status, payload))
    }
}

/// One Choice question in a `decide_all` request.
pub struct Question {
    pub instructions: String,
    pub criteria: BTreeMap<String, String>,
}

/// A single answered Choice: the option picked and Jev's calibrated confidence.
pub struct Answer {
    pub choice: String,
    pub confidence: f64,
}

struct Outcome {
    confidence: Option<f64>,
    choice: Option<String>,
    status: Option<u16>,
    error: Option<String>,
}

impl Outcome {
    fn ok(status: u16, choice: String) -> Self {
        Self {
            confidence: None,
            choice: Some(choice),
            status: Some(status),
            error: None,
        }
    }

    fn status(status: u16) -> Self {
        Self {
            confidence: None,
            choice: None,
            status: Some(status),
            error: None,
        }
    }

    fn error(error: impl Into<String>) -> Self {
        Self {
            confidence: None,
            choice: None,
            status: None,
            error: Some(error.into()),
        }
    }
}

fn cache_key(instructions: &str, state: &Value, options: &BTreeMap<String, String>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    instructions.hash(&mut hasher);
    serde_json::to_string(state)
        .unwrap_or_default()
        .hash(&mut hasher);
    for (option, rubric) in options {
        option.hash(&mut hasher);
        rubric.hash(&mut hasher);
    }
    hasher.finish()
}

fn cached_decision(
    cache: &Mutex<HashMap<u64, (Instant, Option<String>)>>,
    key: u64,
) -> Option<Option<String>> {
    let cache = cache.lock().ok()?;
    let (expires, choice) = cache.get(&key)?;
    (*expires > Instant::now()).then(|| choice.clone())
}

fn store_decision(
    cache: &Mutex<HashMap<u64, (Instant, Option<String>)>>,
    key: u64,
    choice: Option<String>,
) {
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= DECISION_CACHE_CAP {
            cache.retain(|_, (expires, _)| *expires > Instant::now());
            if cache.len() >= DECISION_CACHE_CAP {
                cache.clear();
            }
        }
        cache.insert(key, (Instant::now() + DECISION_CACHE_TTL, choice));
    }
}

/// Sleeps ~800ms + jitter, unless the deadline cannot afford it.
async fn backoff(deadline: Instant) -> bool {
    let jitter = Duration::from_millis(rand::random::<u64>() % RETRY_JITTER_MS);
    let delay = RETRY_DELAY + jitter;
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return false;
    };
    if delay >= remaining {
        return false;
    }
    tokio::time::sleep(delay).await;
    true
}

fn extract_choice(payload: &Value) -> Option<String> {
    payload
        .get("answers")?
        .get(QUESTION_ID)?
        .get("choice")?
        .as_str()
        .map(str::to_string)
}

/// `answers` is {question_id: {choice, confidence}} — pull each answered Choice
/// out; a question with no choice is skipped rather than failing the batch.
fn extract_answers(payload: &Value) -> BTreeMap<String, Answer> {
    let mut out = BTreeMap::new();
    let Some(answers) = payload.get("answers").and_then(|a| a.as_object()) else {
        return out;
    };
    for (id, value) in answers {
        let Some(choice) = value.get("choice").and_then(|c| c.as_str()) else {
            continue;
        };
        let confidence = value
            .get("confidence")
            .and_then(|c| c.as_f64())
            .filter(|c| c.is_finite() && (0.0..=1.0).contains(c))
            .unwrap_or(0.0);
        out.insert(
            id.clone(),
            Answer {
                choice: choice.to_string(),
                confidence,
            },
        );
    }
    out
}

fn api_key() -> Option<String> {
    std::env::var("TYPESAFE_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(read_env_file_key)
}

/// `typesafe.env` holds `TYPESAFE_API_KEY=...` for users who keep the key out
/// of their shell environment; first matching line wins.
fn read_env_file_key() -> Option<String> {
    let path = crate::config::paths::Paths::in_config_dir("typesafe.env");
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("TYPESAFE_API_KEY") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Appends one JSONL record per decision so Jev routing stays auditable. Log
/// write failures only warn — a broken log file must not break routing.
fn log_decision(instructions: &str, outcome: &Outcome, elapsed: Duration) {
    let path = crate::config::paths::Paths::in_state_dir(LOG_FILE);
    let record = json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "instructions": instructions,
        "choice": outcome.choice,
        "confidence": outcome.confidence,
        "model": MODEL,
        "status": outcome.status,
        "error": outcome.error,
        "elapsed_ms": elapsed.as_millis(),
    });
    if let Some(dir) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(dir) {
            tracing::warn!("typesafe log directory could not be created: {error}");
            return;
        }
    }
    let write = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| writeln!(file, "{record}"));
    if let Err(error) = write {
        tracing::warn!("typesafe decision could not be logged: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_not_inferred_from_winning_probability() {
        let answers =
            extract_answers(&json!({"answers":{"x":{"choice":"a","probabilities":{"a":0.99}}}}));
        assert_eq!(answers["x"].confidence, 0.0);
        let answers = extract_answers(&json!({"answers":{"x":{"choice":"a","confidence":2.0}}}));
        assert_eq!(answers["x"].confidence, 0.0);
    }

    #[test]
    fn extract_reads_the_choice_answer() {
        let payload = json!({
            "model": "jev-latest",
            "answers": {
                "decision": {
                    "type": "choice",
                    "choice": "web-search",
                    "probabilities": { "web-search": 0.9, "none": 0.1 },
                    "confidence": 0.8
                }
            }
        });
        assert_eq!(extract_choice(&payload).as_deref(), Some("web-search"));
        assert_eq!(extract_choice(&json!({})), None);
        assert_eq!(
            extract_choice(&json!({"answers": {"decision": {"type": "noul", "noul": 0.9}}})),
            None
        );
    }

    #[test]
    fn env_file_key_is_parsed_when_env_var_is_unset() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_root = temp_dir.path().display().to_string();
        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("typesafe.env"),
            "# comment\nOTHER=x\nTYPESAFE_API_KEY = \"ts_test_key\"\n",
        )
        .unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
            ("TYPESAFE_API_KEY", None::<&str>),
        ]);

        assert_eq!(api_key().as_deref(), Some("ts_test_key"));
    }

    #[test]
    fn env_var_beats_env_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_root = temp_dir.path().display().to_string();
        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("typesafe.env"),
            "TYPESAFE_API_KEY=file_key\n",
        )
        .unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
            ("TYPESAFE_API_KEY", Some("env_key")),
        ]);

        assert_eq!(api_key().as_deref(), Some("env_key"));
    }
}
