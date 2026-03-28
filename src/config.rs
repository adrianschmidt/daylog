use color_eyre::eyre::{Result, WrapErr};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub notes_dir: String,
    #[serde(default = "default_db_path")]
    pub db_path: Option<String>,
    #[serde(default = "default_refresh_secs")]
    pub refresh_secs: u64,
    #[serde(default)]
    pub modules: ModulesConfig,
    #[serde(default)]
    pub exercises: HashMap<String, ExerciseConfig>,
    #[serde(default)]
    pub metrics: HashMap<String, MetricConfig>,
    #[serde(default = "default_toml_table")]
    pub climbing: toml::Value,
}

fn default_db_path() -> Option<String> {
    None
}

fn default_toml_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

fn default_refresh_secs() -> u64 {
    15
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModulesConfig {
    #[serde(default = "default_true")]
    pub dashboard: bool,
    #[serde(default = "default_true")]
    pub training: bool,
    #[serde(default = "default_true")]
    pub trends: bool,
    #[serde(default)]
    pub climbing: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExerciseConfig {
    pub display: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "white".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricConfig {
    pub display: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub unit: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            color_eyre::eyre::bail!(
                "Config not found at {}. Run `daylog init` to create one.",
                path.display()
            );
        }
        let contents = std::fs::read_to_string(&path)
            .wrap_err_with(|| format!("Failed to read config at {}", path.display()))?;
        let config: Config = toml::from_str(&contents)
            .wrap_err_with(|| format!("Failed to parse config at {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_or_keep(current: &Config) -> Config {
        match Self::load() {
            Ok(new_config) => new_config,
            Err(e) => {
                eprintln!("Warning: config reload failed: {e}. Keeping current config.");
                current.clone()
            }
        }
    }

    fn validate(&self) -> Result<()> {
        let notes = self.notes_dir_path();
        if !notes.exists() {
            color_eyre::eyre::bail!(
                "Notes directory does not exist: {}. Check notes_dir in your config or run `daylog init`.",
                notes.display()
            );
        }
        if !notes.is_dir() {
            color_eyre::eyre::bail!(
                "notes_dir points to a file, not a directory: {}. Check your config.",
                notes.display()
            );
        }
        Ok(())
    }

    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?
            .join("daylog");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn notes_dir_path(&self) -> PathBuf {
        expand_tilde(&self.notes_dir)
    }

    pub fn db_path(&self) -> PathBuf {
        match &self.db_path {
            Some(p) => expand_tilde(p),
            None => self.notes_dir_path().join(".daylog.db"),
        }
    }

    pub fn module_config(&self, id: &str) -> Option<&toml::Value> {
        if id == "climbing" {
            if self.climbing.is_table() {
                Some(&self.climbing)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        match id {
            "dashboard" => self.modules.dashboard,
            "training" => self.modules.training,
            "trends" => self.modules.trends,
            "climbing" => self.modules.climbing,
            _ => false,
        }
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

pub fn default_config_contents() -> &'static str {
    include_str!("../presets/default.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/notes");
        assert!(expanded.to_str().unwrap().contains("notes"));
        assert!(!expanded.to_str().unwrap().starts_with("~"));
    }

    #[test]
    fn test_parse_default_config() {
        let config: Config = toml::from_str(default_config_contents()).unwrap();
        assert!(config.modules.dashboard);
        assert!(config.modules.training);
        assert!(config.modules.trends);
        assert!(!config.modules.climbing);
    }
}
