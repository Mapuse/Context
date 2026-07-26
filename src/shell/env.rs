use std::collections::HashMap;

const INTERNAL_PREFIXES: &[&str] = &[
    "_SHOPT_", "_GETOPT_", "_OPT_", "_LOADED_MODULE",
];

fn is_internal(key: &str) -> bool {
    INTERNAL_PREFIXES.iter().any(|p| key.starts_with(p))
}

#[derive(Debug, Clone)]
pub struct Env {
    vars: HashMap<String, String>,
    exported: HashMap<String, bool>,
    aliases: HashMap<String, String>,
    traps: HashMap<String, String>,
    readonly: HashMap<String, bool>,
    named_dirs: HashMap<String, String>,
    assoc_arrays: HashMap<String, HashMap<String, String>>,
    positional: Vec<String>,
    scope_stack: Vec<HashMap<String, String>>,
}

impl Env {
    pub fn new() -> Self {
        let mut vars = HashMap::new();
        let mut exported = HashMap::new();
        for (k, v) in std::env::vars() {
            exported.insert(k.clone(), true);
            vars.insert(k, v);
        }
        let shlvl: u32 = vars.get("SHLVL")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0) + 1;
        vars.insert("SHLVL".to_string(), shlvl.to_string());
        std::env::set_var("SHLVL", shlvl.to_string());
        Self {
            vars,
            exported,
            aliases: HashMap::new(),
            traps: HashMap::new(),
            readonly: HashMap::new(),
            named_dirs: HashMap::new(),
            assoc_arrays: HashMap::new(),
            positional: Vec::new(),
            scope_stack: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        match key {
            "SHELL" => Some("context"),
            _ => {

                for scope in self.scope_stack.iter().rev() {
                    if let Some(val) = scope.get(key) {
                        return Some(val);
                    }
                }
                self.vars.get(key).map(|s| s.as_str())
            }
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        if self.is_readonly(key) {
            return;
        }
        self.vars.insert(key.to_string(), value.to_string());
        if !is_internal(key) {
            std::env::set_var(key, value);
        }
    }

    pub fn set_exported(&mut self, key: &str, value: &str, export: bool) {
        self.vars.insert(key.to_string(), value.to_string());
        self.exported.insert(key.to_string(), export);
        if !is_internal(key) {
            std::env::set_var(key, value);
        }
    }

    pub fn export(&mut self, key: &str) {
        self.exported.insert(key.to_string(), true);
        if !is_internal(key) {
            if let Some(val) = self.vars.get(key).cloned() {
                std::env::set_var(key, &val);
            }
        }
    }

    pub fn unset(&mut self, key: &str) {
        if self.is_readonly(key) {
            eprintln!("context: unset: {}: readonly variable", key);
            return;
        }
        self.vars.remove(key);
        self.exported.remove(key);
        if !is_internal(key) {
            std::env::remove_var(key);
        }
    }

    pub fn set_readonly(&mut self, key: &str) {
        self.readonly.insert(key.to_string(), true);
        self.export(key);
    }

    pub fn is_readonly(&self, key: &str) -> bool {
        self.readonly.get(key).copied().unwrap_or(false)
    }

    pub fn expand_special(&mut self, var: &str) -> String {
        match var {
            "$" | "PID" => std::process::id().to_string(),
            "PPID" => {
                std::fs::read_to_string("/proc/self/stat")
                    .ok()
                    .and_then(|s| {
                        let fields: Vec<&str> = s.split_whitespace().collect();
                        if fields.len() > 3 {
                            Some(fields[3].to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "0".into())
            }
            "UID" => unsafe { libc::getuid() }.to_string(),
            "EUID" => unsafe { libc::geteuid() }.to_string(),
            "GID" => unsafe { libc::getgid() }.to_string(),
            "HOME" => dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".into()),
            "USER" => self.get("USER").unwrap_or("user").to_string(),
            "HOSTNAME" => self.get_hostname(),
            "SHLVL" => self.vars.get("SHLVL").cloned().unwrap_or_else(|| "1".into()),
            _ => self.get(var).unwrap_or("").to_string(),
        }
    }

    fn get_hostname(&self) -> String {
        if let Some(h) = self.get("HOSTNAME") {
            return h.to_string();
        }
        std::fs::read_to_string("/etc/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "localhost".into())
    }

    pub fn set_alias(&mut self, name: &str, value: &str) {
        self.aliases.insert(name.to_string(), value.to_string());
    }

    pub fn get_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }

    pub fn unset_alias(&mut self, name: &str) {
        self.aliases.remove(name);
    }

    pub fn all_aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    pub fn merge_from(&mut self, other: &Env) {
        for (k, v) in &other.vars {
            if !is_internal(k) {
                self.vars.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &other.aliases {
            self.aliases.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.named_dirs {
            self.named_dirs.insert(k.clone(), v.clone());
        }
    }

    pub fn positional(&self) -> &[String] {
        &self.positional
    }

    pub fn set_positional(&mut self, positional: Vec<String>) {
        self.positional = positional;
    }

    pub fn set_trap(&mut self, signal: &str, command: &str) {
        self.traps.insert(signal.to_uppercase(), command.to_string());
    }

    pub fn get_trap(&self, signal: &str) -> Option<&str> {
        self.traps.get(&signal.to_uppercase()).map(|s| s.as_str())
    }

    pub fn remove_trap(&mut self, signal: &str) {
        self.traps.remove(&signal.to_uppercase());
    }

    pub fn unset_all_traps(&mut self) {
        self.traps.clear();
    }

    pub fn all_traps(&self) -> &HashMap<String, String> {
        &self.traps
    }

    pub fn all_vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    pub fn passthrough_env(&self, passthrough: &[String], filter: &[String]) -> Vec<(String, String)> {
        self.vars.iter()
            .filter(|(k, _)| {
                if filter.iter().any(|f| f == k.as_str()) {
                    return false;
                }
                if passthrough.is_empty() {
                    return self.exported.get(k.as_str()).copied().unwrap_or(false);
                }
                passthrough.iter().any(|p| p == k.as_str()) || self.exported.get(k.as_str()).copied().unwrap_or(false)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn user(&self) -> String {
        self.get("USER").unwrap_or("user").to_string()
    }

    pub fn hostname(&self) -> String {
        self.get_hostname()
    }

    pub fn home(&self) -> String {
        self.get("HOME")
            .map(|s| s.to_string())
            .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
            .unwrap_or_else(|| "/".into())
    }

    pub fn set_named_dir(&mut self, name: &str, path: &str) {
        self.named_dirs.insert(name.to_string(), path.to_string());
    }

    pub fn get_named_dir(&self, name: &str) -> Option<&str> {
        self.named_dirs.get(name).map(|s| s.as_str())
    }

    #[allow(dead_code)]
    pub fn unset_named_dir(&mut self, name: &str) {
        self.named_dirs.remove(name);
    }

    #[allow(dead_code)]
    pub fn all_named_dirs(&self) -> &HashMap<String, String> {
        &self.named_dirs
    }

    pub fn create_assoc_array(&mut self, name: &str) {
        if !self.assoc_arrays.contains_key(name) {
            self.assoc_arrays.insert(name.to_string(), HashMap::new());
        }
    }

    pub fn is_assoc_array(&self, name: &str) -> bool {
        self.assoc_arrays.contains_key(name)
    }

    pub fn assoc_set(&mut self, name: &str, key: &str, value: &str) {
        if let Some(map) = self.assoc_arrays.get_mut(name) {
            map.insert(key.to_string(), value.to_string());
        }
    }

    pub fn assoc_get(&self, name: &str, key: &str) -> Option<&str> {
        self.assoc_arrays.get(name)?.get(key).map(|s| s.as_str())
    }

    pub fn assoc_unset(&mut self, name: &str, key: &str) -> bool {
        if let Some(map) = self.assoc_arrays.get_mut(name) {
            map.remove(key).is_some()
        } else {
            false
        }
    }

    pub fn assoc_keys(&self, name: &str) -> Vec<String> {
        self.assoc_arrays.get(name)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn assoc_values(&self, name: &str) -> Vec<String> {
        self.assoc_arrays.get(name)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn assoc_pairs(&self, name: &str) -> Vec<(String, String)> {
        self.assoc_arrays.get(name)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    pub fn assoc_len(&self, name: &str) -> usize {
        self.assoc_arrays.get(name).map(|m| m.len()).unwrap_or(0)
    }

    pub fn push_scope(&mut self) -> usize {
        self.scope_stack.push(HashMap::new());
        self.scope_stack.len()
    }

    pub fn pop_scope(&mut self, _saved: usize) {
        self.scope_stack.pop();
    }

    pub fn set_local(&mut self, key: &str, value: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(key.to_string(), value.to_string());
        } else {
            self.vars.insert(key.to_string(), value.to_string());
        }
    }

    pub fn clear_inherited(&mut self, keep_keys: &[String]) {
        let inherited_keys: Vec<String> = self.vars.keys()
            .filter(|k| !keep_keys.iter().any(|kk| kk == k.as_str()))
            .cloned()
            .collect();
        for key in inherited_keys {
            self.vars.remove(&key);
            self.exported.remove(&key);
            if !is_internal(&key) {
                std::env::remove_var(&key);
            }
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
