use crate::config::Config;
use crate::shell::env::Env;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::Command;
use std::sync::{
    LazyLock, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

pub static FUNCTION_DEPTH: AtomicUsize = AtomicUsize::new(0);

type ReadlineFn = dyn Fn(&str) -> io::Result<String> + Send + Sync;
type VoidFn = dyn Fn(&str) + Send + Sync;
type I32Fn = dyn Fn(i32) + Send + Sync;
type LookupFn = dyn Fn(&str) -> Option<String> + Send + Sync;

pub static READLINE_CB: OnceLock<Box<ReadlineFn>> = OnceLock::new();
pub static HISTORY_CB: OnceLock<Box<dyn Fn() -> Vec<String> + Send + Sync>> = OnceLock::new();
pub static FC_EXEC_CB: OnceLock<Box<VoidFn>> = OnceLock::new();
pub static DISOWN_JOBS_CB: OnceLock<Mutex<Option<Box<I32Fn>>>> = OnceLock::new();
pub static GET_FUNCTION_CB: OnceLock<Mutex<Option<Box<LookupFn>>>> = OnceLock::new();

static MASK_SECRETS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_mask_secrets(enabled: bool) {
    MASK_SECRETS.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

fn maybe_mask_output(s: &str) -> String {
    if !MASK_SECRETS.load(std::sync::atomic::Ordering::Relaxed) {
        return s.to_string();
    }
    let keywords = ["PASSWORD", "TOKEN", "SECRET", "API_KEY", "PRIVATE_KEY"];
    let mut result = String::new();
    for line in s.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = &line[..eq_pos];
            let upper_key = key.to_uppercase();
            if keywords.iter().any(|kw| upper_key.contains(kw)) && !line[eq_pos + 1..].is_empty() {
                result.push_str(&format!("{}=***\n", key));
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

static DISABLED_BUILTINS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub static PATH_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub enum CompletionSpec {
    Function(String),
    Command(String),
}

pub static COMPLETIONS: LazyLock<Mutex<HashMap<String, CompletionSpec>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct CallerFrame {
    pub name: String,
    pub line: usize,
}

pub static CALL_STACK: LazyLock<Mutex<Vec<CallerFrame>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn is_disabled(name: &str) -> bool {
    DISABLED_BUILTINS.lock().unwrap().contains(name)
}

/// Every builtin name the shell dispatches (or intercepts) — the single
/// source of truth for `builtin_is`, `type`, `enable` and completion.
pub const BUILTINS: &[&str] = &[
    ":",
    ".",
    "[",
    "alias",
    "bg",
    "bindkey",
    "break",
    "builtin",
    "caller",
    "cd",
    "command",
    "compgen",
    "complete",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "enable",
    "env",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "math",
    "module",
    "popd",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "realpath",
    "regexmatch",
    "return",
    "select",
    "set",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "unsetenv",
    "wait",
    "which",
];

pub struct BuiltinResult {
    pub status: i32,
    pub exit: bool,
    pub exit_code: Option<i32>,
    pub source_file: Option<String>,
    pub source_args: Option<Vec<String>>,
    pub clear_history: bool,
    pub eval_string: Option<String>,
    pub needs_executor: bool,
}

impl BuiltinResult {
    pub fn ok() -> Self {
        Self {
            status: 0,
            exit: false,
            exit_code: None,
            source_file: None,
            source_args: None,
            clear_history: false,
            eval_string: None,
            needs_executor: false,
        }
    }
    pub fn err(status: i32) -> Self {
        Self {
            status,
            exit: false,
            exit_code: None,
            source_file: None,
            source_args: None,
            clear_history: false,
            eval_string: None,
            needs_executor: false,
        }
    }
    pub fn exit(code: i32) -> Self {
        Self {
            status: code,
            exit: true,
            exit_code: Some(code),
            source_file: None,
            source_args: None,
            clear_history: false,
            eval_string: None,
            needs_executor: false,
        }
    }
    pub fn needs_executor() -> Self {
        Self {
            status: 0,
            exit: false,
            exit_code: None,
            source_file: None,
            source_args: None,
            clear_history: false,
            eval_string: None,
            needs_executor: true,
        }
    }
}

pub fn run(args: &[String], env: &mut Env, cfg: &Config, last_status: i32) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let cmd = args[0].as_str();
    match cmd {
        "cd" => cmd_cd(&args[1..], env, cfg),
        "exit" => cmd_exit(&args[1..], last_status),
        "export" => cmd_export(&args[1..], env),
        "unset" => cmd_unset(&args[1..], env),
        "alias" => cmd_alias(&args[1..], env),
        "unalias" => cmd_unalias(&args[1..], env),
        "source" | "." => cmd_source(&args[1..], env),
        "history" => cmd_history(&args[1..], cfg),
        "set" => cmd_set(&args[1..], env),
        "unsetenv" => cmd_unsetenv(&args[1..], env),
        "env" => cmd_env(env, cfg),
        "pwd" => cmd_pwd(&args[1..]),
        "type" => cmd_type(&args[1..], env),
        "which" => cmd_which(&args[1..]),
        "echo" => cmd_echo(&args[1..]),
        "printf" => cmd_printf(&args[1..]),
        "true" => BuiltinResult::ok(),
        "false" => BuiltinResult::err(1),
        ":" => BuiltinResult::ok(),
        "shift" => cmd_shift(&args[1..], env),
        "test" => cmd_test(&args[1..]),
        "[" => {
            let mut test_args = args[1..].to_vec();
            if test_args.last().map(|s| s == "]").unwrap_or(false) {
                test_args.pop();
            } else {
                eprintln!("context: [: missing `]'");
                return BuiltinResult::err(2);
            }
            cmd_test(&test_args)
        }
        "let" => cmd_let(&args[1..], env),
        "exec" => BuiltinResult::needs_executor(),
        "trap" => cmd_trap(&args[1..], env),
        "pushd" => cmd_pushd(&args[1..], env),
        "popd" => cmd_popd(&args[1..], env),
        "dirs" => cmd_dirs(&args[1..], env),
        "hash" => cmd_hash(&args[1..]),
        "math" => cmd_math(&args[1..], env, cfg),
        "regexmatch" => cmd_regexmatch(&args[1..], env),
        "module" => cmd_module(&args[1..], env),
        "readonly" => cmd_readonly(&args[1..], env),
        "local" => cmd_local(&args[1..], env),
        "builtin" => {
            if args.len() > 1 {
                let builtin_args = &args[1..];
                return run(builtin_args, env, cfg, last_status);
            }
            BuiltinResult::ok()
        }
        "caller" => {
            let stack = CALL_STACK.lock().unwrap();
            if let Some(frame) = stack.last() {
                let depth = stack.len();
                if let Some(arg) = args.get(1) {
                    if let Ok(n) = arg.parse::<usize>() {
                        if n < depth {
                            let f = &stack[depth - 1 - n];
                            println!("{} {}", f.line, f.name);
                            BuiltinResult::ok()
                        } else {
                            eprintln!("context: caller: {}: beyond call stack", n);
                            BuiltinResult::err(1)
                        }
                    } else {
                        eprintln!("context: caller: {}: invalid depth", arg);
                        BuiltinResult::err(1)
                    }
                } else {
                    println!("{} {}", frame.line, frame.name);
                    BuiltinResult::ok()
                }
            } else {
                eprintln!("context: caller: no call stack");
                BuiltinResult::err(1)
            }
        }
        "shopt" => cmd_shopt(&args[1..], env),
        "declare" | "typeset" => cmd_declare(&args[1..], env),
        "wait" => cmd_wait(&args[1..]),
        "kill" => cmd_kill(&args[1..]),
        "umask" => cmd_umask(&args[1..]),
        "command" => cmd_command(&args[1..]),
        "eval" => {
            let code = args[1..].join(" ");
            BuiltinResult {
                status: 0,
                exit: false,
                exit_code: None,
                source_file: None,
                source_args: None,
                clear_history: false,
                eval_string: Some(code),
                needs_executor: false,
            }
        }
        "select" => cmd_select(&args[1..], env),
        "getopts" => cmd_getopts(&args[1..], env),
        "realpath" => cmd_realpath(&args[1..]),
        "read" => cmd_read(&args[1..], env),
        "bindkey" => cmd_bindkey(&args[1..], env),
        "enable" => cmd_enable(&args[1..], env, cfg),
        "help" => cmd_help(&args[1..]),
        "ulimit" => cmd_ulimit(&args[1..]),
        "times" => cmd_times(),
        "logout" => cmd_logout(),
        "mapfile" | "readarray" => cmd_mapfile(&args[1..], env),
        "compgen" => cmd_compgen(&args[1..], env),
        "complete" => cmd_complete(&args[1..]),
        "fc" => cmd_fc(&args[1..], env, cfg),
        "disown" => cmd_disown(&args[1..]),
        "suspend" => cmd_suspend(),
        _ => BuiltinResult::err(127),
    }
}

fn cmd_cd(args: &[String], env: &mut Env, cfg: &Config) -> BuiltinResult {
    let mut physical = false;
    let mut start = 0;
    for arg in args {
        if arg == "-P" {
            physical = true;
            start += 1;
        } else if arg == "-L" {
            start += 1;
        } else {
            break;
        }
    }
    let target = if start >= args.len() {
        env.home()
    } else if args[start] == "-" {
        env.get("OLDPWD").unwrap_or(&env.home()).to_string()
    } else if args[start] == "~" {
        env.home()
    } else if let Some(rest) = args[start].strip_prefix("~/") {
        format!("{}/{}", env.home(), rest)
    } else {
        args[start].clone()
    };

    let old = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let new_dir = Path::new(&target);
    if new_dir.is_absolute() || target.starts_with('/') {
        if let Err(e) = std::env::set_current_dir(new_dir) {
            if cfg.execution.cdspell
                && let Some(suggestion) = spell_correct_dir(&target, &old)
            {
                eprintln!(
                    "context: cd: {}: {}. Did you mean '{}'?",
                    target, e, suggestion
                );
                return BuiltinResult::err(1);
            }
            eprintln!("context: cd: {}: {}", target, e);
            return BuiltinResult::err(1);
        }
    } else if let Ok(cdpath) = std::env::var("CDPATH") {
        let mut found = false;
        for dir in cdpath.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = Path::new(dir).join(&target);
            if candidate.is_dir() {
                if let Err(e) = std::env::set_current_dir(&candidate) {
                    eprintln!("context: cd: {}: {}", candidate.display(), e);
                    return BuiltinResult::err(1);
                }
                println!("{}", candidate.display());
                found = true;
                break;
            }
        }
        if !found && let Err(e) = std::env::set_current_dir(new_dir) {
            if cfg.execution.cdspell
                && let Some(suggestion) = spell_correct_dir(&target, &old)
            {
                eprintln!(
                    "context: cd: {}: {}. Did you mean '{}'?",
                    target, e, suggestion
                );
                return BuiltinResult::err(1);
            }
            eprintln!("context: cd: {}: {}", target, e);
            return BuiltinResult::err(1);
        }
    } else {
        if let Err(e) = std::env::set_current_dir(new_dir) {
            if cfg.execution.cdspell
                && let Some(suggestion) = spell_correct_dir(&target, &old)
            {
                eprintln!(
                    "context: cd: {}: {}. Did you mean '{}'?",
                    target, e, suggestion
                );
                return BuiltinResult::err(1);
            }
            eprintln!("context: cd: {}: {}", target, e);
            return BuiltinResult::err(1);
        }
    }

    env.set("OLDPWD", &old);
    if physical {
        let c_target = std::ffi::CString::new(target.as_str()).unwrap_or_default();
        let resolved = unsafe {
            let buf = vec![0u8; libc::PATH_MAX as usize];
            let ptr = libc::realpath(c_target.as_ptr(), buf.as_ptr() as *mut libc::c_char);
            if ptr.is_null() {
                None
            } else {
                let cstr = std::ffi::CStr::from_ptr(ptr);
                let s = cstr.to_string_lossy().to_string();
                libc::free(ptr as *mut libc::c_void);
                Some(s)
            }
        };
        if let Some(p) = resolved {
            env.set("PWD", &p);
            if start < args.len() && args[start] == "-" {
                println!("{}", p);
            }
        } else if let Ok(cwd) = std::env::current_dir() {
            env.set("PWD", &cwd.to_string_lossy());
        }
    } else {
        // `-L` (default): build PWD logically from the current PWD without
        // resolving symlinks.
        let base = env
            .get("PWD")
            .filter(|p| p.starts_with('/'))
            .map(|s| s.to_string())
            .unwrap_or_else(|| old.clone());
        let logical = logical_path(&base, &target);
        env.set("PWD", &logical);
        if start < args.len() && args[start] == "-" {
            println!("{}", logical);
        }
    }
    BuiltinResult::ok()
}

/// Join `base` and `target` lexically, resolving `.` and `..` without any
/// filesystem access (`cd -L` semantics).
fn logical_path(base: &str, target: &str) -> String {
    let combined = if target.starts_with('/') {
        target.to_string()
    } else if base.is_empty() || base == "/" {
        format!("/{}", target)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), target)
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

fn spell_correct_dir(target: &str, cwd: &str) -> Option<String> {
    let parent = if target.contains('/') {
        Path::new(target).parent().unwrap_or(Path::new(cwd))
    } else {
        Path::new(cwd)
    };
    let prefix = if target.contains('/') {
        target.rsplit('/').next().unwrap_or("")
    } else {
        target
    };
    if prefix.is_empty() {
        return None;
    }
    let entries = std::fs::read_dir(parent).ok()?;
    let threshold = if prefix.len() <= 3 { 1 } else { 2 };
    let mut best: Option<(String, usize)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let dist = crate::shell::executor::levenshtein(prefix, &name);
        if dist <= threshold {
            match &best {
                Some((_, best_dist)) if dist < *best_dist => {
                    let full = if target.contains('/') {
                        let dir_part =
                            target[..target.rfind('/').expect("contains '/'") + 1].to_string();
                        format!("{}{}", dir_part, name)
                    } else {
                        name.clone()
                    };
                    best = Some((full, dist));
                }
                None => {
                    let full = if target.contains('/') {
                        let dir_part =
                            target[..target.rfind('/').expect("contains '/'") + 1].to_string();
                        format!("{}{}", dir_part, name)
                    } else {
                        name.clone()
                    };
                    best = Some((full, dist));
                }
                _ => {}
            }
        }
    }
    best.map(|(name, _)| name)
}

fn cmd_exit(args: &[String], last_status: i32) -> BuiltinResult {
    if args.len() > 1 {
        eprintln!("context: exit: too many arguments");
        return BuiltinResult::err(2);
    }
    let code = match args.first() {
        Some(s) => match s.parse::<i32>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("context: exit: {}: numeric argument required", s);
                return BuiltinResult::exit(2);
            }
        },
        None => last_status,
    };
    BuiltinResult::exit(code)
}

fn cmd_export(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_vars() {
            if env.is_exported(k) {
                println!("declare -x {}=\"{}\"", k, v);
            }
        }
        return BuiltinResult::ok();
    }
    if args[0] == "-p" {
        for (k, v) in env.all_vars() {
            if env.is_exported(k) {
                println!("declare -x {}=\"{}\"", k, v);
            }
        }
        return BuiltinResult::ok();
    }
    if args[0] == "-n" {
        for arg in &args[1..] {
            env.unexport(arg);
        }
        return BuiltinResult::ok();
    }
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set_exported(name, value, true);
        } else {
            env.export(arg);
        }
    }
    BuiltinResult::ok()
}

fn cmd_unset(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut func_mode = false;
    let mut vars: Vec<&String> = Vec::new();
    for arg in args {
        if arg == "-f" || arg == "-v" {
            if arg == "-f" {
                func_mode = true;
            }
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'f' => func_mode = true,
                    'v' => func_mode = false,
                    _ => {
                        eprintln!("context: unset: -{}: invalid option", ch);
                        return BuiltinResult::err(2);
                    }
                }
            }
            continue;
        }
        vars.push(arg);
    }
    if func_mode {
        for arg in &vars {
            eprintln!("context: unset: {}: function unset not yet supported", arg);
        }
        return BuiltinResult::err(1);
    }
    for arg in vars {
        if env.is_readonly(arg) {
            eprintln!("context: unset: {}: readonly variable", arg);
            return BuiltinResult::err(1);
        }
        if arg.contains('[')
            && arg.ends_with(']')
            && let Some(bracket_pos) = arg.find('[')
        {
            let name = &arg[..bracket_pos];
            let key = &arg[bracket_pos + 1..].trim_end_matches(']');
            if env.is_assoc_array(name) {
                env.assoc_unset(name, key);
            } else if env.is_indexed_array(name) {
                env.indexed_array_unset(name, key);
            }
            continue;
        }
        env.unset(arg);
    }
    BuiltinResult::ok()
}

fn cmd_alias(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_aliases() {
            println!("alias {}='{}'", k, v);
        }
        return BuiltinResult::ok();
    }
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            // Strip exactly one matching outer quote pair, preserving any
            // inner quotes.
            let value = match (value.chars().next(), value.chars().last()) {
                (Some('\''), Some('\'')) if value.len() >= 2 => &value[1..value.len() - 1],
                (Some('"'), Some('"')) if value.len() >= 2 => &value[1..value.len() - 1],
                _ => value,
            };
            env.set_alias(name, value);
        } else {
            match env.get_alias(arg) {
                Some(v) => println!("alias {}='{}'", arg, v),
                None => {
                    eprintln!("context: alias: {}: not found", arg);
                    return BuiltinResult::err(1);
                }
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_unalias(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if env.get_alias(arg).is_none() {
            eprintln!("context: unalias: {}: not found", arg);
            return BuiltinResult::err(1);
        }
        env.unset_alias(arg);
    }
    BuiltinResult::ok()
}

fn cmd_source(args: &[String], env: &Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: source: filename argument required");
        return BuiltinResult::err(1);
    }
    let path = if args[0] == "~" {
        env.home()
    } else if let Some(rest) = args[0].strip_prefix("~/") {
        format!("{}/{}", env.home(), rest)
    } else if !args[0].contains('/') {
        match find_in_path(&args[0]) {
            Some(p) => p,
            None => args[0].clone(),
        }
    } else {
        args[0].clone()
    };
    match fs::read_to_string(&path) {
        Ok(_) => {
            let extra_args = if args.len() > 1 {
                Some(args[1..].to_vec())
            } else {
                None
            };
            BuiltinResult {
                status: 0,
                exit: false,
                exit_code: None,
                source_file: Some(path),
                source_args: extra_args,
                clear_history: false,
                eval_string: None,
                needs_executor: false,
            }
        }
        Err(e) => {
            eprintln!("context: source: {}: {}", path, e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_history(args: &[String], cfg: &Config) -> BuiltinResult {
    let history_path = crate::config::loader::history_path(cfg);
    if args.first().map(|s| s.as_str()) == Some("-c") {
        match fs::write(&history_path, "") {
            Ok(_) => BuiltinResult {
                status: 0,
                exit: false,
                exit_code: None,
                source_file: None,
                source_args: None,
                clear_history: true,
                eval_string: None,
                needs_executor: false,
            },
            Err(e) => {
                eprintln!("context: history: -c: {}", e);
                BuiltinResult::err(1)
            }
        }
    } else if args.first().map(|s| s.as_str()) == Some("-w") {
        eprintln!("context: history: -w: history writing not yet supported");
        BuiltinResult::ok()
    } else if args.first().map(|s| s.as_str()) == Some("-d") {
        let Some(num) = args.get(1).and_then(|s| s.parse::<usize>().ok()) else {
            eprintln!("context: history: -d: history position required");
            return BuiltinResult::err(2);
        };
        match fs::read_to_string(&history_path) {
            Ok(contents) => {
                let mut lines: Vec<&str> = contents.lines().collect();
                if num == 0 || num > lines.len() {
                    eprintln!("context: history: {}: no such history entry", num);
                    return BuiltinResult::err(1);
                }
                lines.remove(num - 1);
                let mut out = lines.join("\n");
                if !out.is_empty() {
                    out.push('\n');
                }
                match fs::write(&history_path, out) {
                    Ok(_) => BuiltinResult::ok(),
                    Err(e) => {
                        eprintln!("context: history: -d: {}", e);
                        BuiltinResult::err(1)
                    }
                }
            }
            Err(e) => {
                eprintln!("context: history: -d: {}", e);
                BuiltinResult::err(1)
            }
        }
    } else {
        match fs::read_to_string(&history_path) {
            Ok(contents) => {
                let max = args
                    .first()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let lines: Vec<&str> = contents.lines().collect();
                let start = if max > 0 && max < lines.len() {
                    lines.len() - max
                } else {
                    0
                };
                for (i, line) in lines[start..].iter().enumerate() {
                    println!("{:5}  {}", start + i + 1, line);
                }
                BuiltinResult::ok()
            }
            Err(_) => BuiltinResult::ok(),
        }
    }
}

fn cmd_set(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_vars() {
            println!("{}={}", k, v);
        }
        return BuiltinResult::ok();
    }
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            env.set_positional(args[i..].to_vec());
            break;
        } else if arg == "-e" {
            env.set("_OPT_E", "1");
        } else if arg == "+e" {
            env.set("_OPT_E", "");
        } else if arg == "-u" {
            env.set("_OPT_U", "1");
        } else if arg == "+u" {
            env.set("_OPT_U", "");
        } else if arg == "-x" {
            env.set("_OPT_X", "1");
        } else if arg == "+x" {
            env.set("_OPT_X", "");
        } else if arg == "-a" {
            env.set("_OPT_A", "1");
        } else if arg == "+a" {
            env.set("_OPT_A", "");
        } else if arg == "-b" {
            env.set("_OPT_B", "1");
        } else if arg == "+b" {
            env.set("_OPT_B", "");
        } else if arg == "-f" {
            env.set("_OPT_G", "1");
        } else if arg == "+f" {
            env.set("_OPT_G", "");
        } else if arg == "-h" {
            env.set("_OPT_H", "1");
        } else if arg == "+h" {
            env.set("_OPT_H", "");
        } else if arg == "-m" {
            env.set("_OPT_M", "1");
        } else if arg == "+m" {
            env.set("_OPT_M", "");
        } else if arg == "-n" {
            env.set("_OPT_N_PARSE", "1");
        } else if arg == "+n" {
            env.set("_OPT_N_PARSE", "");
        } else if arg == "-v" {
            env.set("_OPT_V", "1");
        } else if arg == "+v" {
            env.set("_OPT_V", "");
        } else if arg == "-o" {
            i += 1;
            if i < args.len() {
                match args[i].as_str() {
                    "errexit" | "exitonerror" => {
                        env.set("_OPT_E", "1");
                    }
                    "nounset" | "undefinedvariable" => {
                        env.set("_OPT_U", "1");
                    }
                    "xtrace" => {
                        env.set("_OPT_X", "1");
                    }
                    "verbose" => {
                        env.set("_OPT_V", "1");
                    }
                    "allexport" | "all" => {
                        env.set("_OPT_A", "1");
                    }
                    "noclobber" => {
                        env.set("_OPT_N", "1");
                    }
                    "noglob" => {
                        env.set("_OPT_G", "1");
                    }
                    "notify" | "bgn" => {
                        env.set("_OPT_B", "1");
                    }
                    "hashall" | "hashcmds" => {
                        env.set("_OPT_H", "1");
                    }
                    "monitor" => {
                        env.set("_OPT_M", "1");
                    }
                    "noexec" | "noexpansion" => {
                        env.set("_OPT_N_PARSE", "1");
                    }
                    "interactive" | "i" => {}
                    "posix" => {}
                    "nullglob" => {}
                    "pipefail" => {
                        env.set("_OPT_PIPEFAIL", "1");
                    }
                    _ => {
                        eprintln!("context: set: -o: {}: unknown option", args[i]);
                        return BuiltinResult::err(2);
                    }
                }
            } else {
                let opt_names = [
                    ("errexit", "_OPT_E"),
                    ("nounset", "_OPT_U"),
                    ("xtrace", "_OPT_X"),
                    ("allexport", "_OPT_A"),
                    ("noclobber", "_OPT_N"),
                    ("noglob", "_OPT_G"),
                    ("notify", "_OPT_B"),
                    ("hashall", "_OPT_H"),
                    ("monitor", "_OPT_M"),
                    ("noexec", "_OPT_N_PARSE"),
                    ("verbose", "_OPT_V"),
                    ("pipefail", "_OPT_PIPEFAIL"),
                ];
                for (name, var) in &opt_names {
                    let state = if env.get(var).map(|s| s == "1").unwrap_or(false) {
                        "on"
                    } else {
                        "off"
                    };
                    println!("-o {}={}", name, state);
                }
            }
        } else if arg == "+o" {
            i += 1;
            if i < args.len() {
                match args[i].as_str() {
                    "errexit" | "exitonerror" => {
                        env.set("_OPT_E", "");
                    }
                    "nounset" | "undefinedvariable" => {
                        env.set("_OPT_U", "");
                    }
                    "xtrace" => {
                        env.set("_OPT_X", "");
                    }
                    "verbose" => {
                        env.set("_OPT_V", "");
                    }
                    "allexport" | "all" => {
                        env.set("_OPT_A", "");
                    }
                    "noclobber" => {
                        env.set("_OPT_N", "");
                    }
                    "noglob" => {
                        env.set("_OPT_G", "");
                    }
                    "notify" | "bgn" => {
                        env.set("_OPT_B", "");
                    }
                    "hashall" | "hashcmds" => {
                        env.set("_OPT_H", "");
                    }
                    "monitor" => {
                        env.set("_OPT_M", "");
                    }
                    "noexec" | "noexpansion" => {
                        env.set("_OPT_N_PARSE", "");
                    }
                    "pipefail" => {
                        env.set("_OPT_PIPEFAIL", "");
                    }
                    _ => {
                        eprintln!("context: set: +o: {}: unknown option", args[i]);
                        return BuiltinResult::err(2);
                    }
                }
            }
        } else if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set(name, value);
        } else if !arg.starts_with('+') && !arg.starts_with('-') {
            // `set name value...` — set the positional parameters.
            env.set_positional(args[i..].to_vec());
            break;
        } else {
            eprintln!("context: set: {}: unknown option", arg);
        }
        i += 1;
    }
    BuiltinResult::ok()
}

