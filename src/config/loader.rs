use std::fs;
use std::path::{Path, PathBuf};

use super::schema::Config;

const CONFIG_DIRS: &[&str] = &[".config/context", "context", ".context"];
const CONFIG_FILE_NAMES: &[&str] = &["c.toml", "context.toml", ".ctxrc.toml"];

pub fn config_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        for d in CONFIG_DIRS {
            let p = home.join(d);
            if p.is_dir() {
                return p;
            }
        }
        let fallback = home.join(".config/context");
        let _ = fs::create_dir_all(&fallback);
        return fallback;
    }
    PathBuf::from(".")
}

pub fn config_path() -> PathBuf {
    let dir = config_dir();
    for name in CONFIG_FILE_NAMES {
        let p = dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    dir.join("c.toml")
}

pub fn rc_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home.join(".ctxrc");
    }
    PathBuf::from(".ctxrc")
}

pub fn history_path(cfg: &Config) -> PathBuf {
    let raw = &cfg.history.file;
    if (raw.starts_with("~/") || raw.starts_with("~\\"))
        && let Some(home) = dirs::home_dir() {
            return home.join(&raw[2..]);
        }
    if Path::new(raw).is_absolute() {
        return PathBuf::from(raw);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(raw);
    }
    PathBuf::from(raw)
}

pub fn load() -> Config {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("context: error parsing {}: {}", path.display(), e);
                Config::default()
            }
        },
        Err(_) => {
            let cfg = Config::default();
            let _ = save(&cfg);
            cfg
        }
    }
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("c.toml");
    let toml = toml::to_string_pretty(cfg).unwrap_or_default();
    fs::write(path, toml)
}
