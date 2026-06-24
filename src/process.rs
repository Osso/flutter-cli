use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::SystemTime;

use crate::config::Config;
use crate::state::State;
use crate::vm_service::{self, VmServiceConnection};

/// Ensure a connection to the Flutter app's VM Service.
/// If --url is provided, connect directly. Otherwise, use process management.
#[cfg_attr(coverage_nightly, coverage(off))]
pub async fn ensure_connection(
    project_dir: &Path,
    url: Option<&str>,
) -> Result<VmServiceConnection> {
    if let Some(url) = url {
        return VmServiceConnection::connect(url).await;
    }

    // Try existing state
    if let Some(state) = State::load(project_dir)? {
        if state.is_pid_alive() {
            if let Ok(mut conn) = vm_service::try_connect(&state.ws_uri, 3000).await
                && conn.ping().await
            {
                return Ok(conn);
            }
            // Connection failed, kill the old process
            eprintln!("VM Service unreachable, restarting flutter run...");
            kill_process(state.pid);
        }
        State::remove(project_dir)?;
    }

    // Start a new flutter run process
    start_flutter_run(project_dir).await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn start_flutter_run(project_dir: &Path) -> Result<VmServiceConnection> {
    let config = Config::load(project_dir)?;
    let args = config.flutter_run_args();
    eprintln!("Starting: flutter {}", args.join(" "));
    let stderr_file = create_stderr_log_file(project_dir)?;
    let mut child = spawn_flutter_process(project_dir, &args, stderr_file)?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("flutter run stdout unavailable"))?;
    let (ws_uri, app_id) = read_startup_vm_service(stdout, pid, project_dir).await?;
    save_flutter_state(project_dir, pid, &ws_uri, app_id, args)?;
    let conn = VmServiceConnection::connect(&ws_uri).await?;
    Ok(conn)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn create_stderr_log_file(project_dir: &Path) -> Result<std::fs::File> {
    let stderr_path = stderr_log_path(project_dir);
    let log_dir = stderr_path
        .parent()
        .ok_or_else(|| anyhow!("Invalid stderr log path: {}", stderr_path.display()))?;
    std::fs::create_dir_all(log_dir)?;
    let stderr_file = std::fs::File::create(&stderr_path)?;
    Ok(stderr_file)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_flutter_process(
    project_dir: &Path,
    args: &[String],
    stderr_file: std::fs::File,
) -> Result<Child> {
    let child = Command::new("flutter")
        .args(args)
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr_file)
        .spawn()
        .map_err(|e| anyhow!("Failed to start flutter: {e}"))?;
    Ok(child)
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn read_startup_vm_service(
    stdout: std::process::ChildStdout,
    pid: u32,
    project_dir: &Path,
) -> Result<(String, Option<String>)> {
    let project_dir_owned = project_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        parse_flutter_machine_output(stdout, pid, &project_dir_owned)
    })
    .await?
}

