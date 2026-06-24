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

        if let Some(ref device) = self.device
            && device != "auto"
        {
            args.push("--device-id".to_string());
            args.push(device.clone());
        }

        args.extend(self.extra_args.iter().cloned());

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flutter-cli-config-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn load_returns_default_when_config_is_missing() {
        let project_dir = temp_project_dir("missing");

        let config = Config::load(&project_dir).unwrap();

        assert_eq!(config.flutter_run_args(), vec!["run", "--machine"]);
    }

    #[test]
    fn load_reads_flutter_cli_toml() {
        let project_dir = temp_project_dir("toml");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join(CONFIG_FILENAME),
            r#"
device = "pixel"
flavor = "dev"
target = "lib/main_dev.dart"
dart_define_from_file = "env/dev.json"
extra_args = ["--verbose", "--hot"]
"#,
        )
        .unwrap();

        let config = Config::load(&project_dir).unwrap();

        assert_eq!(config.device.as_deref(), Some("pixel"));
        assert_eq!(config.flavor.as_deref(), Some("dev"));
        assert_eq!(config.target.as_deref(), Some("lib/main_dev.dart"));
        assert_eq!(
            config.dart_define_from_file.as_deref(),
            Some("env/dev.json")
        );
        assert_eq!(config.extra_args, vec!["--verbose", "--hot"]);

        std::fs::remove_dir_all(project_dir).ok();
    }

    #[test]
    fn flutter_run_args_preserve_expected_order() {
        let config = Config {
            device: Some("emulator-5554".to_string()),
            flavor: Some("staging".to_string()),
            target: Some("lib/main_staging.dart".to_string()),
            dart_define_from_file: Some("env/staging.json".to_string()),
            extra_args: vec!["--dart-define=feature=true".to_string()],
        };

        assert_eq!(
            config.flutter_run_args(),
            vec![
                "run",
                "--machine",
                "--flavor",
                "staging",
                "--target",
                "lib/main_staging.dart",
                "--dart-define-from-file=env/staging.json",
                "--device-id",
                "emulator-5554",
                "--dart-define=feature=true",
            ]
        );
    }

    #[test]
    fn flutter_run_args_omits_auto_device() {
        let config = Config {
            device: Some("auto".to_string()),
            ..Config::default()
        };

        assert_eq!(config.flutter_run_args(), vec!["run", "--machine"]);
    }
}
