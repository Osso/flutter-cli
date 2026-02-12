use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::isolate;
use crate::process;
use crate::snapshot::{self, SnapshotOptions};
use crate::state::State;

fn resolve_project_dir(project_dir: Option<String>) -> Result<PathBuf> {
    match project_dir {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => std::env::current_dir().context("Failed to get current directory"),
    }
}

pub async fn cmd_snapshot(
    project_dir: Option<String>,
    url: Option<String>,
    depth: Option<usize>,
    filter: Option<String>,
    compact: bool,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;

    let tree = snapshot::get_widget_tree(&mut conn).await?;
    let opts = SnapshotOptions {
        max_depth: depth,
        filter,
        compact,
    };
    let output = snapshot::format_tree(&tree, &opts);

    if json {
        println!("{}", serde_json::json!({ "tree": output }));
    } else if output.is_empty() {
        println!("(empty widget tree)");
    } else {
        println!("{output}");
    }
    Ok(())
}

pub async fn cmd_screenshot(
    project_dir: Option<String>,
    url: Option<String>,
    id: Option<String>,
    path: &str,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;

    let mut params = serde_json::json!({
        "isolateId": isolate_id,
        "width": 1080.0,
        "height": 1920.0,
        "maxPixelRatio": 2.0,
    });
    if let Some(ref id) = id {
        params["id"] = serde_json::json!(id);
    }

    let result = conn
        .send("ext.flutter.inspector.screenshot", params)
        .await?;

    let image_data = result
        .get("screenshot")
        .and_then(|s| s.as_str())
        .context("No screenshot data in response")?;

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(image_data)?;

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &bytes)?;

    if json {
        println!(
            "{}",
            serde_json::json!({ "path": path, "bytes": bytes.len() })
        );
    } else {
        println!("Screenshot saved to {path} ({} bytes)", bytes.len());
    }
    Ok(())
}

pub async fn cmd_details(
    project_dir: Option<String>,
    url: Option<String>,
    value_id: &str,
    depth: usize,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;
    let object_group = "flutter-cli-details";

    let result = conn
        .send(
            "ext.flutter.inspector.getDetailsSubtree",
            serde_json::json!({
                "isolateId": isolate_id,
                "arg": value_id,
                "objectGroup": object_group,
                "subtreeDepth": depth,
            }),
        )
        .await?;

    // Cleanup
    let _ = conn
        .send(
            "ext.flutter.inspector.disposeGroup",
            serde_json::json!({
                "isolateId": isolate_id,
                "objectGroup": object_group,
            }),
        )
        .await;

    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}

pub async fn cmd_layout(
    project_dir: Option<String>,
    url: Option<String>,
    value_id: &str,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;
    let object_group = "flutter-cli-layout";

    let result = conn
        .send(
            "ext.flutter.inspector.getLayoutExplorerNode",
            serde_json::json!({
                "isolateId": isolate_id,
                "id": value_id,
                "groupName": object_group,
                "subtreeDepth": 1,
            }),
        )
        .await?;

    let _ = conn
        .send(
            "ext.flutter.inspector.disposeGroup",
            serde_json::json!({
                "isolateId": isolate_id,
                "objectGroup": object_group,
            }),
        )
        .await;

    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}

pub async fn cmd_dump_render(
    project_dir: Option<String>,
    url: Option<String>,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;

    let result = conn
        .send(
            "ext.flutter.debugDumpRenderTree",
            serde_json::json!({ "isolateId": isolate_id }),
        )
        .await?;

    let text = result.get("data").and_then(|d| d.as_str()).unwrap_or("");

    if json {
        println!("{}", serde_json::json!({ "render_tree": text }));
    } else {
        println!("{text}");
    }
    Ok(())
}

pub async fn cmd_dump_semantics(
    project_dir: Option<String>,
    url: Option<String>,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;

    let result = conn
        .send(
            "ext.flutter.debugDumpSemanticsTreeInTraversalOrder",
            serde_json::json!({ "isolateId": isolate_id }),
        )
        .await?;

    let text = result.get("data").and_then(|d| d.as_str()).unwrap_or("");

    if json {
        println!("{}", serde_json::json!({ "semantics_tree": text }));
    } else {
        println!("{text}");
    }
    Ok(())
}