fn cmd_unsetenv(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        env.unset(arg);
    }
    BuiltinResult::ok()
}

fn cmd_env(env: &Env, cfg: &Config) -> BuiltinResult {
    let mask = cfg.security.mask_secrets;
    for (k, v) in env.all_vars() {
        if mask
            && (k.contains("SECRET")
                || k.contains("TOKEN")
                || k.contains("PASSWORD")
                || k.contains("API_KEY")
                || k.contains("PRIVATE"))
        {
            println!("{}=***", k);
        } else {
            println!("{}={}", k, v);
        }
    }
    BuiltinResult::ok()
}

fn cmd_pwd(args: &[String]) -> BuiltinResult {
    let mut logical = false;
    for arg in args {
        if arg == "-L" {
            logical = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'L' => logical = true,
                    'P' => {}
                    _ => {
                        eprintln!("context: pwd: -{}: invalid option", ch);
                        return BuiltinResult::err(2);
                    }
                }
            }
        }
    }
    if logical
        && let Ok(pwdir) = std::env::var("PWD")
        && !pwdir.is_empty()
        && Path::new(&pwdir).is_dir()
    {
        println!("{}", pwdir);
        return BuiltinResult::ok();
    }
    match std::env::current_dir() {
        Ok(p) => {
            println!("{}", p.display());
            BuiltinResult::ok()
        }
        Err(e) => {
            eprintln!("context: pwd: {}", e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_type(args: &[String], env: &Env) -> BuiltinResult {
    let builtins = BUILTINS;
    let mut mode_t = false;
    let mut mode_p = false;
    let mut mode_big_p = false;
    let mut mode_a = false;
    let mut mode_f = false;
    let mut names: Vec<&String> = Vec::new();
    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 && !arg.contains(' ') {
            for ch in arg[1..].chars() {
                match ch {
                    't' => mode_t = true,
                    'p' => mode_p = true,
                    'P' => mode_big_p = true,
                    'a' => mode_a = true,
                    'f' => mode_f = true,
                    _ => {
                        eprintln!("context: type: -{}: invalid option", ch);
                        return BuiltinResult::err(1);
                    }
                }
            }
        } else {
            names.push(arg);
        }
    }
    if names.is_empty() {
        eprintln!("context: type: name argument required");
        return BuiltinResult::err(1);
    }
    let mut status = 0;
    for name in &names {
        let name_str = name.as_str();
        if mode_p {
            if builtins.contains(&name_str) || env.get_alias(name_str).is_some() {
                continue;
            }
            if let Some(path) = find_in_path(name_str) {
                if mode_t {
                    println!("file");
                } else {
                    println!("{}", path);
                }
            } else {
                eprintln!("context: type: {}: not found", name_str);
                status = 1;
            }
            continue;
        }
        if mode_big_p {
            if let Some(path) = find_in_path(name_str) {
                if mode_t {
                    println!("file");
                } else {
                    println!("{}", path);
                }
            } else {
                if mode_t {
                    if builtins.contains(&name_str) {
                        println!("builtin");
                    } else if env.get_alias(name_str).is_some() {
                        println!("alias");
                    } else {
                        eprintln!("context: type: {}: not found", name_str);
                        status = 1;
                    }
                } else {
                    eprintln!("context: type: {}: not found", name_str);
                    status = 1;
                }
            }
            continue;
        }
        let mut found_any = false;
        if mode_a {
            if !mode_f && env.get_alias(name_str).is_some() {
                if mode_t {
                    println!("alias");
                } else {
                    println!(
                        "{} is aliased to '{}'",
                        name_str,
                        env.get_alias(name_str).unwrap()
                    );
                }
                found_any = true;
            }
            if builtins.contains(&name_str) {
                if mode_t {
                    println!("builtin");
                } else {
                    println!("{} is a shell builtin", name_str);
                }
                found_any = true;
            }
            if let Some(path) = find_in_path(name_str) {
                if mode_t {
                    println!("file");
                } else {
                    println!("{} is {}", name_str, path);
                }
                found_any = true;
            }
            if !found_any {
                eprintln!("context: type: {}: not found", name_str);
                status = 1;
            }
        } else {
            if !mode_f && env.get_alias(name_str).is_some() {
                if mode_t {
                    println!("alias");
                } else {
                    println!(
                        "{} is aliased to '{}'",
                        name_str,
                        env.get_alias(name_str).unwrap()
                    );
                }
            } else if builtins.contains(&name_str) {
                if mode_t {
                    println!("builtin");
                } else {
                    println!("{} is a shell builtin", name_str);
                }
            } else if let Some(path) = find_in_path(name_str) {
                if mode_t {
                    println!("file");
                } else {
                    println!("{} is {}", name_str, path);
                }
            } else {
                eprintln!("context: type: {}: not found", name_str);
                status = 1;
            }
        }
    }
    BuiltinResult::err(status)
}

fn cmd_which(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let mut status = 0;
    for arg in args {
        if let Some(path) = find_in_path(arg) {
            println!("{}", path);
        } else {
            eprintln!("context: which: {}: not found", arg);
            status = 1;
        }
    }
    BuiltinResult::err(status)
}

fn find_in_path(cmd: &str) -> Option<String> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        let full = Path::new(dir).join(cmd);
        if full.is_file() {
            return Some(full.to_string_lossy().to_string());
        }
    }
    None
}

fn cmd_echo(args: &[String]) -> BuiltinResult {
    let mut start = 0;
    let mut newline = true;
    let mut escape = false;

    while start < args.len()
        && args[start].starts_with('-')
        && args[start].len() > 1
        && !args[start].contains(' ')
    {
        let flag_str = &args[start][1..];
        let mut valid = true;
        for ch in flag_str.chars() {
            match ch {
                'n' => newline = false,
                'e' => escape = true,
                'E' => escape = false,
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if valid {
            start += 1;
        } else {
            break;
        }
    }

    let mut output = String::new();
    for (i, arg) in args[start..].iter().enumerate() {
        if i > 0 {
            output.push(' ');
        }
        if escape {
            output.push_str(&escape_echo(arg));
        } else {
            output.push_str(arg);
        }
    }
    if newline {
        output.push('\n');
    }
    print!("{}", maybe_mask_output(&output));
    BuiltinResult::ok()
}

fn escape_echo(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '\\' && i + 1 < len {
            i += 1;
            match chars[i] {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                'a' => result.push('\x07'),
                'b' => result.push('\x08'),
                'e' => result.push('\x1b'),
                'f' => result.push('\x0c'),
                'v' => result.push('\x0b'),
                'c' => break,
                'x' => {
                    i += 1;
                    let mut hex = String::new();
                    while i < len && hex.len() < 2 && chars[i].is_ascii_hexdigit() {
                        hex.push(chars[i]);
                        i += 1;
                    }
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    }
                    continue;
                }
                '0' => {
                    let mut oct = String::new();
                    i += 1;
                    while i < len && oct.len() < 3 && matches!(chars[i], '0'..='7') {
                        oct.push(chars[i]);
                        i += 1;
                    }
                    if !oct.is_empty()
                        && let Ok(byte) = u8::from_str_radix(&oct, 8)
                    {
                        result.push(byte as char);
                    }
                    continue;
                }
                '1'..='7' => {
                    let mut oct = String::new();
                    oct.push(chars[i]);
                    i += 1;
                    while i < len && oct.len() < 3 && matches!(chars[i], '0'..='7') {
                        oct.push(chars[i]);
                        i += 1;
                    }
                    if let Ok(byte) = u8::from_str_radix(&oct, 8) {
                        result.push(byte as char);
                    }
                    continue;
                }
                _ => {
                    result.push('\\');
                    result.push(chars[i]);
                }
            }
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}

fn cmd_suspend() -> BuiltinResult {
    unsafe {
        libc::kill(libc::getpid(), libc::SIGTSTP);
    }
    BuiltinResult::ok()
}

fn cmd_printf(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let format = &args[0];
    let fmt_chars: Vec<char> = format.chars().collect();
    let fmt_len = fmt_chars.len();
    let mut output = String::new();
    let mut arg_idx = 0;
    loop {
        let mut i = 0;
        while i < fmt_len {
            if fmt_chars[i] == '\\' && i + 1 < fmt_len {
                i += 1;
                match fmt_chars[i] {
                    'n' => output.push('\n'),
                    't' => output.push('\t'),
                    'r' => output.push('\r'),
                    '\\' => output.push('\\'),
                    'a' => output.push('\x07'),
                    'b' => output.push('\x08'),
                    'e' => output.push('\x1b'),
                    'f' => output.push('\x0c'),
                    'v' => output.push('\x0b'),
                    '0' => {
                        if i + 1 < fmt_len && fmt_chars[i + 1] == 'x' {
                            i += 2;
                            let mut hex = String::new();
                            while i < fmt_len && fmt_chars[i].is_ascii_hexdigit() {
                                hex.push(fmt_chars[i]);
                                i += 1;
                            }
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                output.push(byte as char);
                            }
                        } else {
                            i += 1;
                            let mut octal = String::new();
                            let mut count = 0;
                            while i < fmt_len && count < 3 && matches!(fmt_chars[i], '0'..='7') {
                                octal.push(fmt_chars[i]);
                                i += 1;
                                count += 1;
                            }
                            if !octal.is_empty()
                                && let Ok(byte) = u8::from_str_radix(&octal, 8)
                            {
                                output.push(byte as char);
                            }
                        }
                        continue;
                    }
                    _ => {
                        output.push('\\');
                        output.push(fmt_chars[i]);
                    }
                }
            } else if fmt_chars[i] == '%' && i + 1 < fmt_len {
                i += 1;
                if fmt_chars[i] == '%' {
                    output.push('%');
                } else if fmt_chars[i] == '(' {
                    // %(strftime)T — time formatted with an inline strftime spec.
                    i += 1;
                    let start = i;
                    while i < fmt_len && fmt_chars[i] != ')' {
                        i += 1;
                    }
                    let fmt_str: String = fmt_chars[start..i.min(fmt_len)].iter().collect();
                    if i < fmt_len {
                        i += 1;
                    } // consume ')'
                    if i < fmt_len && fmt_chars[i] == 'T' {
                        output.push_str(&strftime_now(&fmt_str));
                    } else {
                        output.push_str("%(");
                        output.push_str(&fmt_str);
                        output.push(')');
                    }
                } else {
                    let mut force_sign = false;
                    let mut space_sign = false;
                    if i < fmt_len && fmt_chars[i] == '+' {
                        force_sign = true;
                        i += 1;
                    } else if i < fmt_len && fmt_chars[i] == ' ' {
                        space_sign = true;
                        i += 1;
                    }
                    let mut alternate = false;
                    if i < fmt_len && fmt_chars[i] == '#' {
                        alternate = true;
                        i += 1;
                    }
                    let mut left_align = false;
                    if i < fmt_len && fmt_chars[i] == '-' {
                        left_align = true;
                        i += 1;
                    }
                    let mut zero_pad = false;
                    if i < fmt_len && fmt_chars[i] == '0' {
                        zero_pad = true;
                        i += 1;
                    }
                    let mut width: usize = 0;
                    if i < fmt_len && fmt_chars[i] == '*' {
                        i += 1;
                        let w_val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        width = w_val.parse().unwrap_or(0) as usize;
                        arg_idx += 1;
                    } else {
                        while i < fmt_len && fmt_chars[i].is_ascii_digit() {
                            width = width * 10 + (fmt_chars[i] as usize - '0' as usize);
                            i += 1;
                        }
                    }
                    let mut precision: Option<usize> = None;
                    if i < fmt_len && fmt_chars[i] == '.' {
                        i += 1;
                        if i < fmt_len && fmt_chars[i] == '*' {
                            i += 1;
                            let p_val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            precision = Some(p_val.parse().unwrap_or(0) as usize);
                            arg_idx += 1;
                        } else {
                            let mut prec: usize = 0;
                            while i < fmt_len && fmt_chars[i].is_ascii_digit() {
                                prec = prec * 10 + (fmt_chars[i] as usize - '0' as usize);
                                i += 1;
                            }
                            precision = Some(prec);
                        }
                    }
                    if i >= fmt_len {
                        output.push('%');
                        continue;
                    }
                    match fmt_chars[i] {
                        's' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                            // Precision truncates on char boundaries; padding is
                            // computed in display columns so wide chars align.
                            let s: String = match precision {
                                Some(p) => val.chars().take(p).collect(),
                                None => val.to_string(),
                            };
                            let disp = crate::terminal::color::visible_len(&s);
                            if disp < width {
                                let pad = " ".repeat(width - disp);
                                if left_align {
                                    output.push_str(&s);
                                    output.push_str(&pad);
                                } else {
                                    output.push_str(&pad);
                                    output.push_str(&s);
                                }
                            } else {
                                output.push_str(&s);
                            }
                            arg_idx += 1;
                        }
                        'd' | 'i' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: i64 = val.parse().unwrap_or(0);
                            let mut s = format!("{}", n);
                            let sign_prefix = if n >= 0 {
                                if force_sign {
                                    Some("+".to_string())
                                } else if space_sign {
                                    Some(" ".to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if let Some(ref prefix) = sign_prefix {
                                s = format!("{}{}", prefix, s);
                            }
                            let padded = if s.len() < width {
                                let pad_char = if zero_pad && !left_align { '0' } else { ' ' };
                                let pad_len = width - s.len();
                                let pad_str: String =
                                    std::iter::repeat_n(pad_char, pad_len).collect();
                                if zero_pad && !left_align {
                                    if let Some(rest) = s.strip_prefix('-') {
                                        format!("-{}{}", pad_str, rest)
                                    } else {
                                        format!("{}{}", pad_str, s)
                                    }
                                } else if left_align {
                                    format!("{}{}", s, pad_str)
                                } else {
                                    format!("{}{}", pad_str, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'x' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: i64 = val.parse().unwrap_or(0);
                            let s = if alternate && n != 0 {
                                format!("0x{:x}", n)
                            } else {
                                format!("{:x}", n)
                            };
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'u' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: u64 = val.parse().unwrap_or(0);
                            let s = format!("{}", n);
                            let padded = if s.len() < width {
                                let fill = if zero_pad { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'X' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: i64 = val.parse().unwrap_or(0);
                            let s = if alternate && n != 0 {
                                format!("0X{:X}", n)
                            } else {
                                format!("{:X}", n)
                            };
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'o' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: i64 = val.parse().unwrap_or(0);
                            let s = if alternate && n != 0 {
                                format!("0o{:o}", n)
                            } else {
                                format!("{:o}", n)
                            };
                            let padded = if s.len() < width {
                                let fill = if zero_pad { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'f' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: f64 = val.parse().unwrap_or(0.0);
                            let prec = precision.unwrap_or(6);
                            let mut s = format!("{:.prec$}", n, prec = prec);
                            if force_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!("+{}", s);
                            } else if space_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!(" {}", s);
                            }
                            let padded = if s.len() < width {
                                let fill = if zero_pad { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'e' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: f64 = val.parse().unwrap_or(0.0);
                            let prec = precision.unwrap_or(6);
                            let mut s = format!("{:.prec$e}", n, prec = prec);
                            if force_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!("+{}", s);
                            } else if space_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!(" {}", s);
                            }
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'E' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: f64 = val.parse().unwrap_or(0.0);
                            let prec = precision.unwrap_or(6);
                            let mut s = format!("{:.prec$E}", n, prec = prec);
                            if force_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!("+{}", s);
                            } else if space_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!(" {}", s);
                            }
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'g' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: f64 = val.parse().unwrap_or(0.0);
                            let prec = precision.unwrap_or(6);
                            let mut s = format!("{:.prec$}", n, prec = prec);
                            if s.contains('.') {
                                if alternate {
                                    s = s.trim_end_matches('0').to_string();
                                } else {
                                    s = s.trim_end_matches('0').trim_end_matches('.').to_string();
                                }
                            }
                            if force_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!("+{}", s);
                            } else if space_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!(" {}", s);
                            }
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'G' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                            let n: f64 = val.parse().unwrap_or(0.0);
                            let prec = precision.unwrap_or(6);
                            let mut s = format!("{:.prec$}", n, prec = prec);
                            if s.contains('.') {
                                if alternate {
                                    s = s.trim_end_matches('0').to_string();
                                } else {
                                    s = s.trim_end_matches('0').trim_end_matches('.').to_string();
                                }
                            }
                            let upper = s.to_uppercase();
                            let mut s = upper;
                            if force_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!("+{}", s);
                            } else if space_sign && !s.starts_with('-') && !s.starts_with('+') {
                                s = format!(" {}", s);
                            }
                            let padded = if s.len() < width {
                                let fill = if zero_pad && !left_align { "0" } else { " " };
                                let pad_len = width - s.len();
                                let pad: String = fill.repeat(pad_len);
                                if left_align {
                                    format!("{}{}", s, pad)
                                } else {
                                    format!("{}{}", pad, s)
                                }
                            } else {
                                s
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'q' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                            let quoted = format!("'{}'", val.replace('\'', "'\\''"));
                            let padded = if quoted.len() < width {
                                let fill = " ".repeat(width - quoted.len());
                                if left_align {
                                    format!("{}{}", quoted, fill)
                                } else {
                                    format!("{}{}", fill, quoted)
                                }
                            } else {
                                quoted
                            };
                            output.push_str(&padded);
                            arg_idx += 1;
                        }
                        'c' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                            if let Some(ch) = val.chars().next() {
                                output.push(ch);
                            }
                            arg_idx += 1;
                        }
                        'b' => {
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                            output.push_str(&escape_echo(val));
                            arg_idx += 1;
                        }
                        'T' | '@' => {
                            // %T — strftime-formatted current time (format from arg,
                            // defaulting to %H:%M:%S); '@' is bash's alias for it.
                            let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                            let fmt_str = if val.is_empty() { "%H:%M:%S" } else { val };
                            output.push_str(&strftime_now(fmt_str));
                            arg_idx += 1;
                        }
                        _ => {
                            output.push('%');
                            output.push(fmt_chars[i]);
                        }
                    }
                }
            } else {
                output.push(fmt_chars[i]);
            }
            i += 1;
        }
        if arg_idx < args.len() - 1 {
            continue;
        }
        break;
    }
    print!("{}", output);
    BuiltinResult::ok()
}

fn strftime_now(fmt: &str) -> String {
    let Ok(c_fmt) = std::ffi::CString::new(fmt) else {
        return String::new();
    };
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as _)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    let mut buf = [0u8; 256];
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            c_fmt.as_ptr(),
            &tm,
        )
    };
    if n > 0 {
        String::from_utf8_lossy(&buf[..n]).into_owned()
    } else {
        String::new()
    }
}

