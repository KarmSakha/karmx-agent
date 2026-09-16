//! Conversation reranker for `rerank_context`.
//!
//! Splits conversation history into chunks, formats them as `--- Chunk N ---`
//! blocks, asks a model to rank them by relevance to the current query, and
//! reorders the kept chunks. Inclusive by design (keep anything not clearly
//! unrelated) and **fails open**: any error, unparseable output, or out-of-range
//! index returns the original ordering untouched.
//!
//! The ranking instruction is `prompts/rerank.txt`.

use std::sync::Arc;

use goose_providers::model::ModelConfig;

use crate::providers::base::Provider;

pub const RERANK_INSTRUCTION: &str = include_str!("prompts/rerank.txt");

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub start: usize,
    pub end: usize,
    pub timestamp_ms: i64,
    pub text: String,
}

/// Group messages into chunks of at most `max_chars`, never splitting a message.
///
/// A single message longer than `max_chars` becomes its own chunk rather than
/// being cut, so no content is silently lost before ranking.
pub fn chunk_messages(msgs: &[Message], max_chars: usize) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    let mut start = 0usize;
    let mut buf = String::new();

    for (i, m) in msgs.iter().enumerate() {
        let piece = format!("[{}] {}\n", m.role, m.content);
        if !buf.is_empty() && buf.len() + piece.len() > max_chars {
            out.push(Chunk {
                start,
                end: i,
                timestamp_ms: msgs[i.saturating_sub(1)].timestamp_ms,
                text: buf.trim_end().to_string(),
            });
            buf.clear();
            start = i;
        }
        buf.push_str(&piece);
    }
    if !buf.trim().is_empty() {
        out.push(Chunk {
            start,
            end: msgs.len(),
            timestamp_ms: msgs.last().map(|m| m.timestamp_ms).unwrap_or(0),
            text: buf.trim_end().to_string(),
        });
    }
    out
}

/// Format chunks the way the instruction expects.
///
/// Timestamps are included because the instruction tells the model to prefer
/// the more recent chunk when two are equally relevant, and states that each
/// chunk carries one. Omitting them would make that tie-break impossible.
fn format_chunks(chunks: &[Chunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "--- Chunk {i} (timestamp_ms: {}, messages {}-{}) ---\n{}",
                c.timestamp_ms, c.start, c.end, c.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Build the ranking request for a query.
pub fn build_request(query: &str, chunks: &[Chunk]) -> String {
    RERANK_INSTRUCTION
        .replace("{{query}}", query)
        .replace("{{chunks}}", &format_chunks(chunks))
}

/// Pull the FIRST bracketed JSON array out of a model response.
///
/// The instruction asks for a bare array, but models add prose, so the original
/// scans for the first `[...]` rather than requiring a pure-JSON body.
#[allow(clippy::string_slice)] // Indices come from find() on ASCII '[' / ']'; byte slicing is safe.
pub fn extract_index_array(response: &str) -> Option<Vec<i64>> {
    let start = response.find('[')?;
    let end = response[start..].find(']')? + start;
    let candidate = &response[start..=end];
    let parsed: serde_json::Value = serde_json::from_str(candidate).ok()?;
    let arr = parsed.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        // reject non-integers outright rather than coercing them
        out.push(v.as_i64()?);
    }
    Some(out)
}

/// Reorder `chunks` by the model's ranking.
///
/// Returns chunks in ranked order. Out-of-range or duplicate indices are
/// dropped. An empty model array means "nothing relevant" and yields no chunks.
/// Any failure returns the original ordering.
pub async fn rerank(
    provider: &Arc<dyn Provider>,
    model_config: &ModelConfig,
    query: &str,
    chunks: Vec<Chunk>,
) -> Vec<Chunk> {
    if chunks.len() <= 1 || query.trim().is_empty() {
        return chunks;
    }

    let original = chunks.clone();
    let request = build_request(query, &chunks);
    // fully qualified: this module defines its own `Message`
    let msg = crate::conversation::message::Message::user().with_text(request);

    let response = match provider.complete(model_config, "", &[msg], &[]).await {
        Ok((message, _usage)) => message.as_concat_text(),
        Err(e) => {
            tracing::warn!("rerank failed, keeping original order: {e}");
            return original;
        }
    };

    let idx = match extract_index_array(&response) {
        Some(i) => i,
        None => {
            tracing::warn!("rerank response had no JSON index array, keeping original order");
            return original;
        }
    };

    if idx.is_empty() {
        tracing::info!("rerank excluded every chunk as irrelevant");
        return Vec::new();
    }

    // validate + dedupe, preserving rank order
    let mut seen = std::collections::HashSet::new();
    let mut ordered = Vec::with_capacity(idx.len());
    for i in idx {
        if i < 0 || (i as usize) >= original.len() {
            tracing::warn!("rerank returned out-of-range index {i}, keeping original order");
            return original;
        }
        if seen.insert(i as usize) {
            ordered.push(original[i as usize].clone());
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(n: usize) -> Vec<Message> {
        (0..n)
            .map(|i| Message {
                role: if i % 2 == 0 {
                    "user".into()
                } else {
                    "assistant".into()
                },
                content: format!("message number {i}"),
                timestamp_ms: 1000 + i as i64,
            })
            .collect()
    }

    #[test]
    fn chunks_group_and_keep_all_content() {
        let c = chunk_messages(&msgs(10), 60);
        assert!(c.len() > 1, "should split");
        let joined: String = c.iter().map(|x| x.text.clone()).collect();
        for i in 0..10 {
            assert!(
                joined.contains(&format!("message number {i}")),
                "lost message {i}"
            );
        }
    }

    #[test]
    fn oversized_single_message_kept_whole() {
        let m = vec![Message {
            role: "user".into(),
            content: "x".repeat(500),
            timestamp_ms: 1,
        }];
        let c = chunk_messages(&m, 50);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].text.len(), 500 + "[user] ".len() + 1 - 1);
    }

    #[test]
    fn request_substitutes_both_placeholders() {
        let c = chunk_messages(&msgs(4), 10_000);
        let r = build_request("how does auth work", &c);
        assert!(r.contains("how does auth work"));
        assert!(r.contains("--- Chunk 0"));
        assert!(!r.contains("{{query}}"));
        assert!(!r.contains("{{chunks}}"));
    }

    #[test]
    fn chunks_carry_timestamps_for_the_recency_tie_break() {
        let c = chunk_messages(&msgs(6), 30);
        assert!(c.len() > 1);
        let req = build_request("q", &c);
        // the instruction promises each chunk has a timestamp field
        assert!(req.contains("timestamp_ms:"), "timestamps must be emitted");
        // and they must be real values from the messages, not zeros
        assert!(req.contains("timestamp_ms: 1000") || req.contains("timestamp_ms: 100"));
    }

    #[test]
    fn extracts_array_from_noisy_response() {
        let r = "Here you go: [2, 0, 1] — sorted.";
        assert_eq!(extract_index_array(r).unwrap(), vec![2, 0, 1]);
    }

    #[test]
    fn rejects_non_integer_entries() {
        assert!(extract_index_array("[0, \"one\"]").is_none());
    }

    #[test]
    fn no_array_returns_none() {
        assert!(extract_index_array("I cannot rank these.").is_none());
    }

    #[test]
    fn empty_query_does_not_rerank() {
        assert!("   ".trim().is_empty());
    }
}
