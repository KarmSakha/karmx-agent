use goose_providers::conversation::token_usage::Usage;

pub(super) struct LiveStatus {
    pub model: String,
    pub context_limit: Option<usize>,
    pub tool_count: usize,
    pub usage: Usage,
}

impl LiveStatus {
    pub fn render(&self, activity: &str, elapsed: f64, completed_tools: usize) -> String {
        let tokens = |value: Option<i32>| {
            value
                .filter(|value| *value >= 0)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string())
        };
        let context = match (self.usage.total_tokens, self.context_limit) {
            (Some(used), Some(limit)) if used >= 0 && limit > 0 => {
                format!(
                    "{used}/{limit} ({:.0}%)",
                    used as f64 * 100.0 / limit as f64
                )
            }
            (_, Some(limit)) => format!("—/{limit}"),
            _ => "unreported".to_string(),
        };
        format!(
            "{activity} · {elapsed:.0}s · {completed_tools} tools completed · Ctrl+C to interrupt\n  {} · {} loaded tools · context {}\n  Last reported tokens: in {} · out {} · cache read {} · write {}",
            self.model,
            self.tool_count,
            context,
            tokens(self.usage.input_tokens),
            tokens(self.usage.output_tokens),
            tokens(self.usage.cache_read_input_tokens),
            tokens(self.usage.cache_write_input_tokens),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_usage_does_not_look_like_a_zero_token_request() {
        let status = LiveStatus {
            model: "provider/model".to_string(),
            context_limit: Some(32_000),
            tool_count: 7,
            usage: Usage::default(),
        };
        let rendered = status.render("Waiting for model", 2.0, 0);
        assert!(rendered.contains("context —/32000"));
        assert!(rendered.contains("in — · out — · cache read — · write —"));
        assert!(!rendered.contains("(0%)"));
    }

    #[test]
    fn updated_usage_keeps_overflow_and_real_zero_cache_counts_visible() {
        let status = LiveStatus {
            model: "provider/model".to_string(),
            context_limit: Some(1_000),
            tool_count: 7,
            usage: Usage {
                total_tokens: Some(1_100),
                input_tokens: Some(1_000),
                output_tokens: Some(100),
                cache_read_input_tokens: Some(0),
                cache_write_input_tokens: None,
            },
        };
        let rendered = status.render("Running shell", 3.0, 2);
        assert!(rendered.contains("context 1100/1000 (110%)"));
        assert!(rendered.contains("in 1000 · out 100 · cache read 0 · write —"));
        assert!(rendered.contains("7 loaded tools"));
        assert!(rendered.contains("2 tools completed"));
    }
}
