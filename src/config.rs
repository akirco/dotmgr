use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const APP_NAME: &str = "dotmgr";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub sync_dir: PathBuf,
    #[serde(default)]
    pub synced_ignores: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self {
            sync_dir: home.join(".dotfiles"),
            synced_ignores: vec![
                ".git".to_string(),
                ".github".to_string(),
                "README.md".to_string(),
                "LICENSE".to_string(),
                "Makefile".to_string(),
                ".gitignore".to_string(),
                ".gitattributes".to_string(),
            ],
        }
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(APP_NAME)
}

pub fn load_config() -> AppConfig {
    let path = config_dir().join("config.toml");
    let default_cfg = AppConfig::default();
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_default();
        let mut cfg: AppConfig = toml::from_str(&content).unwrap_or_default();
        if cfg.sync_dir.as_os_str().is_empty() {
            cfg.sync_dir = default_cfg.sync_dir;
        }
        if cfg.synced_ignores.is_empty() {
            cfg.synced_ignores = default_cfg.synced_ignores;
        }
        cfg
    } else {
        default_cfg
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct IgnoresFile {
    ignored: Vec<String>,
}

pub fn load_ignores() -> Vec<String> {
    let path = config_dir().join("ignores.toml");
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_default();
        toml::from_str::<IgnoresFile>(&content)
            .map(|f| f.ignored)
            .unwrap_or_else(|_| default_ignores())
    } else {
        let ignores = default_ignores();
        save_ignores(&ignores);
        ignores
    }
}

fn default_ignores() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        "target".to_string(),
        "__pycache__".to_string(),
        ".venv".to_string(),
        "venv".to_string(),
        ".DS_Store".to_string(),
        "Thumbs.db".to_string(),
    ]
}

pub fn save_ignores(ignored: &[String]) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let file = IgnoresFile {
        ignored: ignored.to_vec(),
    };
    if let Ok(content) = toml::to_string_pretty(&file) {
        let _ = fs::write(dir.join("ignores.toml"), content);
    }
}
