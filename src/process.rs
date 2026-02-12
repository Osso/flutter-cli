use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::config::Config;
use crate::state::State;
use crate::vm_service::{self, VmServiceConnection};

/// Ensure a connection to the Flutter app's VM Service.
/// If --url is provided, connect directly. Otherwise, use process management.
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

async fn start_flutter_run(project_dir: &Path) -> Result<VmServiceConnection> {
    let config = Config::load(project_dir)?;
    let args = config.flutter_run_args();

    eprintln!("Starting: flutter {}", args.join(" "));

    let stderr_path = stderr_log_path(project_dir);
    std::fs::create_dir_all(stderr_path.parent().unwrap())?;
    let stderr_file = std::fs::File::create(&stderr_path)?;

    let mut child = Command::new("flutter")
        .args(&args)
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr_file)
        .spawn()
        .map_err(|e| anyhow!("Failed to start flutter: {e}"))?;

    let pid = child.id();
    let stdout = child.stdout.take().unwrap();

    // Parse stdout for app.debugPort event (blocking in a thread)
    let project_dir_owned = project_dir.to_path_buf();
    let args_clone = args.clone();
    let result = tokio::task::spawn_blocking(move || {
        parse_flutter_machine_output(stdout, pid, &project_dir_owned, &args_clone)
    })
    .await??;

    let (ws_uri, app_id) = result;

    // Save state
    let state = State {
        pid,
        ws_uri: ws_uri.clone(),
        app_id,
        cwd: project_dir.to_string_lossy().to_string(),
        args,
        started_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    state.save(project_dir)?;

    let conn = VmServiceConnection::connect(&ws_uri).await?;
    Ok(conn)
}

fn parse_flutter_machine_output(
    stdout: std::process::ChildStdout,
    pid: u32,
    project_dir: &Path,
    _args: &[String],
) -> Result<(String, Option<String>)> {
    let reader = BufReader::new(stdout);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);

    let mut ws_uri: Option<String> = None;
    let mut app_id: Option<String> = None;

    for line in reader.lines() {
        if std::time::Instant::now() > deadline {
            kill_process(pid);
            State::remove(project_dir).ok();
            return Err(anyhow!("Timeout waiting for flutter run to start (120s)"));
        }

        let line = match line {
            Ok(l) => l,
            Err(e) => {
                return Err(anyhow!("Error reading flutter stdout: {e}"));
            }
        };

        // flutter run --machine outputs JSON events, one per line
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        // Look for array-wrapped events: [{"event":"...", "params":{...}}]
        let event = if event.is_array() {
            match event.as_array().and_then(|a| a.first()) {
                Some(e) => e.clone(),
                None => continue,
            }
        } else {
            event
        };

        let event_name = event.get("event").and_then(|e| e.as_str());

        match event_name {
            Some("app.debugPort") => {
                if let Some(params) = event.get("params") {
                    if let Some(uri) = params.get("wsUri").and_then(|u| u.as_str()) {
                        ws_uri = Some(uri.to_string());
                    }
                    if let Some(id) = params.get("appId").and_then(|a| a.as_str()) {
                        app_id = Some(id.to_string());
                    }
                }
            }
            Some("app.started") => {
                // App is ready, we should have wsUri by now
                if ws_uri.is_some() {
                    break;
                }
            }
            Some("app.stop") | Some("daemon.shutdown") => {
                State::remove(project_dir).ok();
                return Err(anyhow!("Flutter app exited during startup"));
            }
            _ => {}
        }

        // If we have the wsUri, we can connect even before app.started
        if ws_uri.is_some() {
            break;
        }
    }

    match ws_uri {
        Some(uri) => Ok((uri, app_id)),
        None => {
            kill_process(pid);
            State::remove(project_dir).ok();
            Err(anyhow!(
                "flutter run exited without providing VM Service URI. Check {}",
                stderr_log_path(project_dir).display()
            ))
        }
    }
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
