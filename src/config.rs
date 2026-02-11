use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

const CONFIG_FILENAME: &str = ".flutter-cli.toml";

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub flavor: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub dart_define_from_file: Option<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

impl Config {
    /// Load config from `.flutter-cli.toml` in the given directory.
    /// Returns default config if file doesn't exist.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let config_path = project_dir.join(CONFIG_FILENAME);
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Build the arguments for `flutter run --machine`.
    pub fn flutter_run_args(&self) -> Vec<String> {
        let mut args = vec!["run".to_string(), "--machine".to_string()];

        if let Some(ref flavor) = self.flavor {
            args.push("--flavor".to_string());
            args.push(flavor.clone());
        }

        if let Some(ref target) = self.target {
            args.push("--target".to_string());
            args.push(target.clone());
        }

        if let Some(ref dart_define) = self.dart_define_from_file {
            args.push(format!("--dart-define-from-file={dart_define}"));
        }

        if let Some(ref device) = self.device {
            if device != "auto" {
                args.push("--device-id".to_string());
                args.push(device.clone());
            }
        }

        args.extend(self.extra_args.iter().cloned());

        args
    }
}
