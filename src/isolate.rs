use anyhow::{Result, anyhow};

use crate::vm_service::VmServiceConnection;

/// Discover the Flutter isolate by finding one with ext.flutter.* extensions.
/// Returns the isolate ID.
pub async fn find_flutter_isolate(conn: &mut VmServiceConnection) -> Result<String> {
    let vm = conn.send("getVM", serde_json::json!({})).await?;

    let isolates = vm
        .get("isolates")
        .and_then(|i| i.as_array())
        .ok_or_else(|| anyhow!("No isolates in VM response"))?;

    for isolate_ref in isolates {
        let Some(id) = isolate_ref.get("id").and_then(|i| i.as_str()) else {
            continue;
        };

        let isolate = conn
            .send("getIsolate", serde_json::json!({ "isolateId": id }))
            .await?;

        let extensions = isolate
            .get("extensionRPCs")
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        if extensions.iter().any(|ext| ext.starts_with("ext.flutter")) {
            return Ok(id.to_string());
        }
    }

    Err(anyhow!(
        "No Flutter isolate found. Is a Flutter app running?"
    ))
}
