//! Minimal Chrome DevTools Protocol client.
//!
//! CDP is a WebSocket protocol with an HTTP discovery step. We only need
//! request/response, so this is a small client rather than a full CDP library:
//!
//!   1. `GET /json/version` on the debug port  -> `webSocketDebuggerUrl`
//!   2. connect that URL
//!   3. send `{id, method, params}`, await the matching `{id, result}`

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// Where Chrome's debug port lives.
#[derive(Debug, Clone)]
pub struct CdpTarget {
    pub host: String,
    pub port: u16,
}

impl Default for CdpTarget {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            // 9222 is Chrome's conventional remote-debugging port
            port: 9222,
        }
    }
}

impl CdpTarget {
    pub fn http_base(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Discover the page target's WebSocket URL.
    pub async fn discover(&self) -> Result<String> {
        let client = reqwest::Client::new();
        let url = format!("{}/json/version", self.http_base());
        let body: Value = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("cannot reach Chrome debug port at {url}"))?
            .json()
            .await
            .context("Chrome /json/version did not return JSON")?;

        body.get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .with_context(|| {
                format!(
                    "no webSocketDebuggerUrl in {url} response; is Chrome running with --remote-debugging-port={}?",
                    self.port
                )
            })
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A connected CDP session.
pub struct CdpClient {
    ws: Mutex<Ws>,
    next_id: AtomicU64,
}

impl CdpClient {
    /// Connect to a discovered WebSocket URL.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .with_context(|| format!("websocket connect to {ws_url} failed"))?;
        Ok(Self {
            ws: Mutex::new(ws),
            next_id: AtomicU64::new(1),
        })
    }

    /// Discover and connect in one step.
    pub async fn attach(target: &CdpTarget) -> Result<Self> {
        let url = target.discover().await?;
        Self::connect(&url).await
    }

    /// Send one command and await its result.
    ///
    /// Events (messages without a matching id) are skipped: we are only
    /// interested in request/response here.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let payload = serde_json::json!({ "id": id, "method": method, "params": params });
        let mut ws = self.ws.lock().await;

        ws.send(Message::Text(payload.to_string().into()))
            .await
            .with_context(|| format!("send {method} failed"))?;

        // bounded: don't spin forever if the peer stops answering
        for _ in 0..512 {
            let msg = match ws.next().await {
                Some(m) => m.context("websocket closed while awaiting reply")?,
                None => anyhow::bail!("websocket ended while awaiting {method}"),
            };
            let text = match msg {
                Message::Text(t) => t.to_string(),
                Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
                Message::Close(_) => anyhow::bail!("websocket closed during {method}"),
                _ => continue,
            };
            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("id").and_then(|i| i.as_u64()) != Some(id) {
                continue; // an event, not our reply
            }
            if let Some(err) = v.get("error") {
                anyhow::bail!("CDP {method} error: {err}");
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
        anyhow::bail!("no reply to {method} within 512 messages")
    }

    /// Capture a screenshot, returning PNG bytes.
    pub async fn screenshot(&self, params: Value) -> Result<Vec<u8>> {
        let result = self.call("Page.captureScreenshot", params).await?;
        let b64 = result
            .get("data")
            .and_then(|d| d.as_str())
            .context("Page.captureScreenshot returned no data")?;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .context("screenshot data was not valid base64")
    }

    /// Current page URL and title, for reporting what was actually on screen.
    pub async fn page_info(&self) -> Result<HashMap<String, String>> {
        let r = self
            .call(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": "JSON.stringify({url: location.href, title: document.title})",
                    "returnByValue": true
                }),
            )
            .await?;
        let raw = r
            .get("result")
            .and_then(|x| x.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let parsed: HashMap<String, String> = serde_json::from_str(raw).unwrap_or_default();
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_target_is_loopback_9222() {
        let t = CdpTarget::default();
        assert_eq!(t.host, "127.0.0.1");
        assert_eq!(t.port, 9222);
        assert_eq!(t.http_base(), "http://127.0.0.1:9222");
    }

    #[tokio::test]
    async fn discover_fails_cleanly_when_nothing_listens() {
        // port 1 is reserved and never listening
        let t = CdpTarget {
            host: "127.0.0.1".into(),
            port: 1,
        };
        let err = t.discover().await.unwrap_err().to_string();
        assert!(
            err.contains("cannot reach Chrome debug port"),
            "should explain the failure, got: {err}"
        );
    }

    #[tokio::test]
    async fn connect_to_a_dead_websocket_errors() {
        let err = match CdpClient::connect("ws://127.0.0.1:1/devtools/page/x").await {
            Ok(_) => panic!("port 1 should never accept a websocket"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("websocket connect"), "got: {err}");
    }
}