pub async fn cmd_reload(
    project_dir: Option<String>,
    url: Option<String>,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;

    // Hot reload via flutter run --machine stdin protocol
    if url.is_none() {
        if let Some(state) = State::load(&project_dir)? {
            if state.is_pid_alive() {
                return send_machine_command(&state, false, json);
            }
        }
    }

    // Fallback: use VM Service directly
    let mut conn = process::ensure_connection(&project_dir, url.as_deref()).await?;
    let isolate_id = isolate::find_flutter_isolate(&mut conn).await?;

    let result = conn
        .send(
            "ext.flutter.reassemble",
            serde_json::json!({ "isolateId": isolate_id }),
        )
        .await?;

    if json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("Hot reload triggered");
    }
    Ok(())
}

pub async fn cmd_restart(
    project_dir: Option<String>,
    url: Option<String>,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;

    // Hot restart via flutter run --machine stdin protocol
    if url.is_none() {
        if let Some(state) = State::load(&project_dir)? {
            if state.is_pid_alive() {
                return send_machine_command(&state, true, json);
            }
        }
    }

    // Fallback: VM Service doesn't have a clean hot restart method
    // without the flutter tool, so we need the managed process
    anyhow::bail!("Hot restart requires a managed flutter run process. Run without --url first.");
}

fn send_machine_command(state: &State, full_restart: bool, json: bool) -> Result<()> {
    use std::io::Write;

    let app_id = state.app_id.as_deref().unwrap_or("");
    let cmd = serde_json::json!([{
        "method": "app.restart",
        "params": {
            "appId": app_id,
            "fullRestart": full_restart,
            "reason": "flutter-cli",
        }
    }]);

    // Write to the flutter run process stdin via /proc/PID/fd/0
    let stdin_path = format!("/proc/{}/fd/0", state.pid);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&stdin_path)
        .context("Failed to write to flutter run stdin")?;
    writeln!(file, "{}", cmd)?;

    let action = if full_restart {
        "Hot restart"
    } else {
        "Hot reload"
    };

    if json {
        println!(
            "{}",
            serde_json::json!({ "action": action, "status": "sent" })
        );
    } else {
        println!("{action} triggered");
    }
    Ok(())
}

pub async fn cmd_status(
    project_dir: Option<String>,
    url: Option<String>,
    json: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;

    if let Some(ref url) = url {
        let mut conn = crate::vm_service::VmServiceConnection::connect(url).await?;
        let alive = conn.ping().await;
        if json {
            println!(
                "{}",
                serde_json::json!({ "url": url, "connected": alive, "managed": false })
            );
        } else {
            println!("URL: {url}");
            println!("Connected: {alive}");
        }
        return Ok(());
    }

    match State::load(&project_dir)? {
        Some(state) => {
            let pid_alive = state.is_pid_alive();
            let ws_reachable = if pid_alive {
                match crate::vm_service::try_connect(&state.ws_uri, 2000).await {
                    Ok(mut conn) => conn.ping().await,
                    Err(_) => false,
                }
            } else {
                false
            };

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "pid": state.pid,
                        "ws_uri": state.ws_uri,
                        "app_id": state.app_id,
                        "pid_alive": pid_alive,
                        "ws_reachable": ws_reachable,
                        "managed": true,
                    })
                );
            } else {
                println!(
                    "PID: {} ({})",
                    state.pid,
                    if pid_alive { "alive" } else { "dead" }
                );
                println!("VM Service: {}", state.ws_uri);
                if let Some(ref id) = state.app_id {
                    println!("App ID: {id}");
                }
                println!("Reachable: {ws_reachable}");
            }
        }
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "managed": false, "running": false })
                );
            } else {
                println!("No managed flutter run process");
            }
        }
    }
    Ok(())
}

pub async fn cmd_stop(project_dir: Option<String>) -> Result<()> {
    let project_dir = resolve_project_dir(project_dir)?;
    process::stop_process(&project_dir)
}
