use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, Once};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use crate::config::schema::PythonConfig;
use super::expand_tilde;
use std::fs;

#[derive(serde::Deserialize)]
struct ThemeDescConfig {
    #[serde(rename = "theme")]
    themes: HashMap<String, ThemeDescEntry>,
}

#[derive(serde::Deserialize)]
struct ThemeDescEntry {
    name: String,
    path: String,
    description: Option<String>,
}

pub struct ThemeEngine {
    module: PyObject,
}

#[derive(Debug, Clone, Default)]
pub struct ThemeResult {
    pub lines_above: Vec<String>,
    pub input_prefix: String,
    pub right_prompt: String,
    pub colors: HashMap<String, String>,
    pub extra: HashMap<String, String>,
}

fn theme_desc_candidates() -> Vec<std::path::PathBuf> {
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

fn theme_registry() -> &'static Mutex<HashMap<String, String>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ensure_tdesc_loaded() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for path in theme_desc_candidates() {
            if path.exists()
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<ThemeDescConfig>(&content)
            {
                let mut registry = theme_registry().lock().unwrap_or_else(|e| e.into_inner());
                for (id, entry) in config.themes {
                    if let Some(desc) = entry.description {
                        eprintln!("ctx: theme '{}' — {}", entry.name, desc);
                    }
                    registry.insert(id, expand_tilde(&entry.path));
                    registry.insert(entry.name, expand_tilde(&entry.path));
                }
                eprintln!("ctx: loaded themes from t.desc: {}", path.display());
                return;
            }
        }
    });
}

fn resolve_theme(name: &str) -> Option<String> {
    ensure_tdesc_loaded();
    let registry = theme_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = registry.get(name) {
        return Some(path.clone());
    }
    let path = expand_tilde(name);
    if std::path::PathBuf::from(&path).exists() {
        return Some(path);
    }
    None
}

fn default_theme() -> Option<String> {
    ensure_tdesc_loaded();
    theme_registry().lock().unwrap_or_else(|e| e.into_inner()).values().next().cloned()
}

impl ThemeEngine {
    pub fn load(cfg: &PythonConfig) -> Option<Self> {
        let path = if cfg.theme.is_empty() {
            default_theme()?
        } else {
            resolve_theme(&cfg.theme)?
        };
        let std_path = std::path::PathBuf::from(&path);
        if !std_path.exists() {
            eprintln!("ctx: theme file not found: {}", path);
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
                eprintln!("ctx: loaded theme: {}", path);
                Some(engine)
            }
            Err(e) => {
                eprintln!("ctx: failed to load theme {}: {}", path, e);
                None
            }
        }
    }

    pub fn render_prompt(&self, context: &HashMap<String, String>) -> ThemeResult {
        let default = ThemeResult::default_prompt(context);
        let result: PyResult<ThemeResult> = Python::with_gil(|py| {
            let kwargs = PyDict::new(py);
            for (k, v) in context {
                kwargs.set_item(k.as_str(), v.as_str())?;
            }
            let val = self.module.call_method(py, "render_prompt", (), Some(&kwargs))?;
            parse_theme_result(py, &val)
        });
        result.unwrap_or(default)
    }

    pub fn render_right_prompt(&self, context: &HashMap<String, String>) -> String {
        let result: PyResult<String> = Python::with_gil(|py| {
            let kwargs = PyDict::new(py);
            for (k, v) in context {
                kwargs.set_item(k.as_str(), v.as_str())?;
            }
            let val = self.module.call_method(py, "render_right_prompt", (), Some(&kwargs))?;
            val.extract::<String>(py)
        });
        result.unwrap_or_default()
    }

    pub fn render_command_summary(&self, context: &HashMap<String, String>) -> String {
        let result: PyResult<String> = Python::with_gil(|py| {
            let kwargs = PyDict::new(py);
            for (k, v) in context {
                kwargs.set_item(k.as_str(), v.as_str())?;
            }
            let val = self.module.call_method(py, "render_command_summary", (), Some(&kwargs))?;
            val.extract::<String>(py)
        });
        result.unwrap_or_default()
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

fn parse_theme_result(py: Python, val: &PyObject) -> PyResult<ThemeResult> {
    let mut res = ThemeResult::default();
    let any = val.bind(py);
    if let Ok(dict) = any.downcast::<PyDict>() {
        if let Ok(Some(v)) = dict.get_item("lines_above")
            && let Ok(list) = v.downcast::<pyo3::types::PyList>() {
                res.lines_above = list.iter().filter_map(|x| x.extract().ok()).collect();
            }
        if let Ok(Some(v)) = dict.get_item("input_prefix") { res.input_prefix = v.extract().unwrap_or_default(); }
        if let Ok(Some(v)) = dict.get_item("right_prompt") { res.right_prompt = v.extract().unwrap_or_default(); }
        if let Ok(Some(c)) = dict.get_item("colors")
            && let Ok(cd) = c.downcast::<PyDict>() {
                for item in cd.iter() {
                    if let (Ok(key), Ok(val)) = (item.0.extract::<String>(), item.1.extract::<String>()) {
                        res.colors.insert(key, val);
                    }
                }
            }
        for item in dict.iter() {
            if let (Ok(key), Ok(val)) = (item.0.extract::<String>(), item.1.extract::<String>())
                && key != "lines_above" && key != "input_prefix" && key != "right_prompt" && key != "colors" {
                    res.extra.insert(key, val);
                }
        }
    } else if let Ok(s) = any.extract::<String>() {
        res.lines_above = vec![s];
    }
    Ok(res)
}

impl ThemeResult {
    fn default_prompt(context: &HashMap<String, String>) -> Self {
        let cwd = context.get("cwd").map(|s| s.as_str()).unwrap_or("~");
        Self {
            lines_above: vec![],
            input_prefix: format!("{} ❯ ", cwd),
            right_prompt: String::new(),
            colors: HashMap::new(),
            extra: HashMap::new(),
        }
    }
}
