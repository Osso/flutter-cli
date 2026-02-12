use anyhow::{Result, anyhow};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

pub struct VmServiceConnection {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: i64,
}

impl VmServiceConnection {
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to VM Service at {ws_url}: {e}"))?;
        Ok(Self { ws, next_id: 1 })
    }

    /// Send a JSON-RPC 2.0 request and wait for the matching response.
    /// Skips over events (messages without an "id" field).
    pub async fn send(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.ws.send(Message::Text(msg.to_string())).await?;

        while let Some(msg) = self.ws.next().await {
            if let Ok(Message::Text(text)) = msg {
                let mut de = serde_json::Deserializer::from_str(&text);
                de.disable_recursion_limit();
                let resp = serde_json::Value::deserialize(&mut de)?;

                // Skip events (no id field)
                let Some(resp_id) = resp.get("id") else {
                    continue;
                };
                if resp_id != &serde_json::json!(id) {
                    continue;
                }

                if let Some(error) = resp.get("error") {
                    let msg = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error");
                    let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                    return Err(anyhow!("VM Service error {code}: {msg}"));
                }

                return Ok(resp.get("result").cloned().unwrap_or(serde_json::json!({})));
            }
        }
        Err(anyhow!("WebSocket closed without response"))
    }

    /// Check if connection is alive by sending getVersion
    pub async fn ping(&mut self) -> bool {
        self.send("getVersion", serde_json::json!({})).await.is_ok()
    }
}

/// Try to connect to a VM Service URL with a timeout.
pub async fn try_connect(ws_url: &str, timeout_ms: u64) -> Result<VmServiceConnection> {
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        VmServiceConnection::connect(ws_url),
    )
    .await;

    match result {
        Ok(conn) => conn,
        Err(_) => Err(anyhow!("Connection to {ws_url} timed out")),
    }
}
