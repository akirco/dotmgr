use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const APP_NAME: &str = "dotmgr";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// 同步目标目录，文件结构镜像 $HOME
    pub sync_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self {
            sync_dir: home.join(".dotfiles"),
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
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_default()
    } else {
        let cfg = AppConfig::default();
        save_config(&cfg);
        cfg
    }
}

pub fn save_config(cfg: &AppConfig) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(content) = toml::to_string_pretty(cfg) {
        let _ = fs::write(dir.join("config.toml"), content);
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
            .unwrap_or_default()
    } else {
        vec![]
    }
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
