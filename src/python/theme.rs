use std::collections::HashMap;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use crate::config::schema::PythonConfig;
use super::expand_tilde;

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

impl ThemeEngine {
    pub fn load(cfg: &PythonConfig) -> Option<Self> {
        if cfg.theme.is_empty() { return None; }
        let path = expand_tilde(&cfg.theme);
        let std_path = std::path::PathBuf::from(&path);
        if !std_path.exists() {
            eprintln!("context: theme file not found: {}", path);
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
                eprintln!("context: loaded theme: {}", path);
                Some(engine)
            }
            Err(e) => {
                eprintln!("context: failed to load theme {}: {}", path, e);
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
                    eprintln!("context: python TUI exited: {}", e);
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
        if let Ok(Some(v)) = dict.get_item("lines_above") {
            if let Ok(list) = v.downcast::<pyo3::types::PyList>() {
                res.lines_above = list.iter().filter_map(|x| x.extract().ok()).collect();
            }
        }
        if let Ok(Some(v)) = dict.get_item("input_prefix") { res.input_prefix = v.extract().unwrap_or_default(); }
        if let Ok(Some(v)) = dict.get_item("right_prompt") { res.right_prompt = v.extract().unwrap_or_default(); }
        if let Ok(Some(c)) = dict.get_item("colors") {
            if let Ok(cd) = c.downcast::<PyDict>() {
                for item in cd.iter() {
                    if let (Ok(key), Ok(val)) = (item.0.extract::<String>(), item.1.extract::<String>()) {
                        res.colors.insert(key, val);
                    }
                }
            }
        }
        for item in dict.iter() {
            if let (Ok(key), Ok(val)) = (item.0.extract::<String>(), item.1.extract::<String>()) {
                if key != "lines_above" && key != "input_prefix" && key != "right_prompt" && key != "colors" {
                    res.extra.insert(key, val);
                }
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
