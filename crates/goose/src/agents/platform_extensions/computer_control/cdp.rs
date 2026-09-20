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
use std::time::Duration;

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
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()?;
        let url = format!("{}/json/version", self.http_base());
        let body: Value = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("cannot reach Chrome debug port at {url}"))?
            .error_for_status()
            .with_context(|| format!("Chrome discovery at {url} returned an HTTP error"))?
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
    session_id: Option<String>,
}

impl CdpClient {
    /// Connect to a discovered WebSocket URL.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (ws, _) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(ws_url),
        )
        .await
        .context("Chrome websocket connection timed out")?
        .with_context(|| format!("websocket connect to {ws_url} failed"))?;
        Ok(Self {
            ws: Mutex::new(ws),
            next_id: AtomicU64::new(1),
            session_id: None,
        })
    }

    /// Discover and connect in one step.
    pub async fn attach(target: &CdpTarget) -> Result<Self> {
        let url = target.discover().await?;
        let mut client = Self::connect(&url).await?;
        // Own a new page; never select or navigate an unrelated user tab.
        let target = client
            .call(
                "Target.createTarget",
                serde_json::json!({"url": "about:blank"}),
            )
            .await?;
        let target_id = target
            .get("targetId")
            .and_then(Value::as_str)
            .context("Chrome did not create a page target")?;
        let attached = client
            .call(
                "Target.attachToTarget",
                serde_json::json!({"targetId": target_id, "flatten": true}),
            )
            .await?;
        client.session_id = Some(
            attached
                .get("sessionId")
                .and_then(Value::as_str)
                .context("Chrome did not attach to the page target")?
                .to_owned(),
        );
        Ok(client)
    }

    /// Send one command and await its result.
    ///
    /// Events (messages without a matching id) are skipped: we are only
    /// interested in request/response here.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(15), self.call_inner(method, params))
            .await
            .with_context(|| format!("Chrome did not respond to {method} within 15 seconds"))?
    }

    async fn call_inner(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut payload = serde_json::json!({ "id": id, "method": method, "params": params });
        if let Some(session_id) = &self.session_id {
            payload["sessionId"] = Value::String(session_id.clone());
        }
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

    pub async fn ensure_url(&self, requested: &str) -> Result<()> {
        let target = url::Url::parse(requested).context("Invalid browser URL")?;
        let current = self.page_info().await?;
        if current
            .get("url")
            .and_then(|u| url::Url::parse(u).ok())
            .as_ref()
            != Some(&target)
        {
            self.navigate(target.as_str()).await?;
        }
        Ok(())
    }

    pub async fn navigate(&self, url: &str) -> Result<()> {
        let result = self
            .call("Page.navigate", serde_json::json!({"url": url}))
            .await?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            anyhow::bail!("Navigation failed: {error}");
        }
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let state = self
                    .call(
                        "Runtime.evaluate",
                        serde_json::json!({
                            "expression": "document.readyState", "returnByValue": true
                        }),
                    )
                    .await?;
                if matches!(
                    state.pointer("/result/value").and_then(Value::as_str),
                    Some("interactive" | "complete")
                ) {
                    return Ok::<(), anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .context("Page did not finish loading within 15 seconds")?
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
                    "expression": "JSON.stringify({url: location.href, title: document.title, text: document.body?.innerText.slice(0, 12000) || ''})",
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

    async fn discovery_server(status: &str, body: String) -> CdpTarget {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            socket.read(&mut request).await.unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        CdpTarget {
            host: "127.0.0.1".into(),
            port,
        }
    }

    #[tokio::test]
    async fn discovery_rejects_http_errors_even_with_valid_json() {
        let target = discovery_server("404 Not Found", "{}".into()).await;
        assert!(target
            .discover()
            .await
            .unwrap_err()
            .to_string()
            .contains("HTTP error"));
    }

    #[tokio::test]
    async fn discovery_requires_websocket_endpoint() {
        let target = discovery_server("200 OK", "{}".into()).await;
        assert!(target
            .discover()
            .await
            .unwrap_err()
            .to_string()
            .contains("no webSocketDebuggerUrl"));
    }

    #[tokio::test]
    async fn attach_creates_own_page_and_routes_page_commands_to_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_url = format!(
            "ws://{}/devtools/browser/test",
            listener.local_addr().unwrap()
        );
        let target = discovery_server(
            "200 OK",
            serde_json::json!({"webSocketDebuggerUrl": ws_url}).to_string(),
        )
        .await;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            for (method, response) in [
                (
                    "Target.createTarget",
                    serde_json::json!({"targetId": "owned-page"}),
                ),
                (
                    "Target.attachToTarget",
                    serde_json::json!({"sessionId": "owned-session"}),
                ),
                (
                    "Page.navigate",
                    serde_json::json!({"errorText": "net::ERR_NAME_NOT_RESOLVED"}),
                ),
            ] {
                let request: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                assert_eq!(request["method"], method);
                if method == "Target.createTarget" {
                    assert_eq!(request["params"]["url"], "about:blank");
                }
                if method == "Target.attachToTarget" {
                    assert_eq!(request["params"]["targetId"], "owned-page");
                    assert_eq!(request["params"]["flatten"], true);
                }
                if method == "Page.navigate" {
                    assert_eq!(request["sessionId"], "owned-session");
                }
                ws.send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": response})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            }
        });
        let client = CdpClient::attach(&target).await.unwrap();
        assert!(client
            .navigate("https://invalid.example")
            .await
            .unwrap_err()
            .to_string()
            .contains("ERR_NAME_NOT_RESOLVED"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn repeated_url_preserves_page_state() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = discovery_server(
            "200 OK",
            serde_json::json!({
                "webSocketDebuggerUrl": format!("ws://{}/browser", listener.local_addr().unwrap())
            })
            .to_string(),
        )
        .await;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            for (method, response) in [
                (
                    "Target.createTarget",
                    serde_json::json!({"targetId":"page"}),
                ),
                (
                    "Target.attachToTarget",
                    serde_json::json!({"sessionId":"session"}),
                ),
                (
                    "Runtime.evaluate",
                    serde_json::json!({"result":{"value":"{\"url\":\"http://localhost:8771/\"}"}}),
                ),
                (
                    "Runtime.evaluate",
                    serde_json::json!({"result":{"value":"{\"url\":\"http://localhost:8771/\"}"}}),
                ),
                ("Page.navigate", serde_json::json!({})),
                (
                    "Runtime.evaluate",
                    serde_json::json!({"result":{"value":"complete"}}),
                ),
            ] {
                let req: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                assert_eq!(
                    req["method"], method,
                    "same URL must never issue Page.navigate"
                );
                ws.send(Message::Text(
                    serde_json::json!({"id":req["id"],"result":response})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            }
        });
        let client = CdpClient::attach(&target).await.unwrap();
        client.ensure_url("http://localhost:8771").await.unwrap();
        client
            .ensure_url("http://localhost:8771/details")
            .await
            .unwrap();
        server.await.unwrap();
    }

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