fn cmd_test(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        // `test` with no arguments is false (status 1).
        return BuiltinResult::err(1);
    }
    let result = eval_test_expr(args, 0).0;
    if result {
        BuiltinResult::ok()
    } else {
        BuiltinResult::err(1)
    }
}

fn eval_test_expr(args: &[String], pos: usize) -> (bool, usize) {
    eval_test_or(args, pos)
}

fn eval_test_or(args: &[String], pos: usize) -> (bool, usize) {
    let (mut left, mut i) = eval_test_and(args, pos);
    while i < args.len() && args[i] == "-o" {
        i += 1;
        let (right, next_i) = eval_test_and(args, i);
        left = left || right;
        i = next_i;
    }
    (left, i)
}

fn eval_test_and(args: &[String], pos: usize) -> (bool, usize) {
    let (mut left, mut i) = eval_test_not(args, pos);
    while i < args.len() && args[i] == "-a" {
        i += 1;
        let (right, next_i) = eval_test_not(args, i);
        left = left && right;
        i = next_i;
    }
    (left, i)
}

fn eval_test_not(args: &[String], pos: usize) -> (bool, usize) {
    if pos < args.len() && args[pos] == "!" {
        let (val, i) = eval_test_primary(args, pos + 1);
        return (!val, i);
    }
    eval_test_primary(args, pos)
}

fn eval_test_primary(args: &[String], pos: usize) -> (bool, usize) {
    if pos >= args.len() {
        return (false, pos);
    }
    if args[pos] == "(" {
        let (val, i) = eval_test_or(args, pos + 1);
        let i = if i < args.len() && args[i] == ")" {
            i + 1
        } else {
            i
        };
        return (val, i);
    }
    if pos + 2 < args.len() {
        let op = &args[pos + 1];
        let is_bin = matches!(
            op.as_str(),
            "=" | "=="
                | "!="
                | "-eq"
                | "-ne"
                | "-lt"
                | "-le"
                | "-gt"
                | "-ge"
                | "-nt"
                | "-ot"
                | "-ef"
                | "=~"
        );
        if is_bin {
            let b = &args[pos];
            let c = &args[pos + 2];
            let r = match op.as_str() {
                "=" | "==" => b == c,
                "!=" => b != c,
                "-eq" => b.parse::<i64>().unwrap_or(0) == c.parse::<i64>().unwrap_or(0),
                "-ne" => b.parse::<i64>().unwrap_or(0) != c.parse::<i64>().unwrap_or(0),
                "-lt" => b.parse::<i64>().unwrap_or(0) < c.parse::<i64>().unwrap_or(0),
                "-le" => b.parse::<i64>().unwrap_or(0) <= c.parse::<i64>().unwrap_or(0),
                "-gt" => b.parse::<i64>().unwrap_or(0) > c.parse::<i64>().unwrap_or(0),
                "-ge" => b.parse::<i64>().unwrap_or(0) >= c.parse::<i64>().unwrap_or(0),
                "-nt" => {
                    let pa = std::path::Path::new(b);
                    let pb = std::path::Path::new(c);
                    match (pa.metadata(), pb.metadata()) {
                        (Ok(ma), Ok(mb)) => {
                            let ta = ma.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            let tb = mb.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            ta > tb
                        }
                        _ => false,
                    }
                }
                "-ot" => {
                    let pa = std::path::Path::new(b);
                    let pb = std::path::Path::new(c);
                    match (pa.metadata(), pb.metadata()) {
                        (Ok(ma), Ok(mb)) => {
                            let ta = ma.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            let tb = mb.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            ta < tb
                        }
                        _ => false,
                    }
                }
                "-ef" => {
                    use std::os::unix::fs::MetadataExt;
                    let pa = std::path::Path::new(b);
                    let pb = std::path::Path::new(c);
                    match (pa.metadata(), pb.metadata()) {
                        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
                        _ => false,
                    }
                }
                "=~" => {
                    if let Ok(re) = regex::Regex::new(c) {
                        re.is_match(b)
                    } else {
                        false
                    }
                }
                _ => false,
            };
            return (r, pos + 3);
        }
    }
    if pos + 1 >= args.len() {
        return (!args[pos].is_empty(), pos + 1);
    }
    if args[pos].len() >= 2 && args[pos].starts_with('-') {
        let op = &args[pos];
        if pos + 1 >= args.len() {
            return (false, pos + 1);
        }
        let a = &args[pos + 1];
        let result = match op.as_str() {
            "-z" => a.is_empty(),
            "-n" => !a.is_empty(),
            "-e" => std::path::Path::new(a).exists(),
            "-f" => std::path::Path::new(a).is_file(),
            "-d" => std::path::Path::new(a).is_dir(),
            "-r" => {
                let c = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                unsafe { libc::access(c.as_ptr(), libc::R_OK) == 0 }
            }
            "-w" => {
                let c = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                    let euid = unsafe { libc::geteuid() };
                    let egid = unsafe { libc::getegid() };
                    if euid == 0 {
                        true
                    } else if st.st_uid == euid {
                        (st.st_mode & libc::S_IWUSR) != 0
                    } else if st.st_gid == egid {
                        (st.st_mode & libc::S_IWGRP) != 0
                    } else {
                        (st.st_mode & libc::S_IWOTH) != 0
                    }
                } else {
                    false
                }
            }
            "-x" => {
                let c = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                    let euid = unsafe { libc::geteuid() };
                    let egid = unsafe { libc::getegid() };
                    if euid == 0 {
                        true
                    } else if st.st_uid == euid {
                        (st.st_mode & libc::S_IXUSR) != 0
                    } else if st.st_gid == egid {
                        (st.st_mode & libc::S_IXGRP) != 0
                    } else {
                        (st.st_mode & libc::S_IXOTH) != 0
                    }
                } else {
                    false
                }
            }
            "-s" => std::fs::metadata(a).map(|m| m.len() > 0).unwrap_or(false),
            "-L" | "-h" => std::path::Path::new(a).is_symlink(),
            "-S" => std::fs::metadata(a)
                .map(|m| m.file_type().is_socket())
                .unwrap_or(false),
            "-p" => std::fs::metadata(a)
                .map(|m| m.file_type().is_fifo())
                .unwrap_or(false),
            "-c" => std::fs::metadata(a)
                .map(|m| m.file_type().is_char_device())
                .unwrap_or(false),
            "-b" => std::fs::metadata(a)
                .map(|m| m.file_type().is_block_device())
                .unwrap_or(false),
            "-N" => {
                use std::os::unix::fs::MetadataExt;
                std::fs::metadata(a)
                    .map(|m| m.atime() > m.mtime())
                    .unwrap_or(false)
            }
            "-t" => {
                if let Ok(fd) = a.parse::<i32>() {
                    unsafe { libc::isatty(fd) == 1 }
                } else {
                    false
                }
            }
            _ => {
                return (!args[pos].is_empty(), pos + 1);
            }
        };
        return (result, pos + 2);
    }
    (!args[pos].is_empty(), pos + 1)
}

fn cmd_let(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut last_result: Option<i64> = None;
    for arg in args {
        let result = eval_arith_assign(arg, env);
        last_result = Some(result);
    }
    match last_result {
        Some(0) => BuiltinResult::err(1),
        Some(_) => BuiltinResult::ok(),
        None => BuiltinResult::ok(),
    }
}

struct ArithmeticParser<'a> {
    chars: Vec<char>,
    pos: usize,
    env: &'a Env,
}