fn save_flutter_state(
    project_dir: &Path,
    pid: u32,
    ws_uri: &str,
    app_id: Option<String>,
    args: Vec<String>,
) -> Result<()> {
    let started_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let state = State {
        pid,
        ws_uri: ws_uri.to_string(),
        app_id,
        cwd: project_dir.to_string_lossy().to_string(),
        args,
        started_at,
    };
    state.save(project_dir)?;
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn parse_flutter_machine_output(
    stdout: std::process::ChildStdout,
    pid: u32,
    project_dir: &Path,
) -> Result<(String, Option<String>)> {
    let reader = BufReader::new(stdout);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut ws_uri: Option<String> = None;
    let mut app_id: Option<String> = None;
    for line in reader.lines() {
        fail_if_startup_timed_out(deadline, pid, project_dir)?;
        let line = line.map_err(|e| anyhow!("Error reading flutter stdout: {e}"))?;
        let Some(event) = parse_machine_event(&line) else {
            continue;
        };
        if handle_startup_event(&event, &mut ws_uri, &mut app_id, project_dir)? {
            break;
        }
    }
    finalize_startup(ws_uri, app_id, pid, project_dir)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn fail_if_startup_timed_out(
    deadline: std::time::Instant,
    pid: u32,
    project_dir: &Path,
) -> Result<()> {
    if std::time::Instant::now() <= deadline {
        return Ok(());
    }
    kill_process(pid);
    State::remove(project_dir).ok();
    Err(anyhow!("Timeout waiting for flutter run to start (120s)"))
}

fn parse_machine_event(line: &str) -> Option<serde_json::Value> {
    let event = serde_json::from_str::<serde_json::Value>(line).ok()?;
    if event.is_array() {
        return event.as_array().and_then(|a| a.first()).cloned();
    }
    Some(event)
}

fn handle_startup_event(
    event: &serde_json::Value,
    ws_uri: &mut Option<String>,
    app_id: &mut Option<String>,
    project_dir: &Path,
) -> Result<bool> {
    let event_name = event.get("event").and_then(|e| e.as_str());
    match event_name {
        Some("app.debugPort") => {
            update_debug_port_fields(event, ws_uri, app_id);
            Ok(ws_uri.is_some())
        }
        Some("app.started") => Ok(ws_uri.is_some()),
        Some("app.stop") | Some("daemon.shutdown") => {
            State::remove(project_dir).ok();
            Err(anyhow!("Flutter app exited during startup"))
        }
        _ => Ok(false),
    }
}

fn update_debug_port_fields(
    event: &serde_json::Value,
    ws_uri: &mut Option<String>,
    app_id: &mut Option<String>,
) {
    let Some(params) = event.get("params") else {
        return;
    };
    if let Some(uri) = params.get("wsUri").and_then(|u| u.as_str()) {
        *ws_uri = Some(uri.to_string());
    }
    if let Some(id) = params.get("appId").and_then(|a| a.as_str()) {
        *app_id = Some(id.to_string());
    }
}

fn finalize_startup(
    ws_uri: Option<String>,
    app_id: Option<String>,
    pid: u32,
    project_dir: &Path,
) -> Result<(String, Option<String>)> {
    let Some(uri) = ws_uri else {
        kill_process(pid);
        State::remove(project_dir).ok();
        return Err(anyhow!(
            "flutter run exited without providing VM Service URI. Check {}",
            stderr_log_path(project_dir).display()
        ));
    };
    Ok((uri, app_id))
}

fn stderr_log_path(project_dir: &Path) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(project_dir.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    let hex = format!("{:x}", hash);
    let short = &hex[..16];
    std::path::PathBuf::from("/tmp/claude/flutter-cli").join(format!("{short}.stderr"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn kill_process(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    // Give it a moment to shut down gracefully
    std::thread::sleep(std::time::Duration::from_millis(500));
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

/// Stop the managed flutter run process for this project directory.
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn stop_process(project_dir: &Path) -> Result<()> {
    let state = State::load(project_dir)?;
    match state {
        Some(s) => {
            if s.is_pid_alive() {
                kill_process(s.pid);
                eprintln!("Stopped flutter run (PID {})", s.pid);
            } else {
                eprintln!("Process already dead (PID {})", s.pid);
            }
            State::remove(project_dir)?;
            Ok(())
        }
        None => {
            eprintln!("No managed flutter run process found");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flutter-cli-process-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn parse_machine_event_accepts_object_or_singleton_array() {
        let object = parse_machine_event(r#"{"event":"app.started"}"#).unwrap();
        let array = parse_machine_event(r#"[{"event":"app.started"}]"#).unwrap();

        assert_eq!(object["event"], "app.started");
        assert_eq!(array["event"], "app.started");
        assert!(parse_machine_event("not json").is_none());
    }

    #[test]
    fn update_debug_port_fields_extracts_uri_and_app_id() {
        let event = json!({
            "params": {
                "wsUri": "ws://127.0.0.1:1234/ws",
                "appId": "app-123"
            }
        });
        let mut ws_uri = None;
        let mut app_id = None;

        update_debug_port_fields(&event, &mut ws_uri, &mut app_id);

        assert_eq!(ws_uri.as_deref(), Some("ws://127.0.0.1:1234/ws"));
        assert_eq!(app_id.as_deref(), Some("app-123"));
    }

    #[test]
    fn handle_startup_event_reports_when_debug_port_is_ready() {
        let project_dir = temp_project_dir("debug-port");
        let event = json!({
            "event": "app.debugPort",
            "params": {
                "wsUri": "ws://127.0.0.1:1234/ws",
                "appId": "app-123"
            }
        });
        let mut ws_uri = None;
        let mut app_id = None;

        let ready = handle_startup_event(&event, &mut ws_uri, &mut app_id, &project_dir).unwrap();

        assert!(ready);
        assert_eq!(ws_uri.as_deref(), Some("ws://127.0.0.1:1234/ws"));
        assert_eq!(app_id.as_deref(), Some("app-123"));
    }

    #[test]
    fn handle_startup_event_waits_for_debug_port_or_errors_on_stop() {
        let project_dir = temp_project_dir("events");
        let mut ws_uri = None;
        let mut app_id = None;

        assert!(
            !handle_startup_event(
                &json!({"event": "app.started"}),
                &mut ws_uri,
                &mut app_id,
                &project_dir,
            )
            .unwrap()
        );
        assert!(
            handle_startup_event(
                &json!({"event": "daemon.shutdown"}),
                &mut ws_uri,
                &mut app_id,
                &project_dir,
            )
            .is_err()
        );
    }

    #[test]
    fn finalize_startup_returns_uri_and_app_id() {
        let project_dir = temp_project_dir("finalize");

        let (uri, app_id) = finalize_startup(
            Some("ws://127.0.0.1:1234/ws".to_string()),
            Some("app-123".to_string()),
            std::process::id(),
            &project_dir,
        )
        .unwrap();

        assert_eq!(uri, "ws://127.0.0.1:1234/ws");
        assert_eq!(app_id.as_deref(), Some("app-123"));
    }

    #[test]
    fn stderr_log_path_is_stable_and_uses_stderr_extension() {
        let project_dir = PathBuf::from("/tmp/flutter-cli/example");

        let first = stderr_log_path(&project_dir);
        let second = stderr_log_path(&project_dir);

        assert_eq!(first, second);
        assert_eq!(
            first.parent().unwrap(),
            Path::new("/tmp/claude/flutter-cli")
        );
        assert_eq!(
            first.extension().and_then(|ext| ext.to_str()),
            Some("stderr")
        );
    }

    #[test]
    fn save_flutter_state_persists_expected_fields() {
        let project_dir = temp_project_dir("save-state");
        let args = vec!["run".to_string(), "--machine".to_string()];

        save_flutter_state(
            &project_dir,
            42,
            "ws://127.0.0.1:1234/ws",
            Some("app-123".to_string()),
            args.clone(),
        )
        .unwrap();

        let state = State::load(&project_dir).unwrap().unwrap();
        assert_eq!(state.pid, 42);
        assert_eq!(state.ws_uri, "ws://127.0.0.1:1234/ws");
        assert_eq!(state.app_id.as_deref(), Some("app-123"));
        assert_eq!(state.cwd, project_dir.display().to_string());
        assert_eq!(state.args, args);

        State::remove(&project_dir).unwrap();
    }
}
