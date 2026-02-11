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

fn state_file_path(project_dir: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(project_dir.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    let hex = format!("{:x}", hash);
    let short = &hex[..16];
    PathBuf::from(STATE_DIR).join(format!("{short}.json"))
}
