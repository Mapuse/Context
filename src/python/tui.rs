use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, Once};
use pyo3::prelude::*;
use crate::config::schema::PythonConfig;
use super::expand_tilde;
use std::fs;

#[derive(serde::Deserialize)]
struct TuiDescConfig {
    #[serde(rename = "tui")]
    tuis: HashMap<String, TuiDescEntry>,
}

#[derive(serde::Deserialize)]
struct TuiDescEntry {
    name: String,
    path: String,
    description: Option<String>,
}

pub struct TuiEngine {
    module: PyObject,
}

fn tui_desc_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(std::path::PathBuf::from(home).join(".config/context/t.desc"));
    }
    candidates.push(std::path::PathBuf::from("/etc/context/t.desc"));
    candidates.push(std::path::PathBuf::from("./t.desc"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("t.desc"));
    }
    candidates
}

fn tui_registry() -> &'static Mutex<HashMap<String, String>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ensure_tui_desc_loaded() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for path in tui_desc_candidates() {
            if path.exists()
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<TuiDescConfig>(&content)
            {
                let mut registry = tui_registry().lock().unwrap_or_else(|e| e.into_inner());
                for (id, entry) in config.tuis {
                    if let Some(desc) = entry.description {
                        eprintln!("ctx: tui '{}' — {}", entry.name, desc);
                    }
                    registry.insert(id, expand_tilde(&entry.path));
                    registry.insert(entry.name, expand_tilde(&entry.path));
                }
                eprintln!("ctx: loaded tuis from t.desc: {}", path.display());
                return;
            }
        }
    });
}

fn resolve_tui(name: &str) -> Option<String> {
    ensure_tui_desc_loaded();
    let registry = tui_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = registry.get(name) {
        return Some(path.clone());
    }
    let path = expand_tilde(name);
    if std::path::PathBuf::from(&path).exists() {
        return Some(path);
    }
    None
}

fn default_tui() -> Option<String> {
    ensure_tui_desc_loaded();
    tui_registry().lock().unwrap_or_else(|e| e.into_inner()).values().next().cloned()
}

impl TuiEngine {
    pub fn load(cfg: &PythonConfig) -> Option<Self> {
        let path = if cfg.tui.is_empty() {
            default_tui()?
        } else {
            resolve_tui(&cfg.tui)?
        };
        let std_path = std::path::PathBuf::from(&path);
        if !std_path.exists() {
            eprintln!("ctx: tui file not found: {}", path);
            return None;
        }
        let parent = std_path.parent()?;
        let file_stem = std_path.file_stem()?.to_str()?;
        let parent_str = parent.to_str()?.to_string();
        let file_stem = file_stem.to_string();
        let result: PyResult<Self> = Python::with_gil(|py| {
            let sys = py.import("sys")?;
            sys.getattr("path")?.call_method1("insert", (0, &parent_str))?;
            let module = py.import(&file_stem)?.into();
            Ok(Self { module })
        });
        match result {
            Ok(engine) => {
                eprintln!("ctx: loaded tui: {}", path);
                Some(engine)
            }
            Err(e) => {
                eprintln!("ctx: failed to load tui {}: {}", path, e);
                None
            }
        }
    }

    pub fn has_run(&self) -> bool {
        Python::with_gil(|py| {
            self.module.bind(py).hasattr("run").unwrap_or(false)
        })
    }

    pub fn run(&self) -> bool {
        Python::with_gil(|py| {
            match self.module.call_method0(py, "run") {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("ctx: python TUI exited: {}", e);
                    false
                }
            }
        })
    }
}
