use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static SHELL_START_TIME: AtomicU64 = AtomicU64::new(0);

const INTERNAL_PREFIXES: &[&str] = &[
    "_SHOPT_",
    "_GETOPT_",
    "_OPT_",
    "_LOADED_MODULE",
    "_SECONDS_RESET",
];

fn is_internal(key: &str) -> bool {
    INTERNAL_PREFIXES.iter().any(|p| key.starts_with(p))
}

#[derive(Debug, Clone, Default)]
pub struct VarAttrs {
    pub readonly: bool,
    pub exported: bool,
    pub integer: bool,
    pub lowercase: bool,
    pub uppercase: bool,
    pub nameref: Option<String>,
    pub trace: bool,
}

fn is_valid_var_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    match bytes[0] {
        b'a'..=b'z' | b'A'..=b'Z' | b'_' => {}
        _ => return false,
    }
    bytes[1..]
        .iter()
        .all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
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
    attrs: HashMap<String, VarAttrs>,
}

impl Env {
    pub fn new() -> Self {
        let mut vars = HashMap::new();
        let mut exported = HashMap::new();
        for (k, v) in std::env::vars() {
            exported.insert(k.clone(), true);
            vars.insert(k, v);
        }
        let shlvl: u32 = vars.get("SHLVL").and_then(|s| s.parse().ok()).unwrap_or(0) + 1;
        vars.insert("SHLVL".to_string(), shlvl.to_string());
        unsafe {
            std::env::set_var("SHLVL", shlvl.to_string());
        }
        if !vars.contains_key("SHELL") {
            vars.insert("SHELL".to_string(), "context".to_string());
        }
        // `$0` — the shell/invocation name, kept apart from the positionals.
        vars.entry("0".to_string())
            .or_insert_with(|| std::env::args().next().unwrap_or_else(|| "ctx".to_string()));
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        SHELL_START_TIME
            .compare_exchange(0, start, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        unsafe {
            libc::srand(start as u32);
        }
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
            attrs: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.get_resolved(key, 0)
    }

    fn get_resolved(&self, key: &str, depth: u8) -> Option<&str> {
        if depth < 32
            && let Some(attrs) = self.attrs.get(key)
            && let Some(ref target) = attrs.nameref
        {
            return self.get_resolved(target, depth + 1);
        }
        match key {
            "SHELL" => Some(
                self.vars
                    .get("SHELL")
                    .map(|s| s.as_str())
                    .unwrap_or("context"),
            ),
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

    pub fn set(&mut self, key: &str, value: &str) -> bool {
        self.set_resolved(key, value, 0)
    }

    fn set_resolved(&mut self, key: &str, value: &str, depth: u8) -> bool {
        if depth < 32
            && let Some(target) = self.attrs.get(key).and_then(|a| a.nameref.clone())
        {
            return self.set_resolved(&target, value, depth + 1);
        }
        let effective_value;
        let value = if let Some(attrs) = self.attrs.get(key) {
            if attrs.lowercase {
                effective_value = value.to_lowercase();
                &effective_value
            } else if attrs.uppercase {
                effective_value = value.to_uppercase();
                &effective_value
            } else {
                value
            }
        } else {
            value
        };
        if self.is_readonly(key) {
            eprintln!("context: {}: readonly variable", key);
            return false;
        }
        if !is_valid_var_name(key) {
            return false;
        }
        if key == "SECONDS" {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let val: u64 = value.parse().unwrap_or(0);
            self.vars.insert(
                "_SECONDS_RESET".to_string(),
                (now.saturating_sub(val)).to_string(),
            );
        }
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(key.to_string(), value.to_string());
        } else {
            self.vars.insert(key.to_string(), value.to_string());
        }
        // Only exported variables are visible to child processes; plain
        // shell variables must stay local.
        if !is_internal(key) && self.is_exported(key) {
            unsafe {
                std::env::set_var(key, value);
            }
        }
        true
    }

    pub fn set_dirstack(&mut self, stack: &[String]) {
        self.set("DIRSTACK", &stack.join("\n"));
        let count = stack.len();
        for (i, dir) in stack.iter().enumerate() {
            let mut key = String::with_capacity(12);
            key.push_str("DIRSTACK_");
            key.push_str(&i.to_string());
            self.set(&key, dir);
        }
        let mut i = count;
        loop {
            let mut key = String::with_capacity(12);
            key.push_str("DIRSTACK_");
            key.push_str(&i.to_string());
            if self.vars.remove(&key).is_some() {
                unsafe {
                    std::env::remove_var(&key);
                }
            } else {
                break;
            }
            i += 1;
        }
    }

    pub fn set_exported(&mut self, key: &str, value: &str, export: bool) {
        self.vars.insert(key.to_string(), value.to_string());
        self.exported.insert(key.to_string(), export);
        if !is_internal(key) && export {
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }

    pub fn export(&mut self, key: &str) {
        self.exported.insert(key.to_string(), true);
        if !is_internal(key)
            && let Some(val) = self.vars.get(key).cloned()
        {
            unsafe {
                std::env::set_var(key, &val);
            }
        }
    }

    pub fn unexport(&mut self, key: &str) {
        self.exported.insert(key.to_string(), false);
        if !is_internal(key) {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    pub fn is_exported(&self, key: &str) -> bool {
        self.exported.get(key).copied().unwrap_or(false)
    }

    pub fn unset(&mut self, key: &str) {
        if self.is_readonly(key) {
            eprintln!("context: unset: {}: readonly variable", key);
            return;
        }
        // Unsetting an array name removes its `{name}_N` elements and any
        // associated associative-array data as well.
        if self.is_array_name(key) {
            self.unset_whole_array(key);
            return;
        }
        for scope in &mut self.scope_stack {
            scope.remove(key);
        }
        self.vars.remove(key);
        self.exported.remove(key);
        if !is_internal(key) {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    /// True when `name` refers to array storage: an associative array or at
    /// least one indexed `{name}_N` element.
    pub fn is_array_name(&self, name: &str) -> bool {
        if self.assoc_arrays.contains_key(name) {
            return true;
        }
        let prefix = format!("{}_", name);
        let is_elem = |k: &String| {
            k.len() > prefix.len()
                && k.starts_with(prefix.as_str())
                && k[prefix.len()..].bytes().all(|b| b.is_ascii_digit())
        };
        self.vars.keys().any(&is_elem) || self.scope_stack.iter().any(|s| s.keys().any(&is_elem))
    }

    /// Remove every `{name}_N` element, the assoc-array table and the base
    /// key itself.
    pub fn unset_whole_array(&mut self, name: &str) {
        let prefix = format!("{}_", name);
        let is_elem = |k: &String| {
            k.len() > prefix.len()
                && k.starts_with(prefix.as_str())
                && k[prefix.len()..].bytes().all(|b| b.is_ascii_digit())
        };
        let removed: Vec<String> = self.vars.keys().filter(|k| is_elem(k)).cloned().collect();
        self.vars.retain(|k, _| !is_elem(k));
        self.vars.remove(name);
        self.exported.remove(name);
        for scope in &mut self.scope_stack {
            scope.retain(|k, _| !is_elem(k));
            scope.remove(name);
        }
        self.assoc_arrays.remove(name);
        for k in removed {
            if !is_internal(&k) {
                unsafe {
                    std::env::remove_var(&k);
                }
            }
        }
    }

    pub fn set_readonly(&mut self, key: &str) {
        self.readonly.insert(key.to_string(), true);
    }

    pub fn is_readonly(&self, key: &str) -> bool {
        self.readonly.contains_key(key)
    }

    pub fn expand_special(&mut self, var: &str) -> String {
        match var {
            "$" | "PID" => std::process::id().to_string(),
            "PPID" => {
                std::fs::read_to_string("/proc/self/stat")
                    .ok()
                    .and_then(|s| {
                        // Skip past "pid (comm)" — comm may contain spaces/parens.
                        let after_comm = s.rsplit_once(')').map(|(_, rest)| rest)?;
                        after_comm.split_whitespace().nth(1).map(|s| s.to_string())
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
            "SHLVL" => self
                .vars
                .get("SHLVL")
                .cloned()
                .unwrap_or_else(|| "1".into()),
            "RANDOM" => unsafe { libc::rand() % 32768 }.to_string(),
            "LINENO" => crate::shell::expand::CURRENT_LINE
                .load(Ordering::Relaxed)
                .to_string(),
            "SECONDS" => {
                if self.vars.contains_key("_SECONDS_RESET") {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let reset = self
                        .vars
                        .get("_SECONDS_RESET")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    (now.saturating_sub(reset)).to_string()
                } else if let Some(val) = self.vars.get("SECONDS") {
                    val.clone()
                } else {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let start = SHELL_START_TIME.load(Ordering::SeqCst);
                    (now.saturating_sub(start)).to_string()
                }
            }
            "PIPESTATUS" => self.get("PIPESTATUS").unwrap_or("0").to_string(),
            "FUNCNAME" => {
                let stack = crate::shell::builtin::CALL_STACK.lock().unwrap();
                stack.last().map(|f| f.name.clone()).unwrap_or_default()
            }
            "BASH_VERSION" => env!("CARGO_PKG_VERSION").to_string(),
            "HISTCMD" => self.get("HISTCMD").unwrap_or("0").to_string(),
            "BASH_SOURCE" => {
                let stack = crate::shell::executor::SOURCE_STACK.lock().unwrap();
                stack.last().cloned().unwrap_or_default()
            }
            "COPROC_PID" => self.get("COPROC_PID").unwrap_or("0").to_string(),
            "COPROC" => self.get("COPROC").unwrap_or("").to_string(),
            "BASH_ALIASES" => {
                let pairs: Vec<String> = self
                    .aliases
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                pairs.join(" ")
            }
            "BASH_CMDS" => {
                let cache = crate::shell::builtin::PATH_CACHE.lock().unwrap();
                let pairs: Vec<String> =
                    cache.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                pairs.join(" ")
            }
            var if var.starts_with("BASH_REMATCH[") => {
                let idx_str = var
                    .trim_start_matches("BASH_REMATCH[")
                    .trim_end_matches(']');
                let rematch = crate::shell::executor::BASH_REMATCH.lock().unwrap();
                if idx_str == "@" {
                    rematch.join(" ")
                } else if idx_str == "#" {
                    rematch.len().to_string()
                } else if let Ok(idx) = idx_str.parse::<usize>() {
                    rematch.get(idx).cloned().unwrap_or_default()
                } else {
                    String::new()
                }
            }
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
        for (k, v) in &other.attrs {
            self.attrs.insert(k.clone(), v.clone());
        }
    }

    pub fn positional(&self) -> &[String] {
        &self.positional
    }

    pub fn set_positional(&mut self, positional: Vec<String>) {
        self.positional = positional;
    }

    pub fn set_trap(&mut self, signal: &str, command: &str) {
        self.traps
            .insert(signal.to_uppercase(), command.to_string());
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

    pub fn local_vars(&self) -> Vec<(&str, &str)> {
        self.scope_stack
            .last()
            .map(|scope| {
                scope
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn passthrough_env(
        &self,
        passthrough: &[String],
        filter: &[String],
    ) -> Vec<(String, String)> {
        self.vars
            .iter()
            .filter(|(k, _)| {
                if filter.iter().any(|f| f == k.as_str()) {
                    return false;
                }
                if passthrough.is_empty() {
                    return self.exported.get(k.as_str()).copied().unwrap_or(false);
                }
                passthrough.iter().any(|p| p == k.as_str())
                    || self.exported.get(k.as_str()).copied().unwrap_or(false)
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

    pub fn unset_named_dir(&mut self, name: &str) {
        self.named_dirs.remove(name);
    }

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
        self.assoc_arrays
            .get(name)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn assoc_values(&self, name: &str) -> Vec<String> {
        self.assoc_arrays
            .get(name)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn assoc_pairs(&self, name: &str) -> Vec<(String, String)> {
        self.assoc_arrays
            .get(name)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    pub fn assoc_len(&self, name: &str) -> usize {
        self.assoc_arrays.get(name).map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_indexed_array(&self, name: &str) -> bool {
        let mut key = String::with_capacity(name.len() + 2);
        key.push_str(name);
        key.push_str("_0");
        self.vars.contains_key(&key)
    }

    pub fn indexed_array_get(&self, name: &str, key: &str) -> Option<&str> {
        self.vars
            .get(&format!("{}_{}", name, key))
            .map(|s| s.as_str())
    }

    /// All consecutive elements of an indexed array (`{name}_0`, `{name}_1`,
    /// ...), honoring current scoping.
    pub fn indexed_array_elements(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut key = format!("{}_", name);
        let base_len = key.len();
        loop {
            key.truncate(base_len);
            key.push_str(&out.len().to_string());
            match self.get(&key) {
                Some(v) => out.push(v.to_string()),
                None => break,
            }
        }
        out
    }

    pub fn indexed_array_set(&mut self, name: &str, key: &str, value: &str) {
        self.set(&format!("{}_{}", name, key), value);
    }

    pub fn indexed_array_len(&self, name: &str) -> usize {
        let mut count = 0;
        let mut key = format!("{}_", name);
        let base_len = key.len();
        loop {
            key.truncate(base_len);
            key.push_str(&count.to_string());
            if self.vars.contains_key(&key) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    pub fn indexed_array_unset(&mut self, name: &str, key: &str) {
        self.unset(&format!("{}_{}", name, key));
    }

    pub fn push_scope(&mut self) -> usize {
        self.scope_stack.push(HashMap::new());
        self.scope_stack.len()
    }

    pub fn pop_scope(&mut self, saved: usize) {
        if saved <= self.scope_stack.len() {
            self.scope_stack.truncate(saved);
        } else {
            self.scope_stack.clear();
        }
    }

    pub fn set_local(&mut self, key: &str, value: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(key.to_string(), value.to_string());
        } else {
            self.vars.insert(key.to_string(), value.to_string());
        }
    }

    pub fn set_global(&mut self, key: &str, value: &str) {
        if self.is_readonly(key) {
            eprintln!("context: {}: readonly variable", key);
            return;
        }
        self.vars.insert(key.to_string(), value.to_string());
        if !is_internal(key) && self.is_exported(key) {
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }

    pub fn get_var_attrs(&self, key: &str) -> VarAttrs {
        self.attrs.get(key).cloned().unwrap_or_default()
    }

    pub fn set_var_attrs(&mut self, key: &str, new_attrs: VarAttrs) {
        self.attrs.insert(key.to_string(), new_attrs);
    }

    pub fn clear_inherited(&mut self, keep_keys: &[String]) {
        let inherited_keys: Vec<String> = self
            .vars
            .keys()
            .filter(|k| !keep_keys.iter().any(|kk| kk == k.as_str()))
            .cloned()
            .collect();
        for key in inherited_keys {
            self.vars.remove(&key);
            self.exported.remove(&key);
            if !is_internal(&key) {
                unsafe {
                    std::env::remove_var(&key);
                }
            }
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nameref_follow_chain() {
        let mut env = Env::new();
        env.set("target", "hello");
        env.set("ref1", "target");
        env.set_var_attrs(
            "ref1",
            VarAttrs {
                readonly: false,
                exported: false,
                integer: false,
                lowercase: false,
                uppercase: false,
                nameref: Some("target".into()),
                trace: false,
            },
        );
        env.set("ref2", "ref1");
        env.set_var_attrs(
            "ref2",
            VarAttrs {
                readonly: false,
                exported: false,
                integer: false,
                lowercase: false,
                uppercase: false,
                nameref: Some("ref1".into()),
                trace: false,
            },
        );
        assert_eq!(env.get("ref2"), Some("hello"));
    }

    #[test]
    fn test_readonly_prevents_unset() {
        let mut env = Env::new();
        env.set("LOCKED", "value");
        env.set_readonly("LOCKED");
        env.unset("LOCKED");
        assert_eq!(env.get("LOCKED"), Some("value"));
    }

    #[test]
    fn test_export_and_is_exported() {
        let mut env = Env::new();
        env.set("MYVAR", "test");
        assert!(!env.is_exported("MYVAR"));
        env.export("MYVAR");
        assert!(env.is_exported("MYVAR"));
        env.unexport("MYVAR");
        assert!(!env.is_exported("MYVAR"));
    }

    #[test]
    fn test_positional_params() {
        let mut env = Env::new();
        env.set_positional(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(env.positional(), &["a", "b", "c"]);
        assert_eq!(env.positional().len(), 3);
    }

    #[test]
    fn test_alias_roundtrip() {
        let mut env = Env::new();
        env.set_alias("ll", "ls -la");
        assert_eq!(env.get_alias("ll"), Some("ls -la"));
        assert!(env.get_alias("nonexistent").is_none());
        env.unset_alias("ll");
        assert!(env.get_alias("ll").is_none());
    }

    #[test]
    fn test_merge_from() {
        let mut base = Env::new();
        base.set("A", "1");
        let mut overlay = Env::new();
        overlay.set("B", "2");
        overlay.set("A", "override");
        base.merge_from(&overlay);
        assert_eq!(base.get("A"), Some("override"));
        assert_eq!(base.get("B"), Some("2"));
    }

    #[test]
    fn test_dirstack() {
        let mut env = Env::new();
        env.set_dirstack(&["/tmp".into(), "/home".into(), "/var".into()]);
        assert_eq!(env.get("DIRSTACK"), Some("/tmp\n/home\n/var"));
        assert_eq!(env.get("DIRSTACK_0"), Some("/tmp"));
        assert_eq!(env.get("DIRSTACK_2"), Some("/var"));
    }

    #[test]
    fn test_set_var_with_attrs_prevents_overwrite() {
        let mut env = Env::new();
        env.set("A", "original");
        env.set_readonly("A");
        let result = env.set("A", "new_value");
        assert!(!result);
        assert_eq!(env.get("A"), Some("original"));
    }
}
