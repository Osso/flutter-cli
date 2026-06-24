use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const STATE_DIR: &str = "/tmp/claude/flutter-cli";

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    pub pid: u32,
    pub ws_uri: String,
    pub app_id: Option<String>,
    pub cwd: String,
    pub args: Vec<String>,
    pub started_at: u64,
}

impl State {
    pub fn load(project_dir: &Path) -> Result<Option<Self>> {
        let path = state_file_path(project_dir);
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path)?;
        let state: State = serde_json::from_str(&contents)?;
        Ok(Some(state))
    }

    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = state_file_path(project_dir);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    pub fn remove(project_dir: &Path) -> Result<()> {
        let path = state_file_path(project_dir);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Check if the PID in the state file is still alive.
    pub fn is_pid_alive(&self) -> bool {
        unsafe { libc::kill(self.pid as i32, 0) == 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flutter-cli-state-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn state_for(project_dir: &Path) -> State {
        State {
            pid: std::process::id(),
            ws_uri: "ws://127.0.0.1:1234/ws".to_string(),
            app_id: Some("app-1".to_string()),
            cwd: project_dir.display().to_string(),
            args: vec!["run".to_string(), "--machine".to_string()],
            started_at: 123,
        }
    }

    #[test]
    fn load_returns_none_when_state_file_is_missing() {
        let project_dir = temp_project_dir("missing");

        assert!(State::load(&project_dir).unwrap().is_none());
    }

    #[test]
    fn save_load_remove_roundtrip() {
        let project_dir = temp_project_dir("roundtrip");
        let state = state_for(&project_dir);

        state.save(&project_dir).unwrap();
        let loaded = State::load(&project_dir).unwrap().unwrap();

        assert_eq!(loaded.pid, state.pid);
        assert_eq!(loaded.ws_uri, state.ws_uri);
        assert_eq!(loaded.app_id, state.app_id);
        assert_eq!(loaded.cwd, state.cwd);
        assert_eq!(loaded.args, state.args);
        assert_eq!(loaded.started_at, state.started_at);

        State::remove(&project_dir).unwrap();
        assert!(State::load(&project_dir).unwrap().is_none());
    }

    #[test]
    fn state_file_path_is_stable_and_hashed() {
        let project_dir = PathBuf::from("/tmp/flutter-cli/example");

        let first = state_file_path(&project_dir);
        let second = state_file_path(&project_dir);

        assert_eq!(first, second);
        assert_eq!(first.parent().unwrap(), Path::new(STATE_DIR));
        assert_eq!(first.extension().and_then(|ext| ext.to_str()), Some("json"));
    }

    #[test]
    fn is_pid_alive_detects_current_process() {
        let project_dir = temp_project_dir("pid");
        let state = state_for(&project_dir);

        assert!(state.is_pid_alive());
    }
}

fn state_file_path(project_dir: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(project_dir.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    let hex = format!("{:x}", hash);
    let short = &hex[..16];
    PathBuf::from(STATE_DIR).join(format!("{short}.json"))
}