impl<'a> ArithmeticParser<'a> {
    fn new(expr: &str, env: &'a Env) -> Self {
        Self {
            chars: expr.chars().collect(),
            pos: 0,
            env,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_comma(&mut self) -> i64 {
        let mut result = self.parse_conditional();
        self.skip_whitespace();
        while self.peek() == Some(',') {
            self.advance();
            result = self.parse_conditional();
            self.skip_whitespace();
        }
        result
    }

    fn parse_expr(&mut self) -> i64 {
        let mut result = self.parse_term();
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '+' || op == '-' {
                self.advance();
                let rhs = self.parse_term();
                if op == '+' {
                    result += rhs;
                } else {
                    result -= rhs;
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_shift(&mut self) -> i64 {
        let mut result = self.parse_expr();
        self.skip_whitespace();
        loop {
            let two = self.chars.get(self.pos + 1).copied();
            let op = match (self.peek(), two) {
                (Some('<'), Some('<')) => {
                    self.advance();
                    self.advance();
                    Some("sl")
                }
                (Some('>'), Some('>')) => {
                    self.advance();
                    self.advance();
                    Some("sr")
                }
                _ => None,
            };
            if let Some(op) = op {
                let rhs = self.parse_expr();
                result = match op {
                    "sl" => result << rhs,
                    "sr" => result >> rhs,
                    _ => result,
                };
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_conditional(&mut self) -> i64 {
        let cond = self.parse_logical_or();
        self.skip_whitespace();
        if self.peek() == Some('?') {
            self.advance();
            let then_val = self.parse_conditional();
            self.skip_whitespace();
            if self.peek() == Some(':') {
                self.advance();
            }
            let else_val = self.parse_conditional();
            if cond != 0 { then_val } else { else_val }
        } else {
            cond
        }
    }

    fn parse_logical_or(&mut self) -> i64 {
        let mut result = self.parse_logical_and();
        self.skip_whitespace();
        loop {
            if self.peek() == Some('|') && self.chars.get(self.pos + 1) == Some(&'|') {
                self.advance();
                self.advance();
                let rhs = self.parse_logical_and();
                result = if (result != 0) || (rhs != 0) { 1 } else { 0 };
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_logical_and(&mut self) -> i64 {
        let mut result = self.parse_relational();
        self.skip_whitespace();
        loop {
            if self.peek() == Some('&') && self.chars.get(self.pos + 1) == Some(&'&') {
                self.advance();
                self.advance();
                let rhs = self.parse_relational();
                result = if result != 0 && rhs != 0 { 1 } else { 0 };
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_relational(&mut self) -> i64 {
        let mut result = self.parse_shift();
        self.skip_whitespace();
        loop {
            let two = self.chars.get(self.pos + 1).copied();
            let op = match (self.peek(), two) {
                (Some('<'), Some('=')) => {
                    self.advance();
                    self.advance();
                    Some("le")
                }
                (Some('>'), Some('=')) => {
                    self.advance();
                    self.advance();
                    Some("ge")
                }
                (Some('='), Some('=')) => {
                    self.advance();
                    self.advance();
                    Some("eq")
                }
                (Some('!'), Some('=')) => {
                    self.advance();
                    self.advance();
                    Some("ne")
                }
                (Some('<'), _) => {
                    self.advance();
                    Some("lt")
                }
                (Some('>'), _) => {
                    self.advance();
                    Some("gt")
                }
                _ => None,
            };
            if let Some(op) = op {
                let rhs = self.parse_shift();
                result = match op {
                    "lt" => {
                        if result < rhs {
                            1
                        } else {
                            0
                        }
                    }
                    "gt" => {
                        if result > rhs {
                            1
                        } else {
                            0
                        }
                    }
                    "le" => {
                        if result <= rhs {
                            1
                        } else {
                            0
                        }
                    }
                    "ge" => {
                        if result >= rhs {
                            1
                        } else {
                            0
                        }
                    }
                    "eq" => {
                        if result == rhs {
                            1
                        } else {
                            0
                        }
                    }
                    "ne" => {
                        if result != rhs {
                            1
                        } else {
                            0
                        }
                    }
                    _ => result,
                };
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_term(&mut self) -> i64 {
        let mut result = self.parse_factor();
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '*' || op == '/' || op == '%' {
                self.advance();
                let rhs = self.parse_factor();
                match op {
                    '*' => result *= rhs,
                    '/' => {
                        if rhs != 0 {
                            result /= rhs;
                        }
                    }
                    '%' if rhs != 0 => {
                        result %= rhs;
                    }
                    _ => {}
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_factor(&mut self) -> i64 {
        let mut result = self.parse_primary();
        self.skip_whitespace();
        while self.pos + 1 < self.chars.len()
            && self.chars[self.pos] == '*'
            && self.chars[self.pos + 1] == '*'
        {
            self.pos += 2;
            let rhs = self.parse_factor();
            if rhs < 0 {
                eprintln!("context: **: negative exponent");
                return 1;
            }
            result = result.saturating_pow(rhs as u32);
        }
        result
    }

    fn parse_primary(&mut self) -> i64 {
        self.skip_whitespace();
        if let Some(ch) = self.peek() {
            if ch == '(' {
                self.advance();
                let val = self.parse_conditional();
                self.skip_whitespace();
                if self.peek() == Some(')') {
                    self.advance();
                }
                return val;
            }
            if ch == '+' {
                self.advance();
                return self.parse_factor();
            }
            if ch == '-' {
                self.advance();
                return -self.parse_factor();
            }
            if ch.is_ascii_digit() || ch == '\'' {
                return self.parse_number();
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                return self.parse_variable();
            }
        }
        0
    }

    fn parse_number(&mut self) -> i64 {
        let start = self.pos;
        if self.peek() == Some('0') && self.pos + 1 < self.chars.len() {
            let next = self.chars[self.pos + 1];
            if next == 'x' || next == 'X' {
                self.advance();
                self.advance();
                let mut val: i64 = 0;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_hexdigit() {
                        val = val * 16 + ch.to_digit(16).expect("hexdigit") as i64;
                        self.advance();
                    } else {
                        break;
                    }
                }
                return val;
            }
            if next == 'b' || next == 'B' {
                self.advance();
                self.advance();
                let mut val: i64 = 0;
                while let Some(ch) = self.peek() {
                    if ch == '0' || ch == '1' {
                        val = val * 2 + (ch as i64 - '0' as i64);
                        self.advance();
                    } else {
                        break;
                    }
                }
                return val;
            }
            if next.is_ascii_digit() && next <= '7' {
                self.advance();
                let mut val: i64 = 0;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() && ch <= '7' {
                        val = val * 8 + (ch as i64 - '0' as i64);
                        self.advance();
                    } else {
                        break;
                    }
                }
                return val;
            }
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<i64>().unwrap_or(0)
    }

    fn parse_variable(&mut self) -> i64 {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        if name == "RANDOM" {
            return (unsafe { libc::rand() } % 32768) as i64;
        }
        self.env
            .get(&name)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    }
}

pub(crate) fn eval_arithmetic(expr: &str, env: &Env) -> i64 {
    let mut parser = ArithmeticParser::new(expr, env);
    parser.parse_comma()
}

/// Evaluate an arithmetic expression that may contain a top-level assignment
/// (`i = 5`, `i += 1`, ...). The assignment is applied to `env` and the value
/// of the whole expression is returned.
pub(crate) fn eval_arith_assign(expr: &str, env: &mut Env) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }
    if let Some(name) = expr.strip_prefix("++") {
        let name = name.trim();
        let cur = env
            .get(name)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let next = cur + 1;
        env.set(name, &next.to_string());
        return next;
    }
    if let Some(name) = expr.strip_prefix("--") {
        let name = name.trim();
        let cur = env
            .get(name)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let next = cur - 1;
        env.set(name, &next.to_string());
        return next;
    }
    if let Some(name) = expr.strip_suffix("++") {
        let name = name.trim();
        let cur = env
            .get(name)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        env.set(name, &(cur + 1).to_string());
        return cur;
    }
    if let Some(name) = expr.strip_suffix("--") {
        let name = name.trim();
        let cur = env
            .get(name)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        env.set(name, &(cur - 1).to_string());
        return cur;
    }
    let chars: Vec<char> = expr.chars().collect();
    let len = chars.len();
    for i in 0..len {
        let ch = chars[i];
        let is_assign = match ch {
            '=' => {
                let prev = if i > 0 { chars[i - 1] } else { ' ' };
                prev != '='
                    && prev != '!'
                    && prev != '<'
                    && prev != '>'
                    && chars.get(i + 1) != Some(&'=')
            }
            '+' | '-' | '%' => chars.get(i + 1) == Some(&'='),
            _ => false,
        };
        if !is_assign {
            continue;
        }
        let name = expr[..i].trim();
        let value = if ch == '=' {
            expr[i + 1..].trim()
        } else {
            expr[i + 2..].trim()
        };
        if name.is_empty() {
            return 0;
        }
        let result = match ch {
            '=' => eval_arithmetic(value, env),
            op => {
                let current = env
                    .get(name)
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let rhs = eval_arithmetic(value, env);
                match op {
                    '+' => current + rhs,
                    '-' => current - rhs,
                    '*' => current * rhs,
                    '/' => {
                        if rhs != 0 {
                            current / rhs
                        } else {
                            0
                        }
                    }
                    '%' => {
                        if rhs != 0 {
                            current % rhs
                        } else {
                            0
                        }
                    }
                    _ => current,
                }
            }
        };
        env.set(name, &result.to_string());
        return result;
    }
    eval_arithmetic(expr, env)
}

fn cmd_trap(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        let traps = env.all_traps();
        if traps.is_empty() {
            return BuiltinResult::ok();
        }
        let mut entries: Vec<_> = traps.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (signal, command) in entries {
            if command.is_empty() {
                println!("trap -- '' {}", signal);
            } else {
                println!("trap -- '{}' {}", command, signal);
            }
        }
        return BuiltinResult::ok();
    }

    if args[0] == "-l" {
        list_signals();
        return BuiltinResult::ok();
    }

    if args[0] == "-p" {
        if args.len() < 2 {
            let traps = env.all_traps();
            let mut entries: Vec<_> = traps.iter().collect();
            entries.sort_by_key(|(k, _)| (*k).clone());
            for (signal, command) in entries {
                if command.is_empty() {
                    println!("trap -- '' {}", signal);
                } else {
                    println!("trap -- '{}' {}", command, signal);
                }
            }
            return BuiltinResult::ok();
        }
        let signal = &args[1];
        if let Some(command) = env.get_trap(signal) {
            if command.is_empty() {
                println!("trap -- '' {}", signal);
            } else {
                println!("trap -- '{}' {}", command, signal);
            }
        } else {
            println!("trap -- '' {}", signal);
        }
        return BuiltinResult::ok();
    }

    if args.len() == 1 {
        let sig = &args[0];
        let is_signal = signal_names().iter().any(|s| s.eq_ignore_ascii_case(sig))
            || matches!(sig.as_str(), "ERR" | "DEBUG" | "RETURN");
        if is_signal {
            env.remove_trap(sig);
            return BuiltinResult::ok();
        }
        eprintln!("context: trap: signal argument required");
        return BuiltinResult::err(1);
    }

    let command = &args[0];
    let signal = &args[1];

    if command == "-" {
        env.remove_trap(signal);
        if let Some(sig_num) = crate::shell::signals::signal_name_to_number(signal) {
            crate::shell::signals::restore_signal_default(sig_num);
        }
    } else {
        let command = command.trim_matches(|c| c == '\'' || c == '"');
        env.set_trap(signal, command);
        if command.is_empty()
            && let Some(sig_num) = crate::shell::signals::signal_name_to_number(signal)
        {
            crate::shell::signals::ignore_signal_trapped(sig_num);
        }
    }
    BuiltinResult::ok()
}

fn cmd_pushd(args: &[String], env: &mut Env) -> BuiltinResult {
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let mut stack: Vec<String> = dirs_str
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut swap = false;
    let mut target: Option<String> = None;
    let mut no_chdir = false;
    let mut had_positional = false;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--" {
            i += 1;
            if i < args.len() {
                target = Some(if args[i] == "~" {
                    env.home()
                } else if let Some(rest) = args[i].strip_prefix("~/") {
                    format!("{}/{}", env.home(), rest)
                } else {
                    args[i].clone()
                });
                had_positional = true;
                i += 1;
            }
            continue;
        }
        if args[i] == "-n" {
            no_chdir = true;
            i += 1;
            continue;
        }
        if args[i].starts_with('-')
            && args[i].len() > 1
            && args[i][1..].chars().all(|c| c.is_ascii_digit())
        {
            let n: usize = args[i][1..].parse().unwrap_or(0);
            stack.insert(0, cwd.clone());
            if n > 0 && stack.len() > 1 {
                let idx = stack.len() - 1 - n.min(stack.len() - 1);
                let val = stack.remove(idx);
                stack.insert(0, val);
            }
            env.set_dirstack(&stack);
            if let Some(dir) = stack.first()
                && !no_chdir
                && let Err(e) = std::env::set_current_dir(dir)
            {
                eprintln!("context: pushd: {}: {}", dir, e);
                return BuiltinResult::err(1);
            }
            return BuiltinResult::ok();
        }
        if args[i] == "+n" || (args[i].starts_with('+') && args[i].len() > 1) {
            let n: usize = args[i][1..].parse().unwrap_or(0);
            stack.insert(0, cwd.clone());
            if n < stack.len() {
                let val = stack.remove(n);
                stack.insert(0, val);
            }
            env.set_dirstack(&stack);
            had_positional = true;
            i += 1;
            continue;
        }
        target = Some(if args[i] == "~" {
            env.home()
        } else if let Some(rest) = args[i].strip_prefix("~/") {
            format!("{}/{}", env.home(), rest)
        } else {
            args[i].clone()
        });
        had_positional = true;
        i += 1;
    }

    if target.is_none() && !had_positional {
        swap = true;
    }

    if swap {
        if stack.is_empty() {
            eprintln!("context: pushd: no other directory yet");
            return BuiltinResult::err(1);
        }
        let top = stack.remove(0);
        stack.insert(0, cwd.clone());
        stack.insert(0, top.clone());
        env.set_dirstack(&stack);
        if !no_chdir && let Err(e) = std::env::set_current_dir(&top) {
            eprintln!("context: pushd: {}: {}", top, e);
            return BuiltinResult::err(1);
        }
        return BuiltinResult::ok();
    }

    if let Some(dir) = target {
        stack.insert(0, cwd.clone());
        env.set_dirstack(&stack);
        if !no_chdir {
            if let Err(e) = std::env::set_current_dir(&dir) {
                eprintln!("context: pushd: {}: {}", dir, e);
                return BuiltinResult::err(1);
            }
            if let Ok(cwd) = std::env::current_dir() {
                println!("{}", cwd.display());
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_popd(args: &[String], env: &mut Env) -> BuiltinResult {
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let mut stack: Vec<String> = dirs_str
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    if args.is_empty() {
        match stack.pop() {
            Some(dir) => {
                env.set_dirstack(&stack);
                if let Err(e) = std::env::set_current_dir(&dir) {
                    eprintln!("context: popd: {}: {}", dir, e);
                    return BuiltinResult::err(1);
                }
                if let Ok(cwd) = std::env::current_dir() {
                    println!("{}", cwd.display());
                }
                BuiltinResult::ok()
            }
            None => {
                eprintln!("context: popd: directory stack empty");
                BuiltinResult::err(1)
            }
        }
    } else {
        let mut i = 0;
        let mut no_chdir = false;
        let mut popped = false;
        while i < args.len() {
            if args[i] == "--" {
                i += 1;
                continue;
            }
            if args[i] == "-n" {
                no_chdir = true;
                i += 1;
                continue;
            }
            if args[i] == "+n" || (args[i].starts_with('+') && args[i].len() > 1) {
                popped = true;
                let n: usize = args[i][1..].parse().unwrap_or(0);
                if n < stack.len() {
                    let dir = stack.remove(n);
                    if !no_chdir && let Err(e) = std::env::set_current_dir(&dir) {
                        eprintln!("context: popd: {}: {}", dir, e);
                        return BuiltinResult::err(1);
                    }
                } else {
                    eprintln!("context: popd: {} not in directory stack", args[i]);
                    return BuiltinResult::err(1);
                }
            } else {
                eprintln!("context: popd: unknown option: {}", args[i]);
                return BuiltinResult::err(1);
            }
            i += 1;
        }
        if !popped {
            if let Some(dir) = stack.pop() {
                if !no_chdir && let Err(e) = std::env::set_current_dir(&dir) {
                    eprintln!("context: popd: {}: {}", dir, e);
                    return BuiltinResult::err(1);
                }
            } else {
                eprintln!("context: popd: directory stack empty");
                return BuiltinResult::err(1);
            }
        }
        env.set_dirstack(&stack);
        if let Ok(cwd) = std::env::current_dir() {
            println!("{}", cwd.display());
        }
        BuiltinResult::ok()
    }
}

fn cmd_dirs(args: &[String], env: &mut Env) -> BuiltinResult {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let home = env.home();
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let stack: Vec<&str> = dirs_str.split('\n').filter(|s| !s.is_empty()).collect();
    let mut numbered = false;
    let mut long_paths = false;
    let mut one_per_line = false;
    let mut show_named = false;
    let mut clear_named = false;

    for arg in args {
        match arg.as_str() {
            "-v" => numbered = true,
            "-l" => long_paths = true,
            "-p" => one_per_line = true,
            "-n" => show_named = true,
            "-c" => clear_named = true,
            "-" => {
                long_paths = false;
                numbered = false;
                one_per_line = false;
            }
            _ => {}
        }
    }

    if clear_named {
        env.set_dirstack(&[]);
        return BuiltinResult::ok();
    }

    if show_named {
        let dirs = env.all_named_dirs();
        let mut entries: Vec<(String, String)> =
            dirs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, path) in &entries {
            println!("{}={}", name, path);
        }
        return BuiltinResult::ok();
    }

    let mut entries: Vec<String> = Vec::new();
    let mut numbered_entries: Vec<(usize, String)> = Vec::new();

    if long_paths {
        entries.push(cwd.clone());
        numbered_entries.push((0, cwd.clone()));
        for (i, d) in stack.iter().enumerate() {
            entries.push(d.to_string());
            numbered_entries.push((i + 1, d.to_string()));
        }
    } else {
        let short = cwd.replacen(&home, "~", 1);
        entries.push(short.clone());
        numbered_entries.push((0, short));
        for (i, d) in stack.iter().enumerate() {
            let short = d.replacen(&home, "~", 1);
            entries.push(short.clone());
            numbered_entries.push((i + 1, short));
        }
    }

    if numbered {
        let output: Vec<String> = numbered_entries
            .iter()
            .map(|(i, d)| format!("{} {}", i, d))
            .collect();
        if one_per_line {
            println!("{}", output.join("\n"));
        } else {
            println!("{}", output.join(" "));
        }
    } else if one_per_line {
        println!("{}", entries.join("\n"));
    } else {
        println!("{}", entries.join(" "));
    }
    BuiltinResult::ok()
}

fn cmd_hash(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        let cache = PATH_CACHE.lock().expect("PATH_CACHE lock");
        if cache.is_empty() {
            return BuiltinResult::ok();
        }
        let mut entries: Vec<_> = cache.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (name, path) in entries {
            println!("{}={}", name, path);
        }
        return BuiltinResult::ok();
    }
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-r" {
            PATH_CACHE.lock().expect("PATH_CACHE lock").clear();
            i += 1;
        } else if args[i] == "-p" {
            i += 1;
            if i < args.len() {
                let pathname = &args[i];
                let name = Path::new(pathname)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| pathname.to_string());
                PATH_CACHE
                    .lock()
                    .expect("PATH_CACHE lock")
                    .insert(name, pathname.to_string());
                i += 1;
            }
        } else if args[i] == "-d" {
            i += 1;
            while i < args.len() && !args[i].starts_with('-') {
                PATH_CACHE.lock().expect("PATH_CACHE lock").remove(&args[i]);
                i += 1;
            }
        } else {
            let name = &args[i];
            if let Some(path) = find_in_path(name) {
                PATH_CACHE
                    .lock()
                    .expect("PATH_CACHE lock")
                    .insert(name.to_string(), path);
            } else {
                eprintln!("context: hash: {}: not found", name);
            }
            i += 1;
        }
    }
    BuiltinResult::ok()
}

fn cmd_math(args: &[String], env: &mut Env, cfg: &Config) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: math: expression required");
        return BuiltinResult::err(1);
    }
    let expr = args.join(" ");
    let expr = expand_env_in_math(&expr, env);
    match eval_float(&expr) {
        Ok(val) => {
            let precision = cfg.execution.float_precision;
            let formatted = format!("{:.prec$}", val, prec = precision);
            let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
            println!("{}", formatted);
            env.set("_", formatted);
        }
        Err(e) => {
            eprintln!("context: math: {}", e);
            return BuiltinResult::err(1);
        }
    }
    BuiltinResult::ok()
}

fn expand_env_in_math(expr: &str, env: &Env) -> String {
    let mut result = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut name = String::new();
            name.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(val) = env.get(&name) {
                result.push_str(val);
            } else {
                result.push_str(&name);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn eval_float(expr: &str) -> Result<f64, String> {
    let mut parser = FloatParser::new(expr);
    let val = parser.parse_add_sub()?;
    Ok(val)
}

struct FloatParser {
    chars: Vec<char>,
    pos: usize,
}

impl FloatParser {
    fn new(expr: &str) -> Self {
        Self {
            chars: expr.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_add_sub(&mut self) -> Result<f64, String> {
        let mut result = self.parse_mul_div()?;
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '+' || op == '-' {
                self.advance();
                let rhs = self.parse_mul_div()?;
                if op == '+' {
                    result += rhs;
                } else {
                    result -= rhs;
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        Ok(result)
    }

    fn parse_mul_div(&mut self) -> Result<f64, String> {
        let mut result = self.parse_unary()?;
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '*' || op == '/' || op == '%' {
                self.advance();
                let rhs = self.parse_unary()?;
                match op {
                    '*' => result *= rhs,
                    '/' => {
                        if rhs == 0.0 {
                            return Err("division by zero".into());
                        }
                        result /= rhs;
                    }
                    '%' => {
                        if rhs == 0.0 {
                            return Err("division by zero".into());
                        }
                        result %= rhs;
                    }
                    _ => {}
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        Ok(result)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        if let Some(ch) = self.peek() {
            if ch == '+' {
                self.advance();
                return self.parse_unary();
            }
            if ch == '-' {
                self.advance();
                return self.parse_unary().map(|v| -v);
            }
            if ch == '(' {
                self.advance();
                let val = self.parse_add_sub()?;
                self.skip_whitespace();
                if self.peek() == Some(')') {
                    self.advance();
                }
                return Ok(val);
            }
        }
        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit()
                || ch == '.'
                || ch == 'e'
                || ch == 'E'
                || ch == '+'
                    && (self.chars.get(self.pos.wrapping_sub(1)) == Some(&'e')
                        || self.chars.get(self.pos.wrapping_sub(1)) == Some(&'E'))
                || ch == '-'
                    && (self.chars.get(self.pos.wrapping_sub(1)) == Some(&'e')
                        || self.chars.get(self.pos.wrapping_sub(1)) == Some(&'E'))
            {
                self.advance();
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(format!("unexpected character: {:?}", self.peek()));
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f64>().map_err(|e| format!("parse error: {}", e))
    }
}

fn cmd_regexmatch(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.len() < 2 {
        eprintln!("context: regexmatch: usage: regexmatch string pattern [capture_var ...]");
        return BuiltinResult::err(1);
    }
    let string = &args[0];
    let pattern = &args[1];
    let rest = &args[2..];
    match regex::Regex::new(pattern) {
        Ok(re) => {
            if let Some(caps) = re.captures(string) {
                for (i, m) in caps.iter().enumerate() {
                    if let Some(mat) = m {
                        let var_name = if i < rest.len() {
                            rest[i].as_str()
                        } else {
                            &format!("MATCH_{}", i)
                        };
                        env.set(var_name, mat.as_str());
                    }
                }
                env.set("MATCH", string);
                BuiltinResult::ok()
            } else {
                BuiltinResult::err(1)
            }
        }
        Err(e) => {
            eprintln!("context: regexmatch: invalid pattern: {}", e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_module(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: module: usage: module load|unload|list|info <name>");
        return BuiltinResult::err(1);
    }
    let modules_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".config"))
        .join("context/modules");
    let _ = std::fs::create_dir_all(&modules_dir);

    match args[0].as_str() {
        "list" => {
            match std::fs::read_dir(&modules_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let init = entry.path().join("init.context");
                        let status = if init.exists() { "loaded" } else { "no init" };
                        println!("{} ({})", name, status);
                    }
                }
                Err(_) => println!("No modules directory found"),
            }
            BuiltinResult::ok()
        }
        "load" => {
            if args.len() < 2 {
                eprintln!("context: module: load requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let init_path = modules_dir.join(name).join("init.context");
            if !init_path.exists() {
                eprintln!("context: module: {}: not found", name);
                return BuiltinResult::err(1);
            }
            match std::fs::read_to_string(&init_path) {
                Ok(contents) => {
                    env.set("_LOADED_MODULE", name);
                    let module_env = env.clone();
                    let tokens = crate::shell::lexer::tokenize(&contents);
                    let ast = crate::shell::parser::parse(tokens);
                    crate::shell::executor::Executor::new(
                        module_env.clone(),
                        crate::config::Config::default(),
                    )
                    .execute(&ast);
                    env.merge_from(&module_env);
                    println!("Module {} loaded", name);
                    BuiltinResult::ok()
                }
                Err(e) => {
                    eprintln!("context: module: {}: {}", init_path.display(), e);
                    BuiltinResult::err(1)
                }
            }
        }
        "unload" => {
            if args.len() < 2 {
                eprintln!("context: module: unload requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let fini_path = modules_dir.join(name).join("fini.context");
            if fini_path.exists()
                && let Ok(contents) = std::fs::read_to_string(&fini_path)
            {
                let tokens = crate::shell::lexer::tokenize(&contents);
                let ast = crate::shell::parser::parse(tokens);
                crate::shell::executor::Executor::new(
                    env.clone(),
                    crate::config::Config::default(),
                )
                .execute(&ast);
            }
            println!("Module {} unloaded", name);
            BuiltinResult::ok()
        }
        "info" => {
            if args.len() < 2 {
                eprintln!("context: module: info requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let module_dir = modules_dir.join(name);
            if !module_dir.exists() {
                eprintln!("context: module: {}: not found", name);
                return BuiltinResult::err(1);
            }
            let desc_path = module_dir.join("README");
            if desc_path.exists() {
                if let Ok(desc) = std::fs::read_to_string(&desc_path) {
                    println!("{}", desc);
                }
            } else {
                println!("Module: {}", name);
                let init = module_dir.join("init.context");
                let fini = module_dir.join("fini.context");
                println!(
                    "  init.context: {}",
                    if init.exists() { "yes" } else { "no" }
                );
                println!(
                    "  fini.context: {}",
                    if fini.exists() { "yes" } else { "no" }
                );
            }
            BuiltinResult::ok()
        }
        _ => {
            eprintln!("context: module: unknown subcommand: {}", args[0]);
            eprintln!("context: module: usage: load|unload|list|info <name>");
            BuiltinResult::err(1)
        }
    }
}

fn cmd_readonly(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_vars() {
            if env.is_readonly(k) {
                println!("readonly {}={}", k, v);
            }
        }
        return BuiltinResult::ok();
    }
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set(name, value);
            env.set_readonly(name);
        } else {
            env.set_readonly(arg);
        }
    }
    BuiltinResult::ok()
}

fn cmd_declare(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut export = false;
    let mut readonly = false;
    let mut assoc = false;
    let mut integer = false;
    let mut array = false;
    let mut lowercase = false;
    let mut uppercase = false;
    let mut nameref = false;
    let mut global = false;
    let mut print_mode = false;
    let mut print_function = false;
    let mut print_names: Vec<String> = Vec::new();
    let mut vars = Vec::new();
    let mut no_flags = true;
    for arg in args {
        if arg == "-x" || arg == "--export" {
            export = true;
            no_flags = false;
        } else if arg == "-r" || arg == "--readonly" {
            readonly = true;
            no_flags = false;
        } else if arg == "-A" || arg == "--assoc" {
            assoc = true;
            no_flags = false;
        } else if arg == "-a" {
            array = true;
            no_flags = false;
        } else if arg == "-i" {
            integer = true;
            no_flags = false;
        } else if arg == "-l" || arg == "--lowercase" {
            lowercase = true;
            no_flags = false;
        } else if arg == "-u" || arg == "--uppercase" {
            uppercase = true;
            no_flags = false;
        } else if arg == "-n" || arg == "--nameref" {
            nameref = true;
            no_flags = false;
        } else if arg == "-g" || arg == "--global" {
            global = true;
            no_flags = false;
        } else if arg == "-t" {
            no_flags = false;
        } else if arg == "-p" {
            print_mode = true;
            no_flags = false;
        } else if arg == "-f" {
            print_function = true;
            no_flags = false;
        } else {
            if print_mode {
                print_names.push(arg.clone());
            } else {
                vars.push(arg.as_str());
            }
        }
    }
    if print_mode {
        for name in &print_names {
            if env.is_assoc_array(name) {
                let pairs = env.assoc_pairs(name);
                let mut flags = String::new();
                if env.is_exported(name) {
                    flags.push_str("-x ");
                }
                if env.is_readonly(name) {
                    flags.push_str("-r ");
                }
                flags.push_str("-A ");
                if flags.is_empty() {
                    flags.push_str("-- ");
                }
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("[{}]=\"{}\"", k, v.replace('"', "\\\"")))
                    .collect();
                println!("declare {}{}=({})", flags, name, inner.join(" "));
            } else if env.is_indexed_array(name) {
                let len = env.indexed_array_len(name);
                let mut flags = String::new();
                if env.is_exported(name) {
                    flags.push_str("-x ");
                }
                if env.is_readonly(name) {
                    flags.push_str("-r ");
                }
                flags.push_str("-a ");
                if flags.is_empty() {
                    flags.push_str("-- ");
                }
                let inner: Vec<String> = (0..len)
                    .filter_map(|i| {
                        env.indexed_array_get(name, &i.to_string())
                            .map(|v| format!("[{}]=\"{}\"", i, v.replace('"', "\\\"")))
                    })
                    .collect();
                println!("declare {}{}=({})", flags, name, inner.join(" "));
            } else if let Some(val) = env.get(name) {
                let mut flags = String::new();
                if env.is_exported(name) {
                    flags.push_str("-x ");
                }
                if env.is_readonly(name) {
                    flags.push_str("-r ");
                }
                if env
                    .get(&format!("_OPT_{}", name))
                    .map(|s| s == "1")
                    .unwrap_or(false)
                {
                    flags.push_str("-i ");
                }
                if flags.is_empty() {
                    flags.push_str("-- ");
                }
                let display_val = maybe_mask_output(&format!("{}={}", name, val));
                let masked_part = display_val
                    .strip_prefix(&format!("{}=", name))
                    .unwrap_or(val);
                println!("declare {}{}={}", flags, name, masked_part);
            } else {
                eprintln!("context: declare: {}: not found", name);
                return BuiltinResult::err(1);
            }
        }
        return BuiltinResult::ok();
    }
    if print_function {
        if let Some(ref cb) = GET_FUNCTION_CB.get().and_then(|m| m.lock().ok())
            && let Some(ref func) = **cb
        {
            if vars.is_empty() {
                eprintln!("context: typeset: -f requires a function name");
                return BuiltinResult::err(1);
            }
            for name in &vars {
                if let Some(body) = func(name) {
                    println!("{}", body);
                } else {
                    eprintln!("context: typeset: {}: not a function", name);
                    return BuiltinResult::err(1);
                }
            }
            return BuiltinResult::ok();
        }
        eprintln!("context: typeset: -f not supported in this context");
        return BuiltinResult::err(1);
    }
    if no_flags && vars.is_empty() {
        for (k, v) in env.all_vars() {
            let display = maybe_mask_output(&format!("{}={}", k, v));
            if env.is_exported(k) {
                println!("declare -x {}", display);
            } else {
                println!("declare -- {}", display);
            }
        }
        return BuiltinResult::ok();
    }
    for var in vars {
        if assoc && !var.contains('=') {
            env.create_assoc_array(var);
            continue;
        }
        if let Some(eq_pos) = var.find('=') {
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];
            if assoc
                && name.contains('[')
                && let Some(bracket_pos) = name.find('[')
            {
                let arr_name = &name[..bracket_pos];
                let key = &name[bracket_pos + 1..].trim_end_matches(']');
                env.create_assoc_array(arr_name);
                env.assoc_set(arr_name, key, value);
                continue;
            }
            if assoc && value.starts_with('(') && value.ends_with(')') {
                let inner = &value[1..value.len() - 1];
                env.create_assoc_array(name);
                let mut current_key = String::new();
                let mut current_val = String::new();
                let mut in_key = false;
                let mut in_val = false;
                let chars: Vec<char> = inner.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    match chars[i] {
                        '[' if !in_val => {
                            in_key = true;
                            current_key.clear();
                        }
                        ']' if in_key => {
                            in_key = false;
                            if i + 1 < chars.len() && chars[i + 1] == '=' {
                                i += 1;
                            }
                            in_val = true;
                            current_val.clear();
                        }
                        '=' if in_val && current_val.is_empty() => {}
                        ' ' | '\t' | '\n' | '\r' if in_val && !current_val.is_empty() => {
                            env.assoc_set(name, &current_key, &current_val);
                            in_val = false;
                        }
                        _ if in_key => {
                            current_key.push(chars[i]);
                        }
                        _ if in_val => {
                            current_val.push(chars[i]);
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if in_val && !current_key.is_empty() {
                    env.assoc_set(name, &current_key, &current_val);
                }
                continue;
            }
            if array && value.starts_with('(') && value.ends_with(')') {
                let inner = &value[1..value.len() - 1];
                let mut idx = 0;
                let mut current = String::new();
                let mut in_quote = false;
                let mut quote_char = '"';
                let mut chars_iter = inner.chars().peekable();
                while let Some(c) = chars_iter.next() {
                    match c {
                        '"' | '\'' if !in_quote => {
                            in_quote = true;
                            quote_char = c;
                        }
                        '"' | '\'' if in_quote && c == quote_char => {
                            in_quote = false;
                        }
                        ' ' | '\t' if !in_quote => {
                            if !current.is_empty() {
                                env.indexed_array_set(name, &idx.to_string(), &current);
                                idx += 1;
                                current.clear();
                            }
                        }
                        '\\' if in_quote => {
                            if let Some(next) = chars_iter.next() {
                                current.push(next);
                            }
                        }
                        _ => {
                            current.push(c);
                        }
                    }
                }
                if !current.is_empty() {
                    env.indexed_array_set(name, &idx.to_string(), &current);
                }
                env.set(&format!("{}_@", name), &idx.to_string());
            } else if array && name.contains('[') && name.ends_with(']') {
                if let Some(bracket_pos) = name.find('[') {
                    let arr_name = &name[..bracket_pos];
                    let idx_str = &name[bracket_pos + 1..].trim_end_matches(']');
                    env.indexed_array_set(arr_name, idx_str, value);
                }
            } else {
                if nameref {
                    env.set_var_attrs(
                        name,
                        crate::shell::env::VarAttrs {
                            nameref: Some(value.to_string()),
                            ..Default::default()
                        },
                    );
                }
                if global {
                    env.set_global(name, value);
                } else {
                    env.set_exported(name, value, export);
                }
            }
            if readonly {
                env.set_readonly(name);
            }
            if integer {
                env.set(&format!("_OPT_{}", name), "1");
                let v = eval_arith_assign(value, env);
                env.set(name, &v.to_string());
            }
            if array {
                env.set(&format!("_OPT_{}", name), "1");
            }
            if lowercase || uppercase {
                let mut attrs = env.get_var_attrs(name);
                attrs.lowercase = lowercase;
                attrs.uppercase = uppercase;
                env.set_var_attrs(name, attrs);
            }
        } else if assoc {
            env.create_assoc_array(var);
        } else if array {
            env.export(var);
            env.set(&format!("_OPT_{}", var), "1");
        } else {
            env.export(var);
            if readonly {
                env.set_readonly(var);
            }
            if integer {
                env.set(&format!("_OPT_{}", var), "1");
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_wait(args: &[String]) -> BuiltinResult {
    let mut next_child = false;
    let mut pid_args: Vec<&String> = Vec::new();
    for arg in args {
        if arg == "-n" {
            next_child = true;
        } else {
            pid_args.push(arg);
        }
    }
    if pid_args.is_empty() {
        if next_child {
            // `wait -n` blocks until any one child completes.
            let mut status: i32 = 0;
            loop {
                let ret = unsafe { libc::waitpid(-1, &mut status, 0) };
                if ret > 0 {
                    let st = if libc::WIFEXITED(status) {
                        libc::WEXITSTATUS(status)
                    } else if libc::WIFSIGNALED(status) {
                        128 + libc::WTERMSIG(status)
                    } else if libc::WIFSTOPPED(status) {
                        128 + libc::WSTOPSIG(status)
                    } else {
                        1
                    };
                    return BuiltinResult::err(st);
                }
                let err = std::io::Error::last_os_error().raw_os_error();
                if err != Some(libc::EINTR) {
                    break;
                }
            }
            return BuiltinResult::ok();
        }
        let mut last_status = 0;
        loop {
            let mut status: i32 = 0;
            let ret = unsafe { libc::waitpid(-1, &mut status, 0) };
            if ret <= 0 {
                break;
            }
            last_status = if libc::WIFEXITED(status) {
                libc::WEXITSTATUS(status)
            } else if libc::WIFSIGNALED(status) {
                128 + libc::WTERMSIG(status)
            } else if libc::WIFSTOPPED(status) {
                128 + libc::WSTOPSIG(status)
            } else {
                1
            };
        }
        return BuiltinResult::err(last_status);
    }
    let mut last_status = 0;
    for arg in pid_args {
        if let Ok(pid) = arg.parse::<i32>() {
            let mut status: i32 = 0;
            let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
            if ret == -1 {
                eprintln!("context: wait: {}: no such child", pid);
                return BuiltinResult::err(127);
            }
            if ret > 0 {
                last_status = if libc::WIFEXITED(status) {
                    libc::WEXITSTATUS(status)
                } else if libc::WIFSIGNALED(status) {
                    128 + libc::WTERMSIG(status)
                } else if libc::WIFSTOPPED(status) {
                    128 + libc::WSTOPSIG(status)
                } else {
                    1
                };
            }
        } else {
            eprintln!("context: wait: {}: invalid pid", arg);
            return BuiltinResult::err(127);
        }
    }
    BuiltinResult::err(last_status)
}

fn signal_names() -> Vec<&'static str> {
    vec![
        "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1", "SEGV", "USR2",
        "PIPE", "ALRM", "TERM", "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU", "URG",
        "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO", "PWR", "SYS",
    ]
}

fn list_signals() {
    let signals: Vec<(i32, &str)> = signal_names()
        .into_iter()
        .map(|name| {
            let num = match name {
                "HUP" => 1,
                "INT" => 2,
                "QUIT" => 3,
                "ILL" => 4,
                "TRAP" => 5,
                "ABRT" => 6,
                "BUS" => 7,
                "FPE" => 8,
                "KILL" => 9,
                "USR1" => 10,
                "SEGV" => 11,
                "USR2" => 12,
                "PIPE" => 13,
                "ALRM" => 14,
                "TERM" => 15,
                "STKFLT" => 16,
                "CHLD" => 17,
                "CONT" => 18,
                "STOP" => 19,
                "TSTP" => 20,
                "TTIN" => 21,
                "TTOU" => 22,
                "URG" => 23,
                "XCPU" => 24,
                "XFSZ" => 25,
                "VTALRM" => 26,
                "PROF" => 27,
                "WINCH" => 28,
                "IO" => 29,
                "PWR" => 30,
                "SYS" => 31,
                _ => 0,
            };
            (num, name)
        })
        .collect();
    let mut line = String::new();
    for (num, name) in &signals {
        let entry = format!("{:>2}) {}", num, name);
        if line.is_empty() {
            line = entry;
        } else if line.len() + entry.len() + 1 >= 60 {
            println!("{}", line);
            line = entry;
        } else {
            line.push(' ');
            line.push_str(&entry);
        }
    }
    if !line.is_empty() {
        println!("{}", line);
    }
}

fn cmd_kill(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!(
            "context: kill: usage: kill [-s SIGSPEC | -n SIGNUM | -SIGSPEC] pid | jobspec ..."
        );
        return BuiltinResult::err(1);
    }
    if args[0] == "-l" {
        list_signals();
        return BuiltinResult::ok();
    }
    let mut signal: i32 = libc::SIGTERM;
    let mut i = 0;
    if args.len() >= 3 && (args[0] == "-s" || args[0] == "-n") {
        let sigarg = &args[1];
        signal = match crate::shell::signals::signal_name_to_number(sigarg) {
            Some(n) => n,
            None => {
                if let Ok(n) = sigarg.parse::<i32>() {
                    n
                } else {
                    eprintln!("context: kill: {}: invalid signal specification", sigarg);
                    return BuiltinResult::err(1);
                }
            }
        };
        i = 2;
    } else if args[0].starts_with('-')
        && !args[0]
            .chars()
            .nth(1)
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
    {
        let sig = &args[0][1..];
        signal = match crate::shell::signals::signal_name_to_number(sig) {
            Some(n) => n,
            None => {
                if let Ok(n) = sig.parse::<i32>() {
                    n
                } else {
                    eprintln!("context: kill: {}: invalid signal specification", sig);
                    return BuiltinResult::err(1);
                }
            }
        };
        i = 1;
    } else if args[0].starts_with('-')
        && let Ok(n) = args[0][1..].parse::<i32>()
    {
        signal = n;
        i = 1;
    }
    if i >= args.len() {
        eprintln!(
            "context: kill: usage: kill [-s SIGSPEC | -n SIGNUM | -SIGSPEC] pid | jobspec ..."
        );
        return BuiltinResult::err(1);
    }
    let mut status = 0;
    for arg in &args[i..] {
        let pid = if let Some(pct) = arg.strip_prefix('%') {
            if let Ok(job_id) = pct.parse::<usize>() {
                eprintln!("context: kill: job {} not found", job_id);
                status = 1;
                continue;
            } else {
                eprintln!("context: kill: {}: invalid pid", arg);
                status = 1;
                continue;
            }
        } else {
            arg.parse::<i32>().unwrap_or(-1)
        };
        if pid < 0 {
            eprintln!("context: kill: {}: invalid pid", arg);
            status = 1;
            continue;
        }
        if signal == 0 {
            let ret = unsafe { libc::kill(pid, 0) };
            if ret == -1 {
                status = 1;
            }
        } else {
            let ret = unsafe { libc::kill(pid, signal) };
            if ret == -1 {
                eprintln!(
                    "context: kill: ({}) - {}",
                    pid,
                    std::io::Error::last_os_error()
                );
                status = 1;
            }
        }
    }
    BuiltinResult::err(status)
}

fn cmd_umask(args: &[String]) -> BuiltinResult {
    let mut symbolic_output = false;
    let mut mask_args: Vec<&String> = Vec::new();
    for arg in args {
        if arg == "-S" {
            symbolic_output = true;
        } else {
            mask_args.push(arg);
        }
    }
    if mask_args.is_empty() {
        let mask = unsafe { libc::umask(0) };
        unsafe {
            libc::umask(mask);
        }
        if symbolic_output {
            println!("{}", format_umask_symbolic(mask as u32));
        } else {
            println!("{:04o}", mask);
        }
        return BuiltinResult::ok();
    }
    if mask_args.len() > 1 {
        eprintln!("context: umask: too many arguments");
        return BuiltinResult::err(2);
    }
    let mask_str = mask_args[0];
    let is_symbolic = mask_str.contains('+')
        || mask_str.contains('-')
        || mask_str.starts_with("u=")
        || mask_str.starts_with("g=")
        || mask_str.starts_with("o=")
        || mask_str.starts_with("a=");
    if is_symbolic {
        match parse_umask_symbolic(mask_str) {
            Ok(mask) => {
                unsafe {
                    libc::umask(mask as libc::mode_t);
                }
                BuiltinResult::ok()
            }
            Err(e) => {
                eprintln!("context: umask: {}", e);
                BuiltinResult::err(2)
            }
        }
    } else {
        match u32::from_str_radix(mask_str, 8) {
            Ok(mask) => {
                unsafe {
                    libc::umask(mask as libc::mode_t);
                }
                BuiltinResult::ok()
            }
            Err(_) => {
                eprintln!("context: umask: {}: invalid octal number", mask_str);
                BuiltinResult::err(2)
            }
        }
    }
}

fn format_umask_symbolic(mask: u32) -> String {
    let u = format_perms(0o7 & !(mask >> 6));
    let g = format_perms(0o7 & !(mask >> 3));
    let o = format_perms(0o7 & !mask);
    format!("u={},g={},o={}", u, g, o)
}

fn format_perms(perms: u32) -> String {
    let mut s = String::new();
    if perms & 0o4 != 0 {
        s.push('r');
    }
    if perms & 0o2 != 0 {
        s.push('w');
    }
    if perms & 0o1 != 0 {
        s.push('x');
    }
    s
}

fn parse_umask_symbolic(s: &str) -> Result<u32, String> {
    let mut user_mask: u32 = 0;
    let mut group_mask: u32 = 0;
    let mut other_mask: u32 = 0;
    let mut user_set = false;
    let mut group_set = false;
    let mut other_set = false;

    for clause in s.split(',') {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }

        let op_pos = clause
            .find(['+', '-', '='])
            .ok_or_else(|| format!("invalid symbolic mode: {}", clause))?;

        let who_str = &clause[..op_pos];
        let op = clause.as_bytes()[op_pos] as char;
        let perms_str = &clause[op_pos + 1..];

        let (sel_u, sel_g, sel_o) = if who_str.is_empty() || who_str == "a" {
            (true, true, true)
        } else {
            let mut su = false;
            let mut sg = false;
            let mut so = false;
            for ch in who_str.chars() {
                match ch {
                    'u' => su = true,
                    'g' => sg = true,
                    'o' => so = true,
                    'a' => {
                        su = true;
                        sg = true;
                        so = true;
                    }
                    _ => return Err(format!("invalid who character: {}", ch)),
                }
            }
            (su, sg, so)
        };

        let mut perms = 0u32;
        for ch in perms_str.chars() {
            match ch {
                'r' => perms |= 0o4,
                'w' => perms |= 0o2,
                'x' => perms |= 0o1,
                _ => return Err(format!("invalid permission: {}", ch)),
            }
        }

        if sel_u {
            user_set = true;
            match op {
                '+' => user_mask &= !perms,
                '-' => user_mask |= perms,
                '=' => user_mask = (!perms) & 0o7,
                _ => return Err(format!("invalid operator: {}", op)),
            }
        }
        if sel_g {
            group_set = true;
            match op {
                '+' => group_mask &= !perms,
                '-' => group_mask |= perms,
                '=' => group_mask = (!perms) & 0o7,
                _ => return Err(format!("invalid operator: {}", op)),
            }
        }
        if sel_o {
            other_set = true;
            match op {
                '+' => other_mask &= !perms,
                '-' => other_mask |= perms,
                '=' => other_mask = (!perms) & 0o7,
                _ => return Err(format!("invalid operator: {}", op)),
            }
        }
    }

    let current = unsafe { libc::umask(0) };
    unsafe {
        libc::umask(current);
    }
    let mut result = current as u32;
    if user_set {
        result = (result & !0o700) | (user_mask << 6);
    }
    if group_set {
        result = (result & !0o070) | (group_mask << 3);
    }
    if other_set {
        result = (result & !0o007) | other_mask;
    }
    Ok(result & 0o777)
}

fn cmd_command(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let mut i = 0;
    let mut use_posix_path = false;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => {
                use_posix_path = true;
                i += 1;
            }
            "-v" => {
                if i + 1 < args.len() {
                    let name = &args[i + 1];
                    if let Some(path) = find_in_path(name) {
                        println!("{}", path);
                    } else {
                        eprintln!("context: command: {}: not found", name);
                        return BuiltinResult::err(1);
                    }
                    return BuiltinResult::ok();
                }
                return BuiltinResult::err(1);
            }
            "-V" => {
                if i + 1 < args.len() {
                    let name = &args[i + 1];
                    let msg = match find_in_path(name) {
                        Some(p) => format!("{} is {}", name, p),
                        None => format!("{}: not found", name),
                    };
                    eprintln!("{}", msg);
                }
                return BuiltinResult::ok();
            }
            _ => break,
        }
    }
    if i >= args.len() {
        return BuiltinResult::ok();
    }
    if use_posix_path {
        let posix_path = unsafe {
            let mut buf = [0u8; 1024];
            let ret = libc::confstr(
                libc::_CS_PATH,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
            );
            if ret > 1 && ret < buf.len() {
                std::ffi::CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                    .to_string_lossy()
                    .trim()
                    .to_string()
            } else {
                "/usr/bin:/bin".to_string()
            }
        };
        let old_path = std::env::var("PATH").ok();
        unsafe {
            std::env::set_var("PATH", posix_path);
        }
        let result = exec_command(&args[i..]);
        if let Some(old) = old_path {
            unsafe {
                std::env::set_var("PATH", &old);
            }
        }
        result
    } else {
        exec_command(&args[i..])
    }
}

fn exec_command(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let cmd = &args[0];
    let path = if cmd.contains('/') {
        cmd.clone()
    } else if let Some(p) = find_in_path(cmd) {
        p
    } else {
        eprintln!("context: command: {}: not found", cmd);
        return BuiltinResult::err(127);
    };
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("context: command: fork failed");
            BuiltinResult::err(1)
        }
        0 => {
            let c_args: Vec<std::ffi::CString> = args
                .iter()
                .filter_map(|w| std::ffi::CString::new(w.as_str()).ok())
                .collect();
            let mut c_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
            c_ptrs.push(std::ptr::null());
            let c_cmd = std::ffi::CString::new(path).unwrap_or_else(|_| {
                std::ffi::CString::new("sh").expect("failed to create CString for sh")
            });
            unsafe {
                libc::execvp(c_cmd.as_ptr(), c_ptrs.as_ptr());
            }
            std::process::exit(126);
        }
        pid => {
            let mut status: i32 = 0;
            unsafe {
                libc::waitpid(pid, &mut status, 0);
            }
            let exit = if libc::WIFEXITED(status) {
                libc::WEXITSTATUS(status)
            } else if libc::WIFSIGNALED(status) {
                128 + libc::WTERMSIG(status)
            } else {
                1
            };
            BuiltinResult::err(exit)
        }
    }
}

fn cmd_select(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: select: usage: select NAME [in WORDS ...;] do COMMANDS; done");
        return BuiltinResult::err(2);
    }
    let var_name = &args[0];
    let items: Vec<String> = if args.len() > 1 && args[1] == "in" {
        args[2..]
            .iter()
            .map(|s| s.trim_end_matches(';').to_string())
            .filter(|s| s != "do" && s != "done" && !s.is_empty())
            .collect()
    } else {
        eprintln!("context: select: missing 'in'");
        return BuiltinResult::err(2);
    };
    if items.is_empty() {
        return BuiltinResult::err(2);
    }
    let stdin = std::io::stdin();
    let use_editor =
        unsafe { libc::isatty(libc::STDIN_FILENO) == 1 } && READLINE_CB.get().is_some();
    loop {
        for (i, item) in items.iter().enumerate() {
            println!("  {}) {}", i + 1, item);
        }
        let ps3 = env.get("PS3").unwrap_or("#? ").to_string();
        eprint!("{}", ps3);
        let _ = std::io::stderr().flush();
        let line = if use_editor {
            match READLINE_CB.get().unwrap()(&ps3) {
                Ok(s) => s,
                Err(_) => break,
            }
        } else {
            let mut buf = String::new();
            match stdin.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_) => buf,
                Err(_) => break,
            }
        };
        let line = line.trim().to_string();
        env.set("REPLY", &line);
        if line.is_empty() {
            continue;
        }
        if let Ok(n) = line.parse::<usize>()
            && n > 0
            && n <= items.len()
        {
            env.set(var_name, &items[n - 1]);
            continue;
        }
        eprintln!("context: select: invalid selection");
    }
    BuiltinResult::ok()
}

fn cmd_getopts(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.len() < 2 {
        eprintln!("context: getopts: usage: getopts OPTSTRING NAME [ARG...]");
        return BuiltinResult::err(2);
    }
    let optstring_raw = &args[0];
    let silent = optstring_raw.starts_with(':');
    let optstring = optstring_raw.trim_start_matches(':');
    let name = &args[1];
    let shell_args: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        env.positional().to_vec()
    };
    let optind: usize = env.get("OPTIND").and_then(|s| s.parse().ok()).unwrap_or(1);
    let offset: usize = env
        .get("_GETOPT_OFFSET")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if optind > shell_args.len() {
        env.set("OPTARG", "");
        env.set("_GETOPT_OFFSET", "0");
        env.set("OPTIND", "1");
        return BuiltinResult::err(1);
    }
    let current = &shell_args[optind - 1];
    if current == "--" {
        env.set("OPTARG", "");
        env.set("OPTIND", &(optind + 1).to_string());
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    if !current.starts_with('-') || current.len() < 2 {
        env.set("OPTARG", "");
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    let start = 1 + offset;
    if start >= current.len() {
        env.set("OPTARG", "");
        env.set("OPTIND", &(optind + 1).to_string());
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    let opt_char = current.as_bytes()[start] as char;
    let remaining = start + 1 < current.len();
    // Unknown option: report via `?` and continue the loop.
    if !optstring.contains(opt_char) {
        if !silent {
            eprintln!("context: getopts: {}: invalid option", opt_char);
        }
        env.set("OPTARG", &opt_char.to_string());
        env.set(name, "?");
        if remaining {
            env.set("_GETOPT_OFFSET", &(offset + 1).to_string());
        } else {
            env.set("OPTIND", &(optind + 1).to_string());
            env.set("_GETOPT_OFFSET", "0");
        }
        return BuiltinResult::ok();
    }

    // Argument requirement: `c:` mandatory, `c::` optional, none = flag.
    let optional_arg = optstring.contains(&format!("{}::", opt_char));
    let mandatory_arg = optstring.contains(&format!("{}:", opt_char)) && !optional_arg;
    env.set(name, &opt_char.to_string());
    if remaining {
        // Argument glued to the option (`-ovalue`) — both `c:` and `c::`.
        if optional_arg || mandatory_arg {
            env.set("OPTARG", &current[(start + 1)..]);
            env.set("OPTIND", &(optind + 1).to_string());
            env.set("_GETOPT_OFFSET", "0");
            BuiltinResult::ok()
        } else {
            env.set("OPTARG", "");
            env.set("_GETOPT_OFFSET", &(offset + 1).to_string());
            BuiltinResult::ok()
        }
    } else if mandatory_arg {
        // Mandatory argument must come from the next argv.
        if optind < shell_args.len() {
            env.set("OPTARG", &shell_args[optind]);
            env.set("OPTIND", &(optind + 2).to_string());
            env.set("_GETOPT_OFFSET", "0");
            BuiltinResult::ok()
        } else {
            // Mandatory argument missing: report and let the loop continue.
            if silent {
                env.set(name, ":");
            } else {
                eprintln!("context: getopts: {} requires an argument", opt_char);
                env.set(name, "?");
            }
            env.set("OPTARG", &opt_char.to_string());
            env.set("OPTIND", &(optind + 1).to_string());
            env.set("_GETOPT_OFFSET", "0");
            BuiltinResult::ok()
        }
    } else {
        // Flag; for optional-arg (`c::`) nothing is consumed.
        env.set("OPTARG", "");
        env.set("OPTIND", &(optind + 1).to_string());
        env.set("_GETOPT_OFFSET", "0");
        BuiltinResult::ok()
    }
}

fn cmd_realpath(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: realpath: filename argument required");
        return BuiltinResult::err(1);
    }
    for arg in args {
        let path = Path::new(arg);
        match std::fs::canonicalize(path) {
            Ok(canonical) => println!("{}", canonical.display()),
            Err(e) => {
                eprintln!("context: realpath: {}: {}", arg, e);
                return BuiltinResult::err(1);
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_local(args: &[String], env: &mut Env) -> BuiltinResult {
    if FUNCTION_DEPTH.load(Ordering::Relaxed) == 0 {
        eprintln!("context: local: warning: ignoring function-context variable in global scope");
    }
    let mut print_mode = false;
    let mut print_names: Vec<String> = Vec::new();
    let mut vars = Vec::new();
    let mut no_flags = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => {
                print_mode = true;
                no_flags = false;
                i += 1;
            }
            "-a" => {
                no_flags = false;
                i += 1;
            }
            "-i" => {
                no_flags = false;
                i += 1;
            }
            "-r" => {
                no_flags = false;
                i += 1;
            }
            _ => {
                if print_mode {
                    print_names.push(args[i].clone());
                } else {
                    vars.push(args[i].as_str());
                }
                i += 1;
            }
        }
    }
    if print_mode {
        if print_names.is_empty() {
            for (k, v) in env.local_vars() {
                println!("declare -- {}={}", k, v);
            }
        } else {
            for name in &print_names {
                if let Some(val) = env.get(name) {
                    println!("declare -- {}={}", name, val);
                } else {
                    eprintln!("context: local: {}: not found", name);
                    return BuiltinResult::err(1);
                }
            }
        }
        return BuiltinResult::ok();
    }
    if no_flags && vars.is_empty() {
        for (k, v) in env.local_vars() {
            println!("{}={}", k, v);
        }
        return BuiltinResult::ok();
    }
    for var in vars {
        if let Some(eq_pos) = var.find('=') {
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];
            env.set_local(name, value);
        } else {
            env.set_local(var, "");
        }
    }
    BuiltinResult::ok()
}

fn cmd_read(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut raw = false;
    let mut silent = false;
    let mut prompt = String::new();
    let mut array_name = String::new();
    let mut delim = '\n';
    let mut timeout: Option<std::time::Duration> = None;
    let mut max_chars: Option<usize> = None;
    let mut read_fd: Option<i32> = None;
    let mut use_editor = false;
    let mut initial_text = String::new();
    let mut var_names = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-r" => {
                raw = true;
                i += 1;
            }
            "-s" => {
                silent = true;
                i += 1;
            }
            "-e" => {
                use_editor = true;
                i += 1;
            }
            "-i" => {
                i += 1;
                if i < args.len() {
                    initial_text = args[i].clone();
                    i += 1;
                }
            }
            "-a" => {
                i += 1;
                if i < args.len() {
                    array_name = args[i].clone();
                    i += 1;
                }
            }
            "-p" => {
                i += 1;
                if i < args.len() {
                    prompt = args[i].clone();
                    i += 1;
                }
            }
            "-d" => {
                i += 1;
                if i < args.len() {
                    delim = args[i].chars().next().unwrap_or('\n');
                    i += 1;
                }
            }
            "-t" => {
                i += 1;
                if i < args.len() {
                    if let Ok(t) = args[i].parse::<f64>() {
                        timeout = Some(std::time::Duration::from_secs_f64(t));
                    }
                    i += 1;
                }
            }
            "-n" => {
                i += 1;
                if i < args.len() {
                    max_chars = args[i].parse::<usize>().ok();
                    i += 1;
                }
            }
            "-u" => {
                i += 1;
                if i < args.len() {
                    read_fd = args[i].parse::<i32>().ok();
                    i += 1;
                }
            }
            "-" => {
                break;
            }
            _ => {
                var_names.push(args[i].clone());
                i += 1;
            }
        }
    }

    let is_timeout_zero = timeout.map(|t| t.is_zero()).unwrap_or(false);

    if is_timeout_zero {
        let fd = read_fd.unwrap_or(libc::STDIN_FILENO);
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ret = unsafe { libc::poll(&mut pollfd, 1, 0) };
        if ret == 0 || pollfd.revents == 0 {
            if !var_names.is_empty() || !array_name.is_empty() {
            } else {
                env.set("REPLY", "");
            }
            return BuiltinResult::ok();
        }
    }

    if use_editor
        && read_fd.is_none()
        && unsafe { libc::isatty(libc::STDIN_FILENO) } == 1
        && let Some(cb) = READLINE_CB.get()
    {
        let read_prompt = if prompt.is_empty() {
            String::new()
        } else {
            prompt.clone()
        };
        match cb(&read_prompt) {
            Ok(mut line) => {
                if !initial_text.is_empty() {
                    line = initial_text;
                }
                let line = line.trim_end_matches('\n').to_string();
                if !array_name.is_empty() {
                    let ifs = env
                        .get("IFS")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| " \t\n".to_string());
                    let words: Vec<&str> = if ifs.is_empty() {
                        line.split(char::is_whitespace)
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        line.split(|c: char| ifs.contains(c)).collect()
                    };
                    for (i, word) in words.iter().enumerate() {
                        env.set_local(&format!("{}_{}", array_name, i), word);
                    }
                    // Drop stale elements left over from a previous read.
                    let mut i = words.len();
                    while env.get(&format!("{}_{}", array_name, i)).is_some() {
                        env.unset(&format!("{}_{}", array_name, i));
                        i += 1;
                    }
                } else if var_names.is_empty() {
                    env.set("REPLY", &line);
                } else if var_names.len() == 1 {
                    env.set_local(&var_names[0], &line);
                } else {
                    let ifs = env
                        .get("IFS")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| " \t\n".to_string());
                    let words: Vec<&str> = if ifs.is_empty() {
                        line.split(char::is_whitespace)
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        line.split(|c: char| ifs.contains(c)).collect()
                    };
                    for (j, name) in var_names.iter().enumerate() {
                        let val = if j < words.len() { words[j] } else { "" };
                        env.set_local(name, val);
                    }
                }
                return BuiltinResult::ok();
            }
            Err(_) => {
                return BuiltinResult::err(1);
            }
        }
    }

    if !prompt.is_empty() {
        eprint!("{}", prompt);
        let _ = std::io::stderr().flush();
    }

    let stdin = std::io::stdin();
    let mut line = String::new();
    let mut chars_read: usize = 0;
    let mut pending_utf8: Vec<u8> = Vec::new();
    let start = std::time::Instant::now();

    // Read a single byte from the chosen fd, or None on EOF/error.
    macro_rules! read_byte {
        () => {{
            let mut buf = [0u8; 1];
            let ok = if let Some(fd) = read_fd {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
                n > 0
            } else {
                std::io::Read::read(&mut stdin.lock(), &mut buf).unwrap_or(0) > 0
            };
            if ok { Some(buf[0]) } else { None }
        }};
    }

    // Handle one decoded character. `stop` carries an early exit status.
    macro_rules! process_char {
        ($ch:expr, $stop:expr, $saw_delim:expr) => {{
            let ch: char = $ch;
            if ch == delim {
                $saw_delim = true;
            } else if ch == '\x03' {
                eprintln!("^C");
                $stop = Some(130);
            } else if ch == '\x04' && line.is_empty() {
                $stop = Some(1);
            } else if ch == '\x7f' || ch == '\x08' {
                line.pop();
                if !silent {
                    eprint!("\x1b[2D \x1b[2D");
                    let _ = std::io::stderr().flush();
                }
            } else if !raw && ch == '\\' && pending_utf8.is_empty() {
                match read_byte!() {
                    None => {}
                    Some(next_byte) => match next_byte as char {
                        '\n' => {}
                        'n' => {
                            line.push('\n');
                            chars_read += 1;
                        }
                        't' => {
                            line.push('\t');
                            chars_read += 1;
                        }
                        '\\' => {
                            line.push('\\');
                            chars_read += 1;
                        }
                        _ => {
                            // Keep escape semantics for ASCII; multibyte is
                            // pushed through the UTF-8 accumulator below.
                            if next_byte.is_ascii() {
                                line.push('\\');
                                line.push(next_byte as char);
                                chars_read += 2;
                            } else {
                                line.push('\\');
                                chars_read += 1;
                                pending_utf8.push(next_byte);
                            }
                        }
                    },
                }
            } else {
                line.push(ch);
                chars_read += 1;
                if !silent {
                    eprint!("{}", ch);
                    let _ = std::io::stderr().flush();
                }
            }
        }};
    }

    'outer: loop {
        if let Some(t) = timeout
            && !t.is_zero()
            && start.elapsed() >= t
        {
            break;
        }
        if let Some(max) = max_chars
            && chars_read >= max
        {
            break;
        }

        // Poll with the remaining time so `-t` fires even while blocked.
        if let Some(t) = timeout.filter(|t| !t.is_zero()) {
            let fd = read_fd.unwrap_or(libc::STDIN_FILENO);
            let elapsed = start.elapsed();
            let remain_ms = if elapsed >= t {
                0
            } else {
                (t - elapsed).as_millis() as i32
            };
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ret = unsafe { libc::poll(&mut pollfd, 1, remain_ms) };
            if ret == 0 {
                break; // timed out while idle
            }
        }

        let Some(byte) = read_byte!() else { break };
        pending_utf8.push(byte);

        // Incrementally decode complete UTF-8 sequences.
        loop {
            match std::str::from_utf8(&pending_utf8) {
                Ok(text) => {
                    let Some(ch) = text.chars().next() else {
                        pending_utf8.clear();
                        break;
                    };
                    pending_utf8.drain(..ch.len_utf8());
                    let mut stop: Option<i32> = None;
                    let mut saw_delim = false;
                    process_char!(ch, stop, saw_delim);
                    if let Some(code) = stop {
                        return BuiltinResult::err(code);
                    }
                    if saw_delim {
                        break 'outer;
                    }
                    if pending_utf8.is_empty() {
                        break;
                    }
                }
                Err(e) => {
                    match e.error_len() {
                        None => break, // need more bytes for this sequence
                        Some(inv) => {
                            let bad: Vec<u8> = pending_utf8.drain(..inv).collect();
                            let lossy = String::from_utf8_lossy(&bad).to_string();
                            for ch in lossy.chars() {
                                let mut stop: Option<i32> = None;
                                let mut saw_delim = false;
                                process_char!(ch, stop, saw_delim);
                                if let Some(code) = stop {
                                    return BuiltinResult::err(code);
                                }
                                if saw_delim {
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !silent {
        eprintln!();
        let _ = std::io::stderr().flush();
    }

    let line = if delim == '\n' && line.ends_with('\n') {
        line[..line.len() - 1].to_string()
    } else {
        line
    };

    if !array_name.is_empty() {
        let ifs = env
            .get("IFS")
            .map(|s| s.to_string())
            .unwrap_or_else(|| " \t\n".to_string());
        let words: Vec<&str> = if ifs.is_empty() {
            line.split(char::is_whitespace)
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            line.split(|c: char| ifs.contains(c)).collect()
        };
        for (i, word) in words.iter().enumerate() {
            env.set_local(&format!("{}_{}", array_name, i), word);
        }
        // Drop stale elements left over from a previous read.
        let mut i = words.len();
        while env.get(&format!("{}_{}", array_name, i)).is_some() {
            env.unset(&format!("{}_{}", array_name, i));
            i += 1;
        }
    } else if var_names.is_empty() {
        env.set("REPLY", &line);
    } else if var_names.len() == 1 {
        env.set_local(&var_names[0], &line);
    } else {
        let ifs = env
            .get("IFS")
            .map(|s| s.to_string())
            .unwrap_or_else(|| " \t\n".to_string());
        let words: Vec<&str> = if ifs.is_empty() {
            line.split(char::is_whitespace)
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            line.split(|c: char| ifs.contains(c)).collect()
        };
        for (i, name) in var_names.iter().enumerate() {
            let val = if i < words.len() { words[i] } else { "" };
            env.set_local(name, val);
        }
    }

    BuiltinResult::ok()
}

fn cmd_shopt(args: &[String], env: &mut Env) -> BuiltinResult {
    let known_opts = [
        "cdable_vars",
        "cdspell",
        "checkhash",
        "checkwinsize",
        "cmdhist",
        "compat31",
        "compat32",
        "compat40",
        "compat41",
        "compat42",
        "compat43",
        "compat44",
        "complete_fullquote",
        "direxpand",
        "dirspell",
        "dotglob",
        "execfail",
        "expand_aliases",
        "extdebug",
        "extglob",
        "extquote",
        "failglob",
        "force_fignore",
        "globasciiranges",
        "globstar",
        "globskipdots",
        "histappend",
        "histreedit",
        "histverify",
        "hostcomplete",
        "huponexit",
        "inherit_errexit",
        "interactive_comments",
        "lastpipe",
        "localvar_inherit",
        "localvar_unset",
        "login_shell",
        "mailwarn",
        "no_empty_cmd_comp",
        "nocaseglob",
        "nocasematch",
        "nullglob",
        "patsub_replacement",
        "progcomp",
        "progvars",
        "promptvars",
        "restricted",
        "shift_verbose",
        "sourcepath",
        "xpg_echo",
    ];

    if args.is_empty() {
        for name in &known_opts {
            let val = env
                .get(&format!("_SHOPT_{}", name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(name == &"expand_aliases");
            let state = if val { "on" } else { "off" };
            println!("{}{}\t{}", name, " ".repeat(25 - name.len()), state);
        }
        return BuiltinResult::ok();
    }

    let mut i = 0;
    let mut query = false;
    let mut enable = true;
    let mut print_mode = false;
    if args[0] == "-s" {
        i = 1;
    } else if args[0] == "-u" {
        i = 1;
        enable = false;
    } else if args[0] == "-q" {
        i = 1;
        query = true;
    } else if args[0] == "-p" {
        i = 1;
        print_mode = true;
    } else if args[0].starts_with('+') {
        enable = false;
        i = 1;
    } else if args[0].starts_with('-') && args[0] != "-p" {
        eprintln!("context: shopt: {}: invalid option", args[0]);
        return BuiltinResult::err(2);
    }

    if i >= args.len() && !print_mode {
        for name in &known_opts {
            let val = env
                .get(&format!("_SHOPT_{}", name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(name == &"expand_aliases");
            let state = if val { "on" } else { "off" };
            println!("{}{}\t{}", name, " ".repeat(25 - name.len()), state);
        }
        return BuiltinResult::ok();
    }

    if print_mode {
        if i >= args.len() {
            for name in &known_opts {
                let val = env
                    .get(&format!("_SHOPT_{}", name.to_uppercase()))
                    .map(|s| s == "1")
                    .unwrap_or(name == &"expand_aliases");
                if val {
                    println!("shopt -s {}", name);
                } else {
                    println!("shopt -u {}", name);
                }
            }
            return BuiltinResult::ok();
        }
        while i < args.len() {
            let opt_name = &args[i];
            let val = env
                .get(&format!("_SHOPT_{}", opt_name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(opt_name == "expand_aliases");
            if val {
                println!("shopt -s {}", opt_name);
            } else {
                println!("shopt -u {}", opt_name);
            }
            i += 1;
        }
        return BuiltinResult::ok();
    }

    while i < args.len() {
        let opt_name = &args[i];
        if opt_name.starts_with('-') || opt_name.starts_with('+') {
            i += 1;
            continue;
        }
        if query {
            let val = env
                .get(&format!("_SHOPT_{}", opt_name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(opt_name == "expand_aliases");
            let matches = if enable { val } else { !val };
            if !matches {
                return BuiltinResult::err(1);
            }
        } else {
            let var_name = format!("_SHOPT_{}", opt_name.to_uppercase());
            if enable {
                env.set(&var_name, "1");
            } else {
                env.set(&var_name, "0");
            }
        }
        i += 1;
    }
    BuiltinResult::ok()
}

fn cmd_bindkey(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        let bindings_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(".config"))
            .join("context/keybindings");
        if let Ok(entries) = std::fs::read_dir(&bindings_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && let Some(widget) = std::fs::read_to_string(entry.path())
                        .ok()
                        .map(|s| s.trim().to_string())
                {
                    println!("{} -> {}", name, widget);
                }
            }
        } else {
            println!("no keybindings configured (use: bindkey <key> <widget>)");
        }
        return BuiltinResult::ok();
    }
    if args.len() < 2 {
        eprintln!("context: bindkey: usage: bindkey <key-sequence> <widget-name>");
        return BuiltinResult::err(1);
    }
    let key = &args[0];
    let widget = &args[1];
    let valid_widgets = [
        "accept-line",
        "backward-char",
        "forward-char",
        "backward-delete-char",
        "delete-char",
        "backward-word",
        "forward-word",
        "beginning-of-line",
        "end-of-line",
        "kill-line",
        "backward-kill-line",
        "kill-word",
        "backward-kill-word",
        "yank",
        "accept-suggestion",
        "accept-suggestion-word",
        "history-search-backward",
        "history-search-forward",
        "clear-screen",
        "undo",
        "redo",
        "transpose-chars",
    ];
    if !valid_widgets.contains(&widget.as_str()) {
        eprintln!(
            "context: bindkey: unknown widget '{}'. Valid widgets:",
            widget
        );
        for w in &valid_widgets {
            eprintln!("  {}", w);
        }
        return BuiltinResult::err(1);
    }
    let bindings_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".config"))
        .join("context/keybindings");
    let _ = std::fs::create_dir_all(&bindings_dir);
    let safe_name = key.replace(['/', ' '], "_");
    let path = bindings_dir.join(&safe_name);
    match std::fs::write(&path, widget) {
        Ok(()) => {
            env.set("_BINDKEY_LAST", key);
            BuiltinResult::ok()
        }
        Err(e) => {
            eprintln!("context: bindkey: failed to save binding: {}", e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_shift(args: &[String], env: &mut Env) -> BuiltinResult {
    let n: usize = if args.is_empty() {
        1
    } else if let Ok(v) = args[0].parse::<i32>() {
        if v < 0 {
            eprintln!("context: shift: negative shift count");
            return BuiltinResult::err(1);
        }
        v as usize
    } else {
        eprintln!("context: shift: numeric argument required");
        return BuiltinResult::err(1);
    };
    let pos = env.positional().to_vec();
    if n > pos.len() {
        eprintln!("context: shift: shift count exceeds parameter count");
        return BuiltinResult::err(1);
    }
    env.set_positional(pos[n..].to_vec());
    BuiltinResult::ok()
}

fn cmd_ulimit(args: &[String]) -> BuiltinResult {
    let mut show_all = false;
    let mut hard = false;
    let mut resource: Option<libc::c_int> = None;
    let mut value: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-a" {
            show_all = true;
        } else if arg == "-H" {
            hard = true;
        } else if arg == "-S" {
            hard = false;
        } else if arg == "-n"
            || arg == "-s"
            || arg == "-d"
            || arg == "-f"
            || arg == "-m"
            || arg == "-l"
            || arg == "-t"
            || arg == "-p"
            || arg == "-u"
            || arg == "-c"
        {
            resource = Some(match arg.as_str() {
                "-n" => libc::RLIMIT_NOFILE,
                "-s" => libc::RLIMIT_STACK,
                "-d" => libc::RLIMIT_DATA,
                "-f" => libc::RLIMIT_FSIZE,
                "-m" => libc::RLIMIT_AS,
                "-l" => libc::RLIMIT_MEMLOCK,
                "-t" => libc::RLIMIT_CPU,
                "-p" | "-u" => libc::RLIMIT_NPROC,
                "-c" => libc::RLIMIT_CORE,
                _ => libc::RLIMIT_NOFILE,
            } as libc::c_int);
        } else if let Ok(v) = arg.parse::<u64>() {
            value = Some(v);
        } else if arg.starts_with('-') && arg.len() > 1 {
            eprintln!("context: ulimit: {}: invalid option", &arg[1..]);
            return BuiltinResult::err(2);
        } else {
            eprintln!("context: ulimit: {}: invalid argument", arg);
            return BuiltinResult::err(2);
        }
        i += 1;
    }

    if show_all {
        let resources: &[(&str, libc::c_int)] = &[
            ("core file size", libc::RLIMIT_CORE as libc::c_int),
            ("data seg size", libc::RLIMIT_DATA as libc::c_int),
            ("file size", libc::RLIMIT_FSIZE as libc::c_int),
            ("open files", libc::RLIMIT_NOFILE as libc::c_int),
            ("stack size", libc::RLIMIT_STACK as libc::c_int),
            ("cpu time", libc::RLIMIT_CPU as libc::c_int),
            ("max user processes", libc::RLIMIT_NPROC as libc::c_int),
            ("virtual memory", libc::RLIMIT_AS as libc::c_int),
            ("max locked memory", libc::RLIMIT_MEMLOCK as libc::c_int),
        ];
        for &(name, res) in resources {
            let mut rlim: libc::rlimit = unsafe { std::mem::zeroed() };
            if unsafe { libc::getrlimit(res as _, &mut rlim) } == 0 {
                let limit = if hard { rlim.rlim_max } else { rlim.rlim_cur };
                if limit == libc::RLIM_INFINITY {
                    println!("unlimited\t\t-{}", name);
                } else {
                    println!("{}\t\t-{}", limit, name);
                }
            }
        }
        return BuiltinResult::ok();
    }

    if let Some(res) = resource {
        if let Some(v) = value {
            let mut rlim: libc::rlimit = unsafe { std::mem::zeroed() };
            if unsafe { libc::getrlimit(res as _, &mut rlim) } != 0 {
                eprintln!("context: ulimit: getrlimit failed");
                return BuiltinResult::err(2);
            }
            if hard {
                rlim.rlim_max = v;
            } else {
                rlim.rlim_cur = v;
            }
            if unsafe { libc::setrlimit(res as _, &rlim) } != 0 {
                eprintln!("context: ulimit: cannot modify limit: Operation not permitted");
                return BuiltinResult::err(2);
            }
            return BuiltinResult::ok();
        } else {
            let mut rlim: libc::rlimit = unsafe { std::mem::zeroed() };
            if unsafe { libc::getrlimit(res as _, &mut rlim) } == 0 {
                let limit = if hard { rlim.rlim_max } else { rlim.rlim_cur };
                if limit == libc::RLIM_INFINITY {
                    println!("unlimited");
                } else {
                    println!("{}", limit);
                }
            }
            return BuiltinResult::ok();
        }
    }

    BuiltinResult::ok()
}

fn cmd_times() -> BuiltinResult {
    let mut t: libc::tms = unsafe { std::mem::zeroed() };
    let ticks = unsafe { libc::times(&mut t) };
    if ticks == -1 {
        eprintln!("context: times: failed to get process times");
        return BuiltinResult::err(1);
    }
    let format = |clock: libc::clock_t| -> String {
        let secs = clock as f64 / ticks as f64;
        let whole = secs as u64;
        let frac = ((secs - whole as f64) * 100.0) as u64;
        format!("{}.{:02}", whole, frac)
    };
    println!("\t{} \t{}", format(t.tms_utime), format(t.tms_stime));
    println!("\t{} \t{}", format(t.tms_cutime), format(t.tms_cstime));
    BuiltinResult::ok()
}

fn cmd_logout() -> BuiltinResult {
    let shlvl = std::env::var("SHLVL")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(1);
    if shlvl > 1 {
        eprintln!("context: logout: not login shell");
        return BuiltinResult::err(1);
    }
    BuiltinResult::exit(0)
}

fn cmd_enable(args: &[String], _env: &mut Env, cfg: &Config) -> BuiltinResult {
    let all_builtins = BUILTINS;
    if args.is_empty() {
        let disabled = DISABLED_BUILTINS.lock().unwrap();
        for name in all_builtins {
            let status = if disabled.contains(*name) {
                "off"
            } else {
                "on"
            };
            println!("{}={}", name, status);
        }
        return BuiltinResult::ok();
    }
    let mut i = 0;
    let mut disable_mode = false;
    let mut filename: Option<String> = None;
    let mut names: Vec<String> = Vec::new();
    while i < args.len() {
        if args[i] == "-n" {
            disable_mode = true;
            i += 1;
        } else if args[i] == "-f" {
            i += 1;
            if i < args.len() {
                filename = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("context: enable: -f requires a filename argument");
                return BuiltinResult::err(2);
            }
        } else if args[i].starts_with('-') && args[i].len() > 1 {
            let mut skip_next = false;
            for (ci, ch) in args[i][1..].chars().enumerate() {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                match ch {
                    'n' => disable_mode = true,
                    'f' => {
                        i += 1;
                        if i < args.len() {
                            filename = Some(args[i].clone());
                        } else {
                            eprintln!("context: enable: -f requires a filename argument");
                            return BuiltinResult::err(2);
                        }
                    }
                    _ => {
                        eprintln!("context: enable: -{}: invalid option", ch);
                        return BuiltinResult::err(2);
                    }
                }
                if ci + 2 < args[i].len() {
                    skip_next = true;
                }
            }
            i += 1;
        } else {
            names.push(args[i].clone());
            i += 1;
        }
    }
    if let Some(ref _fname) = filename {
        eprintln!(
            "context: enable: loading builtins from external shared libraries is not yet supported"
        );
        return BuiltinResult::err(1);
    }
    if disable_mode && cfg.security.restricted_mode {
        eprintln!("context: enable: restricted mode: cannot disable builtins");
        return BuiltinResult::err(1);
    }
    if names.is_empty() {
        eprintln!("context: enable: requires a builtin name");
        return BuiltinResult::err(1);
    }
    let mut disabled = DISABLED_BUILTINS.lock().unwrap();
    for name in &names {
        if !all_builtins.contains(&name.as_str()) {
            eprintln!("context: enable: {}: not a builtin", name);
            return BuiltinResult::err(1);
        }
        if disable_mode {
            disabled.insert(name.clone());
        } else {
            disabled.remove(name);
        }
    }
    BuiltinResult::ok()
}

fn cmd_help(args: &[String]) -> BuiltinResult {
    let mut brief_mode = false;
    let mut name_args: Vec<&String> = Vec::new();
    for arg in args {
        if arg == "-d" {
            brief_mode = true;
        } else if arg == "-m" {
        } else {
            name_args.push(arg);
        }
    }
    let builtin_list: &[(&str, &str)] = &[
        ("alias", "define or display aliases"),
        ("builtin", "run a shell builtin"),
        ("cd", "change the working directory"),
        ("command", "run a command, ignoring shell functions"),
        ("declare", "declare variables and give them attributes"),
        ("dirs", "display directory stack"),
        ("enable", "enable and disable builtin shell commands"),
        ("eval", "evaluate arguments as a shell command"),
        ("exit", "exit the shell"),
        ("export", "set export attribute for variables"),
        ("false", "return a non-zero exit status"),
        ("getopts", "parse positional parameters as option specs"),
        ("help", "display help information"),
        ("history", "display or manipulate the history list"),
        ("kill", "send signals to processes"),
        ("let", "evaluate arithmetic expressions"),
        ("local", "create local variables"),
        ("logout", "exit a login shell"),
        ("math", "evaluate arithmetic expressions (floating point)"),
        ("popd", "remove directories from the directory stack"),
        ("printf", "formatted output"),
        ("pushd", "push directories onto the directory stack"),
        ("pwd", "print the current working directory"),
        ("read", "read a line from standard input"),
        ("readonly", "mark variables as read-only"),
        ("realpath", "print the resolved path"),
        ("regexmatch", "match a string against a regex"),
        ("select", "select a word from a list"),
        (
            "set",
            "set or unset shell options and positional parameters",
        ),
        ("shift", "shift positional parameters"),
        ("shopt", "set and unset shell options"),
        ("source", "read and execute commands from a file"),
        ("suspend", "suspend the shell"),
        ("test", "evaluate conditional expressions"),
        ("times", "print accumulated process times"),
        ("trap", "set signal handlers"),
        ("true", "return a zero exit status"),
        ("type", "describe a command"),
        ("ulimit", "set or query resource limits"),
        ("umask", "set or get the file mode creation mask"),
        ("unalias", "remove alias definitions"),
        ("unset", "unset variables and functions"),
        ("wait", "wait for child processes"),
        ("which", "locate a command"),
        ("mapfile", "read lines into an array"),
        ("readarray", "alias for mapfile"),
        ("compgen", "generate completion candidates"),
        ("complete", "define completion specifications"),
        ("fc", "fix command, list or re-execute from history"),
        ("disown", "remove jobs from the job table"),
    ];
    if name_args.is_empty() && !brief_mode {
        println!("Context shell builtins:");
        for (name, desc) in builtin_list {
            println!("  {:<14} {}", name, desc);
        }
        return BuiltinResult::ok();
    }
    if name_args.is_empty() && brief_mode {
        for (name, desc) in builtin_list {
            println!("{} - {}", name, desc);
        }
        return BuiltinResult::ok();
    }
    for name_arg in &name_args {
        let name = name_arg.as_str();
        if brief_mode {
            println!("{} - {}", name, get_builtin_description(name));
        } else {
            println!("{}: {}", name, get_builtin_description(name));
        }
    }
    BuiltinResult::ok()
}

fn get_builtin_description(name: &str) -> &'static str {
    match name {
        "alias" => "define or display aliases",
        "builtin" => "run a shell builtin",
        "cd" => "change the working directory",
        "command" => "run a command, ignoring shell functions",
        "declare" => "declare variables and give them attributes",
        "dirs" => "display directory stack",
        "enable" => "enable and disable builtin shell commands",
        "eval" => "evaluate arguments as a shell command",
        "exit" => "exit the shell",
        "export" => "set export attribute for variables",
        "false" => "return a non-zero exit status",
        "getopts" => "parse positional parameters as option specs",
        "help" => "display help information",
        "history" => "display or manipulate the history list",
        "kill" => "send signals to processes",
        "let" => "evaluate arithmetic expressions",
        "local" => "create local variables",
        "logout" => "exit a login shell",
        "math" => "evaluate arithmetic expressions (floating point)",
        "popd" => "remove directories from the directory stack",
        "printf" => "formatted output",
        "pushd" => "push directories onto the directory stack",
        "pwd" => "print the current working directory",
        "read" => "read a line from standard input",
        "readonly" => "mark variables as read-only",
        "realpath" => "print the resolved path",
        "regexmatch" => "match a string against a regex",
        "select" => "select a word from a list",
        "set" => "set or unset shell options and positional parameters",
        "shift" => "shift positional parameters",
        "shopt" => "set and unset shell options",
        "source" => "read and execute commands from a file",
        "test" => "evaluate conditional expressions",
        "times" => "print accumulated process times",
        "trap" => "set signal handlers",
        "true" => "return a zero exit status",
        "type" => "describe a command",
        "ulimit" => "set or query resource limits",
        "umask" => "set or get the file mode creation mask",
        "unalias" => "remove alias definitions",
        "unset" => "unset variables and functions",
        "wait" => "wait for child processes",
        "which" => "locate a command",
        ":" => "no effect; successful completion",
        "." => "read and execute commands from a file",
        "mapfile" => "read lines into an array",
        "readarray" => "alias for mapfile",
        "compgen" => "generate completion candidates",
        "complete" => "define completion specifications",
        "fc" => "fix command, list or re-execute from history",
        "disown" => "remove jobs from the job table",
        "suspend" => "suspend the shell",
        _ => "shell builtin",
    }
}

struct FdReader(i32);

impl io::Read for FdReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.0, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

fn cmd_fc(args: &[String], env: &mut Env, _cfg: &Config) -> BuiltinResult {
    let mut list_mode = false;
    let mut no_numbering = false;
    let mut run_last = false;
    let mut editor: Option<String> = None;
    let mut first: Option<String> = None;
    let mut last: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-l" => {
                list_mode = true;
                i += 1;
            }
            "-n" => {
                no_numbering = true;
                i += 1;
            }
            "-s" => {
                run_last = true;
                i += 1;
            }
            "-e" => {
                i += 1;
                if i < args.len() {
                    editor = Some(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("context: fc: -e requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "--" => {
                break;
            }
            _ if args[i].starts_with('-') => {
                let flags = &args[i][1..];
                for ch in flags.chars() {
                    match ch {
                        'l' => list_mode = true,
                        'n' => no_numbering = true,
                        's' => run_last = true,
                        'e' => {
                            i += 1;
                            if i < args.len() {
                                editor = Some(args[i].clone());
                            } else {
                                eprintln!("context: fc: -e requires an argument");
                                return BuiltinResult::err(2);
                            }
                        }
                        _ => {
                            eprintln!("context: fc: -{}: invalid option", ch);
                            return BuiltinResult::err(2);
                        }
                    }
                }
                i += 1;
            }
            _ => {
                if first.is_none() {
                    first = Some(args[i].clone());
                } else if last.is_none() {
                    last = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    let get_history = || -> Vec<String> {
        if let Some(cb) = HISTORY_CB.get() {
            cb()
        } else {
            Vec::new()
        }
    };

    let exec_cmd = |cmd: &str| {
        if let Some(exec) = FC_EXEC_CB.get() {
            exec(cmd);
        }
    };

    if run_last {
        let history = get_history();
        if history.is_empty() {
            eprintln!("context: fc: no history");
            return BuiltinResult::err(1);
        }
        let cmd = &history[history.len() - 1];
        eprintln!("{}", cmd);
        exec_cmd(cmd);
        return BuiltinResult::ok();
    }

    let history = get_history();
    if history.is_empty() {
        eprintln!("context: fc: no history");
        return BuiltinResult::err(1);
    }

    let total = history.len() as i64;
    let (start, end) = if let Some(ref f_str) = first {
        let f_num = f_str.parse::<i64>().unwrap_or(-1);
        if f_num < 0 {
            ((total + f_num).max(0) as usize, (total - 1) as usize)
        } else {
            let f_usize = (f_num - 1).max(0) as usize;
            let e_usize = last
                .as_ref()
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| {
                    if n < 0 {
                        ((total + n).max(0)) as usize
                    } else {
                        (n - 1).max(0) as usize
                    }
                })
                .unwrap_or(f_usize);
            (f_usize, e_usize)
        }
    } else {
        let start = (total - 16).max(0) as usize;
        let end = (total - 1) as usize;
        (start, end)
    };

    let slice_start = start.min(history.len());
    let slice_end = (end + 1).min(history.len());
    if slice_start >= slice_end {
        eprintln!("context: fc: no matching history entries");
        return BuiltinResult::err(1);
    }

    if list_mode {
        for (idx, entry) in history[slice_start..slice_end].iter().enumerate() {
            if no_numbering {
                println!("    {}", entry);
            } else {
                println!("{:<6}{}", slice_start + idx + 1, entry);
            }
        }
        return BuiltinResult::ok();
    }

    let editor_cmd = editor
        .or_else(|| env.get("FCEDIT").map(|s| s.to_string()))
        .or_else(|| env.get("EDITOR").map(|s| s.to_string()))
        .unwrap_or_else(|| "vi".to_string());

    let joined: String = history[slice_start..slice_end].join("\n");
    // Random, exclusively-created tempfile under $TMPDIR (symlink-safe).
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp_dir = std::env::temp_dir();
    let mut tmp_path = std::path::PathBuf::new();
    for _ in 0..64 {
        let rand: u64 = (unsafe { libc::rand() } as u64) << 32
            ^ std::process::id() as u64
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(0);
        let candidate = tmp_dir.join(format!(".ctx_fc_{:x}", rand));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidate)
        {
            Ok(mut f) => {
                use std::io::Write;
                if f.write_all(joined.as_bytes()).is_ok() {
                    tmp_path = candidate;
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    if tmp_path.as_os_str().is_empty() {
        eprintln!("context: fc: failed to create temporary file");
        return BuiltinResult::err(1);
    }
    let tmp_str = tmp_path.to_string_lossy().to_string();
    let status = std::process::Command::new(&editor_cmd)
        .arg(&tmp_str)
        .status();
    match status {
        Ok(s) if s.success() => {
            if let Ok(edited) = std::fs::read_to_string(&tmp_str) {
                let cmd = edited.trim().to_string();
                let _ = std::fs::remove_file(&tmp_str);
                if !cmd.is_empty() {
                    exec_cmd(&cmd);
                    return BuiltinResult::ok();
                }
            }
            let _ = std::fs::remove_file(&tmp_str);
            BuiltinResult::err(1)
        }
        _ => {
            let _ = std::fs::remove_file(&tmp_str);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_disown(args: &[String]) -> BuiltinResult {
    let mut pids: Vec<i32> = Vec::new();
    let mut all = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-a" => {
                all = true;
                i += 1;
            }
            "-h" => {
                i += 1;
            }
            "--" => {
                break;
            }
            _ if args[i].starts_with('-') => {
                eprintln!("context: disown: {}: invalid option", args[i]);
                return BuiltinResult::err(2);
            }
            _ => {
                if let Ok(pid) = args[i].parse::<i32>() {
                    pids.push(pid);
                } else {
                    let id_str = args[i].trim_start_matches('%');
                    if let Ok(id) = id_str.parse::<i32>() {
                        pids.push(id);
                    } else {
                        eprintln!("context: disown: {}: no such job", args[i]);
                        return BuiltinResult::err(1);
                    }
                }
                i += 1;
            }
        }
    }

    let cb = DISOWN_JOBS_CB.get_or_init(|| Mutex::new(None));
    let mut cb = cb.lock().unwrap();
    if let Some(ref mut f) = *cb {
        if all {
            f(-1);
        } else if pids.is_empty() {
            f(0);
        } else {
            for pid in pids {
                f(pid);
            }
        }
    } else {
        eprintln!("context: disown: job control not available");
        return BuiltinResult::err(1);
    }
    BuiltinResult::ok()
}

fn cmd_mapfile(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut delim: char = '\n';
    let mut count: Option<usize> = None;
    let mut skip: usize = 0;
    let mut origin: usize = 0;
    let mut strip_newline = false;
    let mut read_fd: Option<i32> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-d" => {
                i += 1;
                if i < args.len() {
                    delim = args[i].chars().next().unwrap_or('\n');
                    i += 1;
                } else {
                    eprintln!("context: mapfile: -d requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-n" => {
                i += 1;
                if i < args.len() {
                    count = args[i].parse::<usize>().ok();
                    i += 1;
                } else {
                    eprintln!("context: mapfile: -n requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-s" => {
                i += 1;
                if i < args.len() {
                    skip = args[i].parse::<usize>().unwrap_or(0);
                    i += 1;
                } else {
                    eprintln!("context: mapfile: -s requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-O" => {
                i += 1;
                if i < args.len() {
                    origin = args[i].parse::<usize>().unwrap_or(0);
                    i += 1;
                } else {
                    eprintln!("context: mapfile: -O requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-t" => {
                strip_newline = true;
                i += 1;
            }
            "-u" => {
                i += 1;
                if i < args.len() {
                    read_fd = args[i].parse::<i32>().ok();
                    i += 1;
                } else {
                    eprintln!("context: mapfile: -u requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-C" | "-c" => {
                i += 1;
                if i < args.len() {
                    i += 1;
                } else {
                    eprintln!(
                        "context: mapfile: -{} requires an argument",
                        &args[i - 1][1..]
                    );
                    return BuiltinResult::err(2);
                }
            }
            "--" => {
                i += 1;
                break;
            }
            _ if args[i].starts_with('-') => {
                eprintln!("context: mapfile: {}: invalid option", args[i]);
                return BuiltinResult::err(2);
            }
            _ => break,
        }
    }

    if i >= args.len() {
        eprintln!("context: mapfile: array name argument required");
        return BuiltinResult::err(1);
    }
    let array_name = &args[i];

    let mut lines: Vec<String> = Vec::new();

    if delim == '\n' {
        let stdin = io::stdin();
        let mut reader: Box<dyn io::Read> = if let Some(fd) = read_fd {
            Box::new(FdReader(fd))
        } else {
            Box::new(stdin.lock())
        };
        let buf_reader = io::BufReader::new(&mut reader);
        for line_result in buf_reader.lines() {
            match line_result {
                Ok(line) => lines.push(line),
                Err(_) => break,
            }
        }
    } else {
        let stdin = io::stdin();
        let mut reader: Box<dyn io::Read> = if let Some(fd) = read_fd {
            Box::new(FdReader(fd))
        } else {
            Box::new(stdin.lock())
        };
        let mut content = String::new();
        let _ = reader.read_to_string(&mut content);
        for part in content.split(delim) {
            lines.push(part.to_string());
        }
    }

    for _ in 0..skip {
        if !lines.is_empty() {
            lines.remove(0);
        }
    }

    if let Some(max) = count {
        lines.truncate(max);
    }

    for (idx, line) in lines.iter().enumerate() {
        let val = if strip_newline {
            line.trim_end_matches('\n').to_string()
        } else {
            line.to_string()
        };
        env.set(&format!("{}_{}", array_name, origin + idx), &val);
    }

    // Drop stale elements beyond the new content.
    let mut i = origin + lines.len();
    while env.get(&format!("{}_{}", array_name, i)).is_some() {
        env.unset(&format!("{}_{}", array_name, i));
        i += 1;
    }
    BuiltinResult::ok()
}

fn cmd_compgen(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut wordlist: Option<String> = None;
    let mut action: Option<String> = None;
    let mut do_files = false;
    let mut do_dirs = false;
    let mut do_builtins = false;
    let mut do_aliases = false;
    let mut do_vars = false;
    let mut do_commands = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-W" => {
                i += 1;
                if i < args.len() {
                    wordlist = Some(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("context: compgen: -W requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-A" => {
                i += 1;
                if i < args.len() {
                    action = Some(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("context: compgen: -A requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-f" => {
                do_files = true;
                i += 1;
            }
            "-d" => {
                do_dirs = true;
                i += 1;
            }
            "-b" => {
                do_builtins = true;
                i += 1;
            }
            "-a" => {
                do_aliases = true;
                i += 1;
            }
            "-v" => {
                do_vars = true;
                i += 1;
            }
            "-c" => {
                do_commands = true;
                i += 1;
            }
            "-k" => {
                i += 1;
            }
            "--" => {
                i += 1;
                break;
            }
            _ if args[i].starts_with('-') => {
                i += 1;
            }
            _ => break,
        }
    }

    let filter = if i < args.len() { &args[i] } else { "" };
    let mut candidates: Vec<String> = Vec::new();

    if let Some(ref wl) = wordlist {
        for word in wl.split_whitespace() {
            candidates.push(word.to_string());
        }
    }

    if let Some(ref act) = action {
        match act.as_str() {
            "alias" => {
                for k in env.all_aliases().keys() {
                    candidates.push(k.clone());
                }
            }
            "builtin" => {
                let builtins = [
                    "enable",
                    "wait",
                    "umask",
                    "trap",
                    "type",
                    "times",
                    "readonly",
                    "printf",
                    "pushd",
                    "popd",
                    "pwd",
                    "read",
                    "set",
                    "shift",
                    "shopt",
                    "source",
                    "test",
                    "true",
                    "false",
                    "command",
                    "eval",
                    "exit",
                    "export",
                    "cd",
                    "dirs",
                    "hash",
                    "kill",
                    "let",
                    "local",
                    "math",
                    "select",
                    "getopts",
                    "realpath",
                    "regexmatch",
                    "declare",
                    "typeset",
                    "help",
                    "ulimit",
                    "logout",
                    "mapfile",
                    "readarray",
                    "compgen",
                    "complete",
                ];
                for b in builtins {
                    candidates.push(b.to_string());
                }
            }
            "command" => {
                let path_env = std::env::var("PATH").unwrap_or_default();
                for dir in path_env.split(':') {
                    if dir.is_empty() {
                        continue;
                    }
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !candidates.contains(&name) {
                                candidates.push(name);
                            }
                        }
                    }
                }
            }
            "variable" => {
                for k in env.all_vars().keys() {
                    candidates.push(k.clone());
                }
            }
            "function" => {}
            "completion" => {
                let comps = COMPLETIONS.lock().unwrap();
                for name in comps.keys() {
                    candidates.push(name.clone());
                }
            }
            _ => {}
        }
    }

    if do_builtins {
        let builtins = [
            "enable",
            "wait",
            "umask",
            "trap",
            "type",
            "times",
            "readonly",
            "printf",
            "pushd",
            "popd",
            "pwd",
            "read",
            "set",
            "shift",
            "shopt",
            "source",
            "test",
            "true",
            "false",
            "command",
            "eval",
            "exit",
            "export",
            "cd",
            "dirs",
            "hash",
            "kill",
            "let",
            "local",
            "math",
            "select",
            "getopts",
            "realpath",
            "regexmatch",
            "declare",
            "typeset",
            "help",
            "ulimit",
            "logout",
            "mapfile",
            "readarray",
            "compgen",
            "complete",
        ];
        for b in builtins {
            if !candidates.contains(&b.to_string()) {
                candidates.push(b.to_string());
            }
        }
    }

    if do_aliases {
        for k in env.all_aliases().keys() {
            if !candidates.contains(k) {
                candidates.push(k.clone());
            }
        }
    }

    if do_vars {
        for k in env.all_vars().keys() {
            if !candidates.contains(k) {
                candidates.push(k.clone());
            }
        }
    }

    if do_files {
        let path_env = std::env::var("PATH").unwrap_or_default();
        let _ = path_env;
        let search_dir = if filter.contains('/') {
            Path::new(filter).parent().unwrap_or(Path::new("."))
        } else {
            Path::new(".")
        };
        let prefix = if filter.contains('/') {
            filter.rsplit('/').next().unwrap_or("")
        } else {
            filter
        };
        if let Ok(entries) = std::fs::read_dir(search_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(prefix) {
                    candidates.push(name);
                }
            }
        }
    }

    if do_dirs {
        let search_dir = if filter.contains('/') {
            Path::new(filter).parent().unwrap_or(Path::new("."))
        } else {
            Path::new(".")
        };
        let prefix = if filter.contains('/') {
            filter.rsplit('/').next().unwrap_or("")
        } else {
            filter
        };
        if let Ok(entries) = std::fs::read_dir(search_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(prefix) {
                        candidates.push(name);
                    }
                }
            }
        }
    }

    if do_commands {
        let path_env = std::env::var("PATH").unwrap_or_default();
        for dir in path_env.split(':') {
            if dir.is_empty() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(filter) && !candidates.contains(&name) {
                        candidates.push(name);
                    }
                }
            }
        }
    }

    if !filter.is_empty() && !do_files && !do_dirs && !do_commands {
        candidates.retain(|c| c.starts_with(filter));
    }

    candidates.sort();
    candidates.dedup();

    for c in &candidates {
        println!("{}", c);
    }

    if candidates.is_empty() {
        BuiltinResult::err(1)
    } else {
        BuiltinResult::ok()
    }
}

fn cmd_complete(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!(
            "context: complete: usage: complete [-F function | -C command] [-p] [-r] name ..."
        );
        return BuiltinResult::err(1);
    }

    let mut print_mode = false;
    let mut remove_mode = false;
    let mut function: Option<String> = None;
    let mut command: Option<String> = None;
    let mut names: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => {
                print_mode = true;
                i += 1;
            }
            "-r" => {
                remove_mode = true;
                i += 1;
            }
            "-F" => {
                i += 1;
                if i < args.len() {
                    function = Some(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("context: complete: -F requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "-C" => {
                i += 1;
                if i < args.len() {
                    command = Some(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("context: complete: -C requires an argument");
                    return BuiltinResult::err(2);
                }
            }
            "--" => {
                i += 1;
                while i < args.len() {
                    names.push(args[i].clone());
                    i += 1;
                }
            }
            _ if args[i].starts_with('-') => {
                eprintln!("context: complete: {}: invalid option", &args[i][..]);
                return BuiltinResult::err(2);
            }
            _ => {
                names.push(args[i].clone());
                i += 1;
            }
        }
    }

    if print_mode {
        let completions = COMPLETIONS.lock().unwrap();
        if names.is_empty() {
            for (name, spec) in completions.iter() {
                match spec {
                    CompletionSpec::Function(f) => println!("complete -F {} {}", f, name),
                    CompletionSpec::Command(c) => println!("complete -C {} {}", c, name),
                }
            }
        } else {
            for name in &names {
                if let Some(spec) = completions.get(name) {
                    match spec {
                        CompletionSpec::Function(f) => println!("complete -F {} {}", f, name),
                        CompletionSpec::Command(c) => println!("complete -C {} {}", c, name),
                    }
                } else {
                    eprintln!("context: complete: {}: no completion specification", name);
                    return BuiltinResult::err(1);
                }
            }
        }
        return BuiltinResult::ok();
    }

    if remove_mode {
        let mut completions = COMPLETIONS.lock().unwrap();
        if names.is_empty() {
            completions.clear();
        } else {
            for name in &names {
                completions.remove(name);
            }
        }
        return BuiltinResult::ok();
    }

    if names.is_empty() {
        eprintln!("context: complete: command name argument required");
        return BuiltinResult::err(1);
    }

    if function.is_some() && command.is_some() {
        eprintln!("context: complete: -F and -C are mutually exclusive");
        return BuiltinResult::err(2);
    }

    if function.is_none() && command.is_none() {
        eprintln!("context: complete: -F or -C option required");
        return BuiltinResult::err(2);
    }

    let mut completions = COMPLETIONS.lock().unwrap();
    if let Some(f) = &function {
        for name in &names {
            completions.insert(name.clone(), CompletionSpec::Function(f.clone()));
        }
    } else if let Some(c) = &command {
        for name in &names {
            completions.insert(name.clone(), CompletionSpec::Command(c.clone()));
        }
    }
    BuiltinResult::ok()
}

static BUILTIN_NAMES: &[&str] = &[
    "cd",
    "exit",
    "export",
    "unset",
    "alias",
    "unalias",
    "source",
    "history",
    "set",
    "unsetenv",
    "env",
    "pwd",
    "type",
    "which",
    "echo",
    "printf",
    "true",
    "false",
    "shift",
    "test",
    "let",
    "trap",
    "pushd",
    "popd",
    "dirs",
    "hash",
    "math",
    "regexmatch",
    "module",
    "readonly",
    "local",
    "builtin",
    "caller",
    "shopt",
    "declare",
    "typeset",
    "wait",
    "kill",
    "umask",
    "command",
    "eval",
    "select",
    "getopts",
    "realpath",
    "read",
    "bindkey",
    "enable",
    "help",
    "ulimit",
    "times",
    "logout",
    "break",
    "continue",
    "mapfile",
    "readarray",
    "compgen",
    "complete",
    "suspend",
    ":",
    ".",
];

fn find_word_start(input: &str, cursor: usize) -> usize {
    let chars: Vec<char> = input.chars().collect();
    if cursor == 0 {
        return 0;
    }
    let mut i = cursor;
    while i > 0 {
        let ch = chars[i - 1];
        if ch == ' '
            || ch == '\t'
            || ch == '\n'
            || ch == '|'
            || ch == '&'
            || ch == ';'
            || ch == '('
            || ch == '{'
        {
            return i;
        }
        i -= 1;
    }
    0
}

fn path_completions(word: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let (search_dir, prefix) = if let Some(rest) = word.strip_prefix("~/") {
        let slash_pos = rest.find('/');
        match slash_pos {
            Some(pos) => {
                let dir_part = &rest[..pos];
                let prefix_part = &rest[pos + 1..];
                (
                    std::path::PathBuf::from(format!("{}/{}", home, dir_part)),
                    prefix_part.to_string(),
                )
            }
            None => (std::path::PathBuf::from(&home), rest.to_string()),
        }
    } else if word.starts_with('~') {
        (std::path::PathBuf::from(&home), String::new())
    } else if let Some(rest) = word.strip_prefix('/') {
        match word.rfind('/') {
            Some(0) => (std::path::PathBuf::from("/"), rest.to_string()),
            Some(pos) => (
                std::path::PathBuf::from(&word[..pos]),
                word[pos + 1..].to_string(),
            ),
            None => (std::path::PathBuf::from("."), word.to_string()),
        }
    } else if let Some(pos) = word.find('/') {
        (
            std::path::PathBuf::from(&word[..pos]),
            word[pos + 1..].to_string(),
        )
    } else {
        (std::path::PathBuf::from("."), word.to_string())
    };

    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                let full_path = search_dir.join(&name);
                if full_path.is_dir() {
                    results.push(format!("{}/", name));
                } else {
                    results.push(format!("{} ", name));
                }
            }
        }
    }
    results
}

fn command_completions(word: &str) -> Vec<String> {
    let mut results = Vec::new();
    for &b in BUILTIN_NAMES {
        if b.starts_with(word) {
            results.push(b.to_string());
        }
    }
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        if dir.is_empty() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(word) && !results.contains(&name) {
                    results.push(name);
                }
            }
        }
    }
    results
}

fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub fn get_completions(input: &str, cursor: usize) -> Vec<String> {
    let word_start = find_word_start(input, cursor);
    let word: String = input
        .chars()
        .skip(word_start)
        .take(cursor - word_start)
        .collect();
    let line_so_far: String = input.chars().take(cursor).collect();
    let words = shell_words(&line_so_far);

    let is_start_of_word = cursor == 0
        || input
            .chars()
            .nth(cursor - 1)
            .is_some_and(|c| c == ' ' || c == '\t');

    if words.is_empty() || (is_start_of_word && (words.len() == 1 || cursor <= word_start)) {
        let mut results = command_completions(&word);
        let mut path_results = path_completions(&word);
        results.append(&mut path_results);
        results.sort();
        results.dedup();
        return results;
    }

    let cmd_name = words[0].clone();
    let completions_map = COMPLETIONS.lock().unwrap();
    if let Some(spec) = completions_map.get(&cmd_name) {
        match spec {
            CompletionSpec::Function(_fname) => {
                return Vec::new();
            }
            CompletionSpec::Command(ext_cmd) => {
                let comp_words = words.join(" ");
                let cword = if !words.is_empty() {
                    words.len() - 1
                } else {
                    0
                };
                let output = Command::new(ext_cmd)
                    .arg(&word)
                    .arg(&cmd_name)
                    .arg(&comp_words)
                    .arg(cword.to_string())
                    .output();
                drop(completions_map);
                match output {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        let mut results: Vec<String> =
                            stdout.lines().map(|l| l.to_string()).collect();
                        results.retain(|r| !r.is_empty());
                        results.sort();
                        results.dedup();
                        return results;
                    }
                    Err(_) => return Vec::new(),
                }
            }
        }
    }
    drop(completions_map);

    let mut results = path_completions(&word);
    results.sort();
    results.dedup();
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> Env {
        Env::new()
    }

    fn run_builtin(cmd: &str, args: &[String], env: &mut Env) -> BuiltinResult {
        let cfg = Config::default();
        let mut full_args = vec![cmd.to_string()];
        full_args.extend_from_slice(args);
        run(&full_args, env, &cfg, 0)
    }

    #[test]
    fn test_getopts_advances_past_double_dash() {
        let mut env = make_env();
        env.set("OPTIND", "1");
        let args: Vec<String> = vec![
            "ab:".into(),
            "OPT".into(),
            "-a".into(),
            "--".into(),
            "foo".into(),
        ];
        let result = run_builtin("getopts", &args, &mut env);
        assert_eq!(result.status, 0);
        assert_eq!(env.get("OPT").expect("OPT set"), "a");
        assert_eq!(env.get("OPTIND").expect("OPTIND set"), "2");

        let result2 = run_builtin("getopts", &args, &mut env);
        assert_eq!(result2.status, 1);
        assert_eq!(env.get("OPTIND").expect("OPTIND set"), "3");
    }

    #[test]
    fn test_realpath_current_dir() {
        let mut env = make_env();
        let result = run_builtin("realpath", &[".".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_realpath_nonexistent() {
        let mut env = make_env();
        let result = run_builtin("realpath", &["/nonexistent/path/xyz".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_realpath_no_args() {
        let mut env = make_env();
        let result = run_builtin("realpath", &[], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_pushd_plus_zero() {
        let mut env = make_env();
        env.set("DIRSTACK", "/tmp\n/home");
        let result = run_builtin("pushd", &["+0".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_pushd_minus_one() {
        let mut env = make_env();
        env.set("DIRSTACK", "/tmp\n/home\n/var");
        let result = run_builtin("pushd", &["-n".into(), "-1".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_dirs_long_format() {
        let mut env = make_env();
        let result = run_builtin("dirs", &["-l".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_dirs_long_with_stack() {
        let mut env = make_env();
        env.set("DIRSTACK", "/tmp\n/var");
        let result = run_builtin("dirs", &["-l".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_dirs_one_per_line() {
        let mut env = make_env();
        env.set("DIRSTACK", "/tmp\n/var");
        let result = run_builtin("dirs", &["-p".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_dirs_numbered() {
        let mut env = make_env();
        env.set("DIRSTACK", "/tmp");
        let result = run_builtin("dirs", &["-v".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_hash_nonexistent() {
        let mut env = make_env();
        let result = run_builtin(
            "hash",
            &["totally_nonexistent_cmd_xyz_999".into()],
            &mut env,
        );
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_hash_clear() {
        let mut env = make_env();
        let result = run_builtin("hash", &["-r".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_hash_print_empty() {
        let mut env = make_env();
        PATH_CACHE.lock().unwrap().clear();
        let result = run_builtin("hash", &[], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_compgen_c() {
        let mut env = make_env();
        let result = run_builtin("compgen", &["-c".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_compgen_b() {
        let mut env = make_env();
        let result = run_builtin("compgen", &["-b".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_complete_p_empty() {
        let mut env = make_env();
        let result = run_builtin("complete", &[], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_enable_disable_builtin() {
        let mut env = make_env();
        let cfg = Config::default();
        let args_disable: Vec<String> = vec!["enable".into(), "-n".into(), "echo".into()];
        let result = run(&args_disable, &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
        assert!(is_disabled("echo"));

        let args_enable: Vec<String> = vec!["enable".into(), "echo".into()];
        let result = run(&args_enable, &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
        assert!(!is_disabled("echo"));
    }

    #[test]
    fn test_enable_list() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(&["enable".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_enable_nonexistent() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(
            &[
                "enable".into(),
                "-n".into(),
                "not_a_real_builtin_xyz".into(),
            ],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_echo_n_flag() {
        let result = cmd_echo(&["-n".into(), "hello".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_echo_escape() {
        let result = cmd_echo(&["-e".into(), "hello\\nworld".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_shift_too_many() {
        let mut env = make_env();
        env.set_positional(vec!["a".into()]);
        let result = run_builtin("shift", &["5".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_shift_zero() {
        let mut env = make_env();
        env.set_positional(vec!["a".into(), "b".into()]);
        let result = run_builtin("shift", &["0".into()], &mut env);
        assert_eq!(result.status, 0);
        assert_eq!(env.positional().len(), 2);
    }

    #[test]
    fn test_test_string_equality() {
        let result = cmd_test(&["hello".into(), "=".into(), "hello".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_string_inequality() {
        let result = cmd_test(&["hello".into(), "=".into(), "world".into()]);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_test_numeric_eq() {
        let result = cmd_test(&["5".into(), "-eq".into(), "5".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_numeric_lt() {
        let result = cmd_test(&["3".into(), "-lt".into(), "5".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_empty_args() {
        let result = cmd_test(&[]);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_test_not_operator() {
        let result = cmd_test(&["!".into(), "false".into()]);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_test_file_exists() {
        let result = cmd_test(&["-e".into(), ".".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_string_not_empty() {
        let result = cmd_test(&["-n".into(), "hello".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_string_empty() {
        let result = cmd_test(&["-z".into(), "".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_let_zero_returns_one() {
        let mut env = make_env();
        let result = run_builtin("let", &["0".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_let_nonzero_returns_ok() {
        let mut env = make_env();
        let result = run_builtin("let", &["1".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_unset_nonexistent() {
        let mut env = make_env();
        let result = run_builtin("unset", &["NONEXISTENT_VAR_XYZ".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_unset_readonly() {
        let mut env = make_env();
        env.set("RO_VAR", "val");
        env.set_readonly("RO_VAR");
        let result = run_builtin("unset", &["RO_VAR".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_exit_no_args() {
        let result = cmd_exit(&[], 42);
        assert!(result.exit);
        assert_eq!(result.exit_code, Some(42));
    }

    #[test]
    fn test_exit_with_code() {
        let result = cmd_exit(&["7".into()], 0);
        assert!(result.exit);
        assert_eq!(result.exit_code, Some(7));
    }

    #[test]
    fn test_exit_too_many_args() {
        let result = cmd_exit(&["1".into(), "2".into()], 0);
        assert_eq!(result.status, 2);
    }

    #[test]
    fn test_exit_non_numeric() {
        let result = cmd_exit(&["abc".into()], 0);
        assert!(result.exit);
        assert_eq!(result.exit_code, Some(2));
    }

    #[test]
    fn test_true_builtin() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(&["true".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_false_builtin() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(&["false".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_colon_builtin() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(&[":".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_type_builtin_echo() {
        let mut env = make_env();
        let result = run_builtin("type", &["echo".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_type_not_found() {
        let mut env = make_env();
        let result = run_builtin(
            "type",
            &["totally_nonexistent_cmd_xyz_999".into()],
            &mut env,
        );
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_alias_set_and_get() {
        let mut env = make_env();
        let result = run_builtin("alias", &["ll=ls -la".into()], &mut env);
        assert_eq!(result.status, 0);
        assert_eq!(env.get_alias("ll"), Some("ls -la"));
    }

    #[test]
    fn test_alias_not_found() {
        let mut env = make_env();
        let result = run_builtin("alias", &["nonexistent_alias_xyz".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_unalias_nonexistent() {
        let mut env = make_env();
        let result = run_builtin("unalias", &["nonexistent_alias_xyz".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_set_no_args() {
        let mut env = make_env();
        let result = run_builtin("set", &[], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_set_string_var() {
        let mut env = make_env();
        let result = run_builtin("set", &["FOO=bar".into()], &mut env);
        assert_eq!(result.status, 0);
        assert_eq!(env.get("FOO"), Some("bar"));
    }

    #[test]
    fn test_export_and_unexport() {
        let mut env = make_env();
        let result = run_builtin("export", &["MYTESTVAR=hello".into()], &mut env);
        assert_eq!(result.status, 0);
        assert!(env.is_exported("MYTESTVAR"));
        let result = run_builtin("export", &["-n".into(), "MYTESTVAR".into()], &mut env);
        assert_eq!(result.status, 0);
        assert!(!env.is_exported("MYTESTVAR"));
    }

    #[test]
    fn test_unset_env_var() {
        let mut env = make_env();
        env.set("TEMPVAR", "tempval");
        let result = run_builtin("unsetenv", &["TEMPVAR".into()], &mut env);
        assert_eq!(result.status, 0);
        assert!(env.get("TEMPVAR").is_none());
    }

    #[test]
    fn test_pwd_builtin() {
        let mut env = make_env();
        let result = run_builtin("pwd", &[], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_pwd_logical() {
        let mut env = make_env();
        let result = run_builtin("pwd", &["-L".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_math_basic() {
        let mut env = make_env();
        let cfg = Config::default();
        let mut full_args = vec![
            "math".to_string(),
            "2".to_string(),
            "+".to_string(),
            "3".to_string(),
        ];
        full_args.drain(0..0);
        let result = run(&["math".into(), "2+3".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_math_empty() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(&["math".into()], &mut env, &cfg, 0);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_times_builtin() {
        let result = cmd_times();
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_shift_negative() {
        let mut env = make_env();
        let result = run_builtin("shift", &["-1".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_shift_non_numeric() {
        let mut env = make_env();
        let result = run_builtin("shift", &["abc".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_test_file_nonexistent() {
        let result = cmd_test(&["-e".into(), "/nonexistent_path_xyz".into()]);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_test_file_readable() {
        let result = cmd_test(&["-r".into(), ".".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_file_writable() {
        let result = cmd_test(&["-w".into(), ".".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_string_ne() {
        let result = cmd_test(&["hello".into(), "!=".into(), "world".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_numeric_gt() {
        let result = cmd_test(&["5".into(), "-gt".into(), "3".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_numeric_le() {
        let result = cmd_test(&["3".into(), "-le".into(), "3".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_test_numeric_ge() {
        let result = cmd_test(&["5".into(), "-ge".into(), "3".into()]);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_type_alias() {
        let mut env = make_env();
        run_builtin("alias", &["ll=ls -la".into()], &mut env);
        let result = run_builtin("type", &["ll".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_printf_basic() {
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(
            &["printf".into(), "%s\n".into(), "hello".into()],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(result.status, 0);
    }

    // C1: arithmetic comparisons, shifts and conditional operator must flow
    // through the full precedence chain (comma → conditional → …).
    #[test]
    fn test_arithmetic_comparisons() {
        let env = make_env();
        assert_eq!(eval_arithmetic("3 < 5", &env), 1);
        assert_eq!(eval_arithmetic("5 < 3", &env), 0);
        assert_eq!(eval_arithmetic("3 > 5", &env), 0);
        assert_eq!(eval_arithmetic("2 <= 2", &env), 1);
        assert_eq!(eval_arithmetic("3 >= 4", &env), 0);
        assert_eq!(eval_arithmetic("1 == 1", &env), 1);
        assert_eq!(eval_arithmetic("1 != 1", &env), 0);
        assert_eq!(eval_arithmetic("(3 < 5) && (2 > 1)", &env), 1);
        assert_eq!(eval_arithmetic("(3 < 5) || (2 < 1)", &env), 1);
        assert_eq!(eval_arithmetic("1 << 4", &env), 16);
        assert_eq!(eval_arithmetic("256 >> 4", &env), 16);
        assert_eq!(eval_arithmetic("1 ? 10 : 20", &env), 10);
        assert_eq!(eval_arithmetic("0 ? 10 : 20", &env), 20);
        assert_eq!(eval_arithmetic("1 + 2 * 3", &env), 7);
        assert_eq!(eval_arithmetic("(1 + 2) * 3", &env), 9);
    }

    // C7: printf `%.*s` must truncate on char boundaries, never on bytes.
    #[test]
    fn test_printf_precision_multibyte() {
        let mut env = make_env();
        let cfg = Config::default();
        let emoji = "\u{1F600}x";
        let r = run(
            &["printf".into(), "%.2s\n".into(), emoji.into()],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(r.status, 0);
        let r = run(
            &["printf".into(), "%.1s".into(), emoji.into()],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(r.status, 0);
        let r = run(
            &[
                "printf".into(),
                "%.2s|%.0s\n".into(),
                emoji.into(),
                "y".into(),
            ],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(r.status, 0);
    }

    // M9: every dispatched builtin is reachable through BUILTINS.
    #[test]
    fn test_builtins_dispatch_list() {
        for name in [
            "mapfile",
            "readarray",
            "help",
            "ulimit",
            "times",
            "logout",
            "suspend",
            "printf",
        ] {
            assert!(BUILTINS.contains(&name), "{} missing from BUILTINS", name);
        }
        let mut env = make_env();
        let cfg = Config::default();
        let result = run(
            &["printf".into(), "%d\n".into(), "5".into()],
            &mut env,
            &cfg,
            0,
        );
        assert_eq!(result.status, 0);
    }

    // M10: indexed arrays store arr_N elements; @/# expansion reads them.
    #[test]
    fn test_indexed_array_read_elements() {
        let mut env = make_env();
        env.indexed_array_set("arr", "0", "one");
        env.indexed_array_set("arr", "1", "two");
        assert!(env.is_indexed_array("arr"));
        assert_eq!(env.indexed_array_get("arr", "0"), Some("one"));
        assert_eq!(env.indexed_array_get("arr", "1"), Some("two"));
        assert_eq!(env.indexed_array_len("arr"), 2);
        assert_eq!(env.indexed_array_elements("arr"), vec!["one", "two"]);
        let bg_pid = 0;
        {
            let mut expander = crate::shell::expand::Expander::new(&mut env, 0, vec![], bg_pid);
            assert_eq!(expander.expand_word("${arr[@]}"), "one two");
            assert_eq!(expander.expand_word("${arr[*]}"), "one two");
            assert_eq!(expander.expand_word("${arr[#]}"), "2");
            assert_eq!(expander.expand_word("${arr[0]}"), "one");
        }
    }

    // M24: `cd -L` builds a logical PWD (no symlink resolution).
    #[test]
    fn test_logical_path_cd_l() {
        assert_eq!(logical_path("/tmp", "/tmp/symtest"), "/tmp/symtest");
        assert_eq!(logical_path("/home/user", "docs"), "/home/user/docs");
        assert_eq!(logical_path("/home/user", ".."), "/home");
        assert_eq!(logical_path("/", "etc"), "/etc");
        assert_eq!(logical_path("/a/b/", "c"), "/a/b/c");
    }
}
