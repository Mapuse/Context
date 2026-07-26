use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::config::Config;
use crate::shell::ast::*;
use crate::shell::builtin;
use crate::shell::env::Env;
use crate::shell::expand::Expander;
use crate::shell::signals;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const MAX_ALIAS_EXPAND: usize = 10;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub pid: i32,
    pub cmd: String,
    pub running: bool,
}

type ExpandedCaseArm = (Vec<String>, Box<Node>);

pub struct Executor {
    pub env: Env,
    pub cfg: Config,
    pub last_status: i32,
    pub functions: std::collections::HashMap<String, Node>,
    pub jobs: Vec<Job>,
    pub clear_history: bool,
    pub disabled_builtins: std::collections::HashSet<String>,
    prev_cwd: String,
    next_job_id: usize,
    fork_count: Arc<AtomicUsize>,
}

impl Executor {
    pub fn new(env: Env, cfg: Config) -> Self {

        if let Ok(mask) = u32::from_str_radix(cfg.execution.umask.trim_start_matches('0'), 8) {
            unsafe { libc::umask(mask as libc::mode_t); }
        }

        if cfg.security.sanitize_path {
            if let Ok(path) = std::env::var("PATH") {
                let sanitized: String = path.split(':').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(":");
                std::env::set_var("PATH", &sanitized);
            }
        }

        let mut env = env;
        let default_keys: Vec<String> = cfg.environment.set_defaults.iter()
            .filter_map(|e| e.split_once('=').map(|(k, _)| k.to_string()))
            .collect();
        for entry in &cfg.environment.set_defaults {
            if let Some((key, value)) = entry.split_once('=') {
                env.set(key, value);
            }
        }
        if !cfg.environment.inherit_parent {
            env.clear_inherited(&default_keys);
        }
        Self {
            env,
            cfg,
            last_status: 0,
            functions: std::collections::HashMap::new(),
            jobs: Vec::new(),
            clear_history: false,
            disabled_builtins: std::collections::HashSet::new(),
            prev_cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            next_job_id: 1,
            fork_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn run_source_rc(&mut self) {
        let rc = crate::config::loader::rc_path();
        if rc.is_file() {
            if let Ok(contents) = std::fs::read_to_string(&rc) {
                let tokens = crate::shell::lexer::tokenize(&contents);
                let ast = crate::shell::parser::parse(tokens);
                self.execute(&ast);
            }
        }
    }

    pub fn run_integrations(&mut self) {
        let cfg = self.cfg.clone();

        if cfg.integration.enable_fzf && cfg.integration.fzf_key_bindings {
            let candidates = [
                dirs::home_dir().map(|h| h.join(".fzf/shell/key-bindings.bash")),
                Some(std::path::PathBuf::from("/usr/share/fzf/key-bindings.bash")),
                Some(std::path::PathBuf::from("/usr/share/doc/fzf/examples/key-bindings.bash")),
            ];
            for candidate in candidates.into_iter().flatten() {
                if candidate.is_file() {
                    if let Ok(contents) = std::fs::read_to_string(&candidate) {
                        let tokens = crate::shell::lexer::tokenize(&contents);
                        let ast = crate::shell::parser::parse(tokens);
                        self.execute(&ast);
                    }
                    break;
                }
            }
        }

        if cfg.integration.enable_zoxide && cfg.integration.zoxide_init {
            if let Ok(output) = std::process::Command::new("zoxide")
                .args(["init", "context"])
                .output()
            {
                if output.status.success() {
                    let init_script = String::from_utf8_lossy(&output.stdout).to_string();
                    if !init_script.is_empty() {
                        let tokens = crate::shell::lexer::tokenize(&init_script);
                        let ast = crate::shell::parser::parse(tokens);
                        self.execute(&ast);
                    }
                }
            }
        }

        if cfg.integration.starship_prompt
            && self.find_in_path("starship").is_some() {
                if let Ok(output) = std::process::Command::new("starship")
                    .args(["init", "bash"])
                    .output()
                {
                    if output.status.success() {
                        let init_script = String::from_utf8_lossy(&output.stdout).to_string();
                        let tokens = crate::shell::lexer::tokenize(&init_script);
                        let ast = crate::shell::parser::parse(tokens);
                        self.execute(&ast);
                    }
                }
            }
    }

    pub fn execute(&mut self, node: &Node) -> i32 {
        let status = self.execute_node(node);
        let current_cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if current_cwd != self.prev_cwd {
            self.prev_cwd = current_cwd;
        }
        if self.opt_e() && status != 0 {
            match node {
                Node::Empty | Node::If { .. } | Node::While { .. } | Node::Until { .. }
                | Node::Compound { .. } | Node::Function { .. } => {}
                _ => {
                    eprintln!("context: terminating on errexit (status {})", status);
                    signals::EXIT_CODE.store(status, std::sync::atomic::Ordering::SeqCst);
                    signals::SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
        status
    }

    fn execute_node(&mut self, node: &Node) -> i32 {
        match node {
            Node::Empty => 0,
            Node::Command { words, redirects, background } => {
                if words.is_empty() { return 0; }

                if self.cfg.security.restricted_mode {
                    let restricted = [
                        "exec", "eval", "source", ".", "kill", "env",
                        "export", "bash", "sh", "zsh", "fish",
                        "command", "builtin", "enable",
                    ];
                    if restricted.contains(&words[0].as_str()) {
                        eprintln!("context: restricted mode: {} not allowed", words[0]);
                        return 1;
                    }
                    if words[0] == "cd" && words.len() > 1
                        && (words[1] == "/" || words[1] == ".." || words[1].contains(".."))
                    {
                        eprintln!("context: restricted mode: cd to parent/root not allowed");
                        return 1;
                    }
                    if words[0] == "hash" {
                        eprintln!("context: restricted mode: hash not allowed");
                        return 1;
                    }
                }

                if self.cfg.security.audit_log {
                    self.audit_log(&words.join(" "));
                }

                if let Some(func_body) = self.functions.get(&words[0]).cloned() {
                    let saved = self.env.push_scope();
                    let nounset = self.opt_u();
                    let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                    let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                    expander.set_nounset(nounset);
                    let expanded_words: Vec<String> = words[1..].iter().map(|w| expander.expand_word(w)).collect();
                    let nounset_err = expander.had_nounset_error();
                    drop(expander);
                    self.env.set_positional(expanded_words.clone());
                    if !nounset_err {
                        for (i, arg) in expanded_words.iter().enumerate() {
                            self.env.set_local(&(i + 1).to_string(), arg);
                        }
                        self.env.set_local("@", &expanded_words.join(" "));
                        self.env.set_local("#", &expanded_words.len().to_string());
                    }
                    let status = if nounset_err { 1 } else { self.execute(&func_body) };
                    self.env.pop_scope(saved);
                    self.last_status = status;
                    signals::set_last_status(status);
                    return status;
                }
                let words = if self.cfg.editor.expand_aliases {
                    self.expand_aliases(words)
                } else {
                    words.clone()
                };
                let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                let nounset = self.opt_u();
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                expander.set_nounset(nounset);
                let words: Vec<String> = words.iter().map(|w| expander.expand_word(w)).collect();
                let pending = expander.take_pending_sets();
                let nounset_err = expander.had_nounset_error();
                drop(expander);
                for (k, v) in pending {
                    self.env.set(&k, &v);
                }
                if nounset_err { return 1; }
                let words = self.word_split(&words);
                let words = self.expand_globs(&words);
                if words.is_empty() { return 0; }
                self.trace_print(&words);

                let is_builtin = builtin_is(&words[0]) && !self.disabled_builtins.contains(&words[0]);
                if is_builtin {
                    match words[0].as_str() {
                        "exec" => {
                            if words.len() > 1 {

                                for r in redirects {
                                    self.apply_redirect(r);
                                }
                                if builtin_is(&words[1]) {
                                    return builtin::run(&words[1..], &mut self.env, &self.cfg, self.last_status).status;
                                }

                                let cmd = &words[1];
                                let path = if cmd.contains('/') {
                                    cmd.clone()
                                } else if let Some(p) = self.find_in_path(cmd) {
                                    p
                                } else {
                                    eprintln!("context: exec: {}: command not found", cmd);
                                    return 127;
                                };

                                match unsafe { libc::fork() } {
                                    -1 => { eprintln!("context: exec: fork failed"); return 1; }
                                    0 => {
                                        unsafe { libc::setpgid(0, 0); }
                                        self.child_exec(&words[1..], &[], &path);
                                    }
                                    pid => {
                                        let mut status: i32 = 0;
                                        unsafe { libc::waitpid(pid, &mut status, 0); }
                                        let exit = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
                                                   else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
                                                   else { 1 };

                                        signals::EXIT_CODE.store(exit, std::sync::atomic::Ordering::SeqCst);
                                        signals::SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
                                        return exit;
                                    }
                                }
                            }

                            for r in redirects {
                                self.apply_redirect(r);
                            }
                            return 0;
                        }
                        "jobs" => {
                            self.cleanup_jobs();
                            for job in &self.jobs {
                                let status = if job.running { "Running" } else { "Stopped" };
                                println!("[{}] {} {} {}", job.id, status, job.pid, job.cmd);
                            }
                            return 0;
                        }
                        "fg" => {
                            let job_id = words.get(1).and_then(|s| s.parse::<usize>().ok());
                            let job_opt = if let Some(id) = job_id {
                                self.jobs.iter().position(|j| j.id == id).map(|pos| self.jobs.remove(pos))
                            } else {
                                self.jobs.iter().rposition(|j| j.running).map(|pos| self.jobs.remove(pos))
                            };
                            if let Some(job) = job_opt {
                                unsafe { libc::kill(job.pid, libc::SIGCONT); }
                                let exit = self.wait_for_pid(job.pid);
                                if (147..=150).contains(&exit) {
                                    let new_job = Job {
                                        id: self.next_job_id,
                                        pid: job.pid,
                                        cmd: job.cmd.clone(),
                                        running: false,
                                    };
                                    self.next_job_id += 1;
                                    println!("[{}] {}", new_job.id, new_job.cmd);
                                    self.jobs.push(new_job);
                                }
                                self.last_status = exit;
                                signals::set_last_status(exit);
                                return exit;
                            }
                            eprintln!("context: fg: no such job");
                            return 1;
                        }
                        "bg" => {
                            let job_id = words.get(1).and_then(|s| s.parse::<usize>().ok());
                            if let Some(id) = job_id {
                                if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                                    unsafe {
                                        libc::kill(job.pid, libc::SIGCONT);
                                    }
                                    job.running = true;
                                    println!("[{}] {} &", job.id, job.cmd);
                                    return 0;
                                }
                            } else if let Some(job) = self.jobs.iter_mut().rev().find(|j| !j.running) {
                                unsafe {
                                    libc::kill(job.pid, libc::SIGCONT);
                                }
                                job.running = true;
                                println!("[{}] {} &", job.id, job.cmd);
                                return 0;
                            }
                            eprintln!("context: bg: no such job");
                            return 1;
                        }
                        "enable" => {
                            return self.cmd_enable(&words[1..]);
                        }
                        _ => {}
                    }
                    let words = if words[0] == "kill" {
                        let mut resolved = words.clone();
                        for arg in resolved.iter_mut().skip(2) {
                            if let Some(job_id_str) = arg.strip_prefix('%') {
                                if let Ok(job_id) = job_id_str.parse::<usize>() {
                                    if let Some(job) = self.jobs.iter().find(|j| j.id == job_id) {
                                        *arg = job.pid.to_string();
                                    }
                                }
                            }
                        }
                        resolved
                    } else {
                        words.clone()
                    };
                    let result = builtin::run(&words, &mut self.env, &self.cfg, self.last_status);
                    if result.clear_history {
                        self.clear_history = true;
                    }
                    if let Some(eval_code) = result.eval_string {
                        let tokens = crate::shell::lexer::tokenize(&eval_code);
                        let ast = crate::shell::parser::parse(tokens);
                        let status = self.execute(&ast);
                        self.last_status = status;
                        signals::set_last_status(status);
                        return status;
                    }
                    if let Some(path) = result.source_file {
                        let status = self.source_file(&path);
                        self.last_status = status;
                        signals::set_last_status(status);
                        return status;
                    }
                    let status = result.status;
                    self.last_status = status;
                    signals::set_last_status(status);
                    if result.exit {
                        let code = result.exit_code.unwrap_or(status);
                        signals::EXIT_CODE.store(code, std::sync::atomic::Ordering::SeqCst);
                        signals::SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
                        return code;
                    }
                    return status;
                }

                if *background {
                    self.exec_background(&words, redirects);
                    return 0;
                }

                self.exec_external(&words, redirects)
            }
            Node::Pipeline { commands, bang } => {
                let n = commands.len();
                if n == 0 { return 0; }
                if n == 1 { return self.execute(&commands[0]); }
                let mut pipes: Vec<[i32; 2]> = Vec::new();
                for _ in 0..n-1 {
                    let mut fds = [0i32; 2];
                    unsafe { libc::pipe(fds.as_mut_ptr()); }
                    pipes.push(fds);
                }
                let mut children: Vec<i32> = Vec::new();
                for (i, cmd) in commands.iter().enumerate() {
                    match unsafe { libc::fork() } {
                        -1 => {
                            for p in &pipes {
                                unsafe { libc::close(p[0]); libc::close(p[1]); }
                            }
                            for pid in &children {
                                unsafe { libc::kill(*pid, libc::SIGTERM); }
                                let mut status: i32 = 0;
                                unsafe { libc::waitpid(*pid, &mut status, 0); }
                            }
                            return 1;
                        }
                        0 => {
                            unsafe {
                                if i > 0 { libc::dup2(pipes[i-1][0], libc::STDIN_FILENO); }
                                if i < n-1 { libc::dup2(pipes[i+1][1], libc::STDOUT_FILENO); }
                                for p in &pipes { libc::close(p[0]); libc::close(p[1]); }
                                libc::setpgid(0, 0);
                            }
                            signals::setup_child_handlers();
                            let status = self.execute(cmd);
                            std::process::exit(status);
                        }
                        pid => {
                            children.push(pid);
                            unsafe {
                                if i > 0 { libc::close(pipes[i-1][0]); }
                                if i < n-1 { libc::close(pipes[i][1]); }
                            }
                        }
                    }
                }
                for p in &pipes { unsafe { libc::close(p[0]); libc::close(p[1]); } }
                let mut last_status = 0;
                let mut any_nonzero = 0;
                for pid in children {
                    let mut status: i32 = 0;
                    unsafe { libc::waitpid(pid, &mut status, 0); }
                    let exit = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
                                 else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
                                 else { 1 };
                    if exit != 0 && any_nonzero == 0 {
                        any_nonzero = exit;
                    }
                    last_status = exit;
                }
                if (self.opt_pipefail() || self.cfg.execution.exit_on_pipefail) && any_nonzero != 0 {
                    last_status = any_nonzero;
                }
                if *bang { last_status = if last_status == 0 { 1 } else { 0 }; }
                self.last_status = last_status;
                signals::set_last_status(last_status);
                last_status
            }
            Node::Compound { kind, left, right } => {
                match kind {
                    CompoundKind::And => {
                        let s = self.execute(left);
                        let status = if s == 0 { self.execute(right) } else { s };
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                    CompoundKind::Or => {
                        let s = self.execute(left);
                        let status = if s != 0 { self.execute(right) } else { s };
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                    CompoundKind::Semicolon => {
                        self.execute(left);
                        let status = self.execute(right);
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                }
            }
            Node::Subshell { body } => {
                match unsafe { libc::fork() } {
                    -1 => { eprintln!("context: fork failed"); 1 }
                    0 => {
                        signals::setup_child_handlers();
                        let status = self.execute(body);
                        std::process::exit(status);
                    }
                    pid => {
                        unsafe {
                            libc::setpgid(pid, pid);
                            libc::tcsetpgrp(libc::STDIN_FILENO, pid);
                        }
                        signals::CHILD_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
                        signals::RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
                        let mut status: i32 = 0;
                        loop {
                            let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
                            if ret != -1 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                                break;
                            }
                        }
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                        }
                        signals::RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                        signals::CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
                        signals::set_foreground(0);
                        let exit = if libc::WIFEXITED(status) {
                            libc::WEXITSTATUS(status)
                        } else if libc::WIFSIGNALED(status) {
                            128 + libc::WTERMSIG(status)
                        } else { 1 };
                        self.last_status = exit;
                        signals::set_last_status(exit);
                        exit
                    }
                }
            }
            Node::BraceGroup { body } => {
                let status = self.execute(body);
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::Assignment { name, value } => {
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                let value = expander.expand_word(value);
                let pending = expander.take_pending_sets();
                drop(expander);
                for (k, v) in pending {
                    self.env.set(&k, &v);
                }
                self.env.set(name, &value);
                0
            }
            Node::For { var, values, body } => {
                let positional: Vec<String> = if values.is_empty() {
                    (1..).map_while(|i| self.env.get(&i.to_string()).map(|s| s.to_string())).collect()
                } else {
                    vec![]
                };
                let iter_values: Vec<String> = if values.is_empty() {
                    positional
                } else {
                    let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                    values.iter().map(|v| expander.expand_word(v)).collect()
                };
                let mut last = 0;
                for val in &iter_values {
                    self.env.set(var, val);
                    last = self.execute(body);
                }
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::While { condition, body } => {
                let mut last = 0;
                loop {
                    let status = self.execute(condition);
                    if status != 0 { break; }
                    last = self.execute(body);
                }
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::Until { condition, body } => {
                let mut last = 0;
                loop {
                    let status = self.execute(condition);
                    if status == 0 { break; }
                    last = self.execute(body);
                }
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::If { condition, then_body, elif, else_body } => {
                let status = self.execute(condition);
                if status == 0 {
                    let s = self.execute(then_body);
                    self.last_status = s;
                    signals::set_last_status(s);
                    return s;
                }
                for (cond, body) in elif {
                    let s = self.execute(cond);
                    if s == 0 {
                        let s = self.execute(body);
                        self.last_status = s;
                        signals::set_last_status(s);
                        return s;
                    }
                }
                if let Some(else_b) = else_body {
                    let s = self.execute(else_b);
                    self.last_status = s;
                    signals::set_last_status(s);
                    return s;
                }
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::Case { word, arms } => {
                let (expanded_word, expanded_arms): (String, Vec<ExpandedCaseArm>) = {
                    let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                    let word = expander.expand_word(word);
                    let arms_expanded: Vec<(Vec<String>, Box<Node>)> = arms.iter().map(|(patterns, body)| {
                        let expanded: Vec<String> = patterns.iter().map(|p| expander.expand_word(p)).collect();
                        (expanded, body.clone())
                    }).collect();
                    (word, arms_expanded)
                };
                for (patterns, body) in expanded_arms {
                    for pat in &patterns {
                        if pat == &expanded_word || glob_match(pat, &expanded_word) {
                            let s = self.execute(&body);
                            self.last_status = s;
                            signals::set_last_status(s);
                            return s;
                        }
                    }
                }
                0
            }
            Node::Function { name, body } => {
                self.functions.insert(name.clone(), *body.clone());
                0
            }
            Node::TestDoubleBracket { tokens } => {
                let nounset = self.opt_u();
                let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                expander.set_nounset(nounset);
                let expanded: Vec<String> = tokens.iter().map(|t| expander.expand_word(t)).collect();
                let nounset_err = expander.had_nounset_error();
                drop(expander);
                if nounset_err { return 2; }
                let status = eval_test_bracket(&expanded);
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::Select { var, values, body } => {
                let iter_values: Vec<String> = if values.is_empty() || (values.len() == 1 && values[0] == "$@") {
                    self.env.positional().to_vec()
                } else {
                    let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                    values.iter().map(|v| expander.expand_word(v)).collect()
                };
                let stdin = std::io::stdin();
                let mut last = 0;
                loop {
                    for (i, item) in iter_values.iter().enumerate() {
                        println!("  {}) {}", i + 1, item);
                    }
                    let ps3 = self.env.get("PS3").unwrap_or("#?");
                    eprint!("{} ", ps3);
                    let _ = std::io::stderr().flush();
                    let mut line = String::new();
                    match stdin.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let line = line.trim().to_string();
                            if line.is_empty() { continue; }
                            if line == "EOF" || line == "quit" || line == "exit" { break; }
                            if let Ok(n) = line.parse::<usize>() {
                                if n > 0 && n <= iter_values.len() {
                                    self.env.set(var, &iter_values[n - 1]);
                                    last = self.execute(body);
                                    break;
                                }
                            }
                            eprintln!("context: select: invalid selection");
                        }
                        Err(_) => break,
                    }
                }
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
        }
    }

    fn format_done_msg(&self, command: &str, exit: i32) -> String {
        self.cfg.jobs.done_format.expand(&[
            ("command", command),
            ("exit_code", &exit.to_string()),
        ])
    }

    fn exec_external(&mut self, words: &[String], redirects: &[Redirect]) -> i32 {
        let cmd = &words[0];
        let path = if cmd.contains('/') {
            cmd.clone()
        } else if let Some(p) = self.find_in_path(cmd) {
            p
        } else if self.cfg.editor.auto_cd && !cmd.starts_with('-') && Path::new(cmd).is_dir() {
            let args = vec!["cd".to_string(), cmd.clone()];
            return builtin::run(&args, &mut self.env, &self.cfg, self.last_status).status;
        } else {
            if self.cfg.execution.cdspell {
                if let Some(suggestion) = spell_correct(cmd) {
                    eprintln!("context: {}: command not found. Did you mean '{}'?", cmd, suggestion);
                } else {
                    eprintln!("context: {}: command not found", cmd);
                }
            } else {
                eprintln!("context: {}: command not found", cmd);
            }
            return 127;
        };

        let max_forks = self.cfg.execution.max_forks_per_command as usize;
        if max_forks > 0 && self.fork_count.load(Ordering::SeqCst) >= max_forks {
            eprintln!("context: fork limit reached");
            return 125;
        }

        if self.cfg.jobs.max_jobs > 0 && self.jobs.len() >= self.cfg.jobs.max_jobs as usize {
            eprintln!("context: job limit reached (max {})", self.cfg.jobs.max_jobs);
            return 125;
        }

        if self.cfg.security.no_exec_commands.contains(&words[0]) {
            eprintln!("context: {}: command blocked by security policy", words[0]);
            return 1;
        }

        let use_posix_spawn = self.cfg.execution.fork_method == "spawn";

        if use_posix_spawn {
            let mut c_args: Vec<CString> = words.iter()
                .filter_map(|w| CString::new(w.as_str()).ok())
                .collect();
            let mut c_ptrs: Vec<*mut libc::c_char> = c_args.iter_mut().map(|s| s.as_ptr() as *mut libc::c_char).collect();
            c_ptrs.push(std::ptr::null_mut());

            let mut env_vars: Vec<CString> = self.env.all_vars().iter()
                .filter_map(|(k, v)| CString::new(format!("{}={}", k, v)).ok())
                .collect();
            let mut env_ptrs: Vec<*mut libc::c_char> = env_vars.iter_mut().map(|s| s.as_ptr() as *mut libc::c_char).collect();
            env_ptrs.push(std::ptr::null_mut());

            let c_path = CString::new(path.as_str()).unwrap_or_else(|_| CString::new("sh").unwrap());

            let mut file_actions: libc::posix_spawn_file_actions_t = unsafe { std::mem::zeroed() };
            let mut attr: libc::posix_spawnattr_t = unsafe { std::mem::zeroed() };
            unsafe {
                libc::posix_spawn_file_actions_init(&mut file_actions);
                libc::posix_spawnattr_init(&mut attr);
                for r in redirects {
                    match r.kind {
                        RedirKind::Output | RedirKind::OutputFd | RedirKind::Clobber => {
                            if let Ok(file) = File::create(&r.target) {
                                libc::posix_spawn_file_actions_adddup2(&mut file_actions, file.as_raw_fd(), libc::STDOUT_FILENO);
                                if matches!(r.kind, RedirKind::OutputFd | RedirKind::Clobber) {
                                    libc::posix_spawn_file_actions_adddup2(&mut file_actions, file.as_raw_fd(), libc::STDERR_FILENO);
                                }
                            }
                        }
                        RedirKind::OutputAppend | RedirKind::OutputFdAppend => {
                            if let Ok(file) = OpenOptions::new().create(true).append(true).open(&r.target) {
                                libc::posix_spawn_file_actions_adddup2(&mut file_actions, file.as_raw_fd(), libc::STDOUT_FILENO);
                                if matches!(r.kind, RedirKind::OutputFdAppend) {
                                    libc::posix_spawn_file_actions_adddup2(&mut file_actions, file.as_raw_fd(), libc::STDERR_FILENO);
                                }
                            }
                        }
                        RedirKind::Input | RedirKind::InputFd => {
                            let fd = if let RedirKind::InputFd = r.kind {
                                r.target.parse::<i32>().unwrap_or(-1)
                            } else {
                                File::open(&r.target).map(|f| f.as_raw_fd()).unwrap_or(-1)
                            };
                            if fd >= 0 {
                                libc::posix_spawn_file_actions_adddup2(&mut file_actions, fd, libc::STDIN_FILENO);
                            }
                        }
                        _ => {}
                    }
                }
            }

            self.fork_count.fetch_add(1, Ordering::SeqCst);
            let mut child_pid: libc::pid_t = 0;
            let ret = unsafe {
                libc::posix_spawn(
                    &mut child_pid,
                    c_path.as_ptr(),
                    &file_actions,
                    &attr,
                    c_ptrs.as_ptr(),
                    env_ptrs.as_ptr(),
                )
            };
            unsafe {
                libc::posix_spawn_file_actions_destroy(&mut file_actions);
                libc::posix_spawnattr_destroy(&mut attr);
            }
            if ret != 0 {
                self.fork_count.fetch_sub(1, Ordering::SeqCst);
                eprintln!("context: posix_spawn failed: {}", std::io::Error::from_raw_os_error(ret));
                return 126;
            }

            self.kill_after_timeout(child_pid, self.cfg.execution.timeout_seconds);

            signals::CHILD_PID.store(child_pid, std::sync::atomic::Ordering::SeqCst);
            signals::RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);

            let exit = self.wait_for_pid(child_pid);
            self.fork_count.fetch_sub(1, Ordering::SeqCst);
            if (147..=150).contains(&exit) {
                let job = Job {
                    id: self.next_job_id,
                    pid: child_pid,
                    cmd: words.join(" "),
                    running: false,
                };
                self.next_job_id += 1;
                if self.cfg.jobs.warn_on_suspended {
                    println!("[{}] {}", job.id, job.cmd);
                }
                self.jobs.push(job);
            } else if self.cfg.jobs.notify_on_job_done {
                eprintln!("{}", self.format_done_msg(&words.join(" "), exit));
            }
            signals::RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
            signals::CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
            signals::set_foreground(0);
            self.last_status = exit;
            signals::set_last_status(exit);
            return exit;
        }

        self.fork_count.fetch_add(1, Ordering::SeqCst);
        match unsafe { libc::fork() } {
            -1 => {
                self.fork_count.fetch_sub(1, Ordering::SeqCst);
                eprintln!("context: fork failed"); 1
            }
            0 => {
                unsafe {
                    libc::setpgid(0, 0);
                }
                self.child_exec(words, redirects, &path);
            }
            pid => {
                unsafe {
                    libc::setpgid(pid, pid);
                    libc::tcsetpgrp(libc::STDIN_FILENO, pid);
                }
                signals::CHILD_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
                signals::RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
                self.kill_after_timeout(pid, self.cfg.execution.timeout_seconds);

                let exit = self.wait_for_pid(pid);
                self.fork_count.fetch_sub(1, Ordering::SeqCst);
                if (147..=150).contains(&exit) {
                    let job = Job {
                        id: self.next_job_id,
                        pid,
                        cmd: words.join(" "),
                        running: false,
                    };
                    self.next_job_id += 1;
                    if self.cfg.jobs.warn_on_suspended {
                        println!("[{}] {}", job.id, job.cmd);
                    }
                    self.jobs.push(job);
                } else if self.cfg.jobs.notify_on_job_done {
                    eprintln!("{}", self.format_done_msg(&words.join(" "), exit));
                }
                unsafe {
                    libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                }
                signals::RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                signals::CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
                signals::set_foreground(0);
                self.last_status = exit;
                signals::set_last_status(exit);
                exit
            }
        }
    }

    fn exec_background(&mut self, words: &[String], redirects: &[Redirect]) {
        let cmd = &words[0];
        let path = if cmd.contains('/') {
            cmd.clone()
        } else if let Some(p) = self.find_in_path(cmd) {
            p
        } else {
            eprintln!("context: {}: command not found", cmd);
            return;
        };

        let max_forks = self.cfg.execution.max_forks_per_command as usize;
        if max_forks > 0 && self.fork_count.load(Ordering::SeqCst) >= max_forks {
            eprintln!("context: fork limit reached");
            return;
        }

        if self.cfg.jobs.max_jobs > 0 && self.jobs.len() >= self.cfg.jobs.max_jobs as usize {
            eprintln!("context: job limit reached (max {})", self.cfg.jobs.max_jobs);
            return;
        }

        if self.cfg.security.no_exec_commands.contains(&words[0]) {
            eprintln!("context: {}: command blocked by security policy", words[0]);
            return;
        }

        match unsafe { libc::fork() } {
            -1 => { eprintln!("context: fork failed"); }
            0 => {
                self.fork_count.fetch_add(1, Ordering::SeqCst);
                unsafe {
                    libc::setsid();
                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                    let devnull = libc::open(c"/dev/null".as_ptr() as *const _, libc::O_RDONLY);
                    libc::dup2(devnull, libc::STDIN_FILENO);
                    libc::close(devnull);
                }
                self.child_exec(words, redirects, &path);
            }
            pid => {
                self.fork_count.fetch_add(1, Ordering::SeqCst);
                self.kill_after_timeout(pid, self.cfg.execution.timeout_seconds);
                let fork_count_clone = Arc::clone(&self.fork_count);
                std::thread::spawn(move || {
                    let mut status: i32 = 0;
                    unsafe { libc::waitpid(pid, &mut status, 0); }
                    fork_count_clone.fetch_sub(1, Ordering::SeqCst);
                });
                signals::BACKGROUND_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
                let job = Job {
                    id: self.next_job_id,
                    pid,
                    cmd: words.join(" "),
                    running: true,
                };
                self.next_job_id += 1;
                println!("[{}] {} {}", job.id, job.pid, job.cmd);
                self.jobs.push(job);
            }
        }
    }

    fn child_exec(&self, words: &[String], redirects: &[Redirect], path: &str) -> ! {
        signals::setup_child_handlers();
        for r in redirects {
            self.apply_redirect(r);
        }
        if self.cfg.execution.strip_env_on_exec {
            unsafe { libc::clearenv(); }
        }
        if self.cfg.execution.bash_compat {
            if let Ok(ck) = CString::new("BASH_COMPAT") {
                if let Ok(cv) = CString::new("5.2") {
                    unsafe { libc::setenv(ck.as_ptr(), cv.as_ptr(), 1); }
                }
            }
        }
        let passthrough: Vec<String> = self.cfg.environment.passthrough.to_vec();
        let filter: Vec<String> = self.cfg.environment.filter.to_vec();
        for (k, v) in self.env.passthrough_env(&passthrough, &filter) {
            if let (Ok(ck), Ok(cv)) = (CString::new(k.as_str()), CString::new(v.as_str())) {
                unsafe { libc::setenv(ck.as_ptr(), cv.as_ptr(), 1); }
            }
        }
        for key in &self.cfg.environment.strip_on_exit {
            if let Ok(ck) = CString::new(key.as_str()) {
                unsafe { libc::unsetenv(ck.as_ptr()); }
            }
        }
        let c_args: Vec<CString> = words.iter()
            .filter_map(|w| CString::new(w.as_str()).ok())
            .collect();
        let mut c_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
        c_ptrs.push(std::ptr::null());
        let c_cmd = CString::new(path).unwrap_or_else(|_| CString::new("sh").unwrap());
        unsafe { libc::execvp(c_cmd.as_ptr(), c_ptrs.as_ptr()); }
        std::process::exit(126);
    }

    fn kill_after_timeout(&self, pid: i32, timeout_secs: u32) {
        if timeout_secs == 0 { return; }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(timeout_secs as u64));
            unsafe {
                let mut status: i32 = 0;
                let ret = libc::waitpid(pid, &mut status, libc::WNOHANG);
                if ret == 0 {
                    eprintln!("context: command timed out after {}s, sending SIGKILL", timeout_secs);
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        });
    }

    fn wait_for_pid(&self, pid: libc::pid_t) -> i32 {
        if self.cfg.editor.hide_cursor_on_exec {
            print!("\x1b[?25l");
            let _ = std::io::stdout().flush();
        }
        let mut status: i32 = 0;
        loop {
            let ret = unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) };
            if ret != -1 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                break;
            }
        }
        if self.cfg.editor.hide_cursor_on_exec {
            print!("\x1b[?25h");
            let _ = std::io::stdout().flush();
        }
        if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else if libc::WIFSIGNALED(status) {
            128 + libc::WTERMSIG(status)
        } else if libc::WIFSTOPPED(status) {
            128 + libc::WSTOPSIG(status)
        } else {
            1
        }
    }

    fn cleanup_jobs(&mut self) {
        self.jobs.retain(|job| {
            if !job.running { return true; }
            let mut status: i32 = 0;
            let ret = unsafe { libc::waitpid(job.pid, &mut status, libc::WNOHANG) };
            if ret > 0
                && (libc::WIFEXITED(status) || libc::WIFSIGNALED(status)) {
                    return false;
                }
            true
        });
    }

    fn apply_redirect(&self, redirect: &Redirect) {
        match redirect.kind {
            RedirKind::Output => {
                if self.opt_n() && Path::new(&redirect.target).exists() {
                    eprintln!("context: {}: cannot overwrite existing file", redirect.target);
                    return;
                }
                if let Ok(file) = File::create(&redirect.target) {
                    unsafe { libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO); }
                }
            }
            RedirKind::OutputAppend => {
                if let Ok(file) = OpenOptions::new().create(true).append(true).open(&redirect.target) {
                    unsafe { libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO); }
                }
            }
            RedirKind::Input => {
                if let Ok(file) = File::open(&redirect.target) {
                    unsafe { libc::dup2(file.as_raw_fd(), libc::STDIN_FILENO); }
                }
            }
            RedirKind::OutputFd => {
                if let Ok(file) = File::create(&redirect.target) {
                    unsafe {
                        libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
                        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
                    }
                }
            }
            RedirKind::OutputFdAppend => {
                if let Ok(file) = OpenOptions::new().create(true).append(true).open(&redirect.target) {
                    unsafe {
                        libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
                        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
                    }
                }
            }
            RedirKind::RedirectFd => {
                if let Ok(fd) = redirect.target.parse::<i32>() {
                    unsafe { libc::dup2(fd, libc::STDOUT_FILENO); }
                }
            }
            RedirKind::HereDoc(ref word) => {
                let mut fds = [0i32; 2];
                unsafe { libc::pipe(fds.as_mut_ptr()); }
                let (r, w) = (fds[0], fds[1]);
                unsafe {
                    libc::dup2(r, libc::STDIN_FILENO);
                    libc::close(r);
                }
                let data = format!("{}\n", word);
                std::thread::spawn(move || unsafe {
                    libc::write(w, data.as_ptr() as *const libc::c_void, data.len());
                    libc::close(w);
                });
            }
            RedirKind::HereDocBody(ref body) => {
                let mut fds = [0i32; 2];
                unsafe { libc::pipe(fds.as_mut_ptr()); }
                let (r, w) = (fds[0], fds[1]);
                unsafe {
                    libc::dup2(r, libc::STDIN_FILENO);
                    libc::close(r);
                }
                let data = body.clone();
                std::thread::spawn(move || unsafe {
                    libc::write(w, data.as_ptr() as *const libc::c_void, data.len());
                    libc::close(w);
                });
            }
            RedirKind::HereString(ref word) => {
                let mut fds = [0i32; 2];
                unsafe { libc::pipe(fds.as_mut_ptr()); }
                let (r, w) = (fds[0], fds[1]);
                unsafe {
                    libc::dup2(r, libc::STDIN_FILENO);
                    libc::close(r);
                }
                let data = format!("{}\n", word);
                std::thread::spawn(move || unsafe {
                    libc::write(w, data.as_ptr() as *const libc::c_void, data.len());
                    libc::close(w);
                });
            }
            RedirKind::Clobber => {
                if let Ok(file) = File::create(&redirect.target) {
                    unsafe { libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO); }
                }
            }
            RedirKind::InputFd => {
                if let Ok(fd) = redirect.target.parse::<i32>() {
                    unsafe { libc::dup2(fd, libc::STDIN_FILENO); }
                }
            }
        }
    }

    fn expand_aliases(&self, words: &[String]) -> Vec<String> {
        if words.is_empty() { return words.to_vec(); }
        let mut result = words.to_vec();
        let mut seen = 0;
        while seen < MAX_ALIAS_EXPAND {
            if let Some(alias_val) = self.env.get_alias(&result[0]) {
                let alias_words: Vec<String> = alias_val.split_whitespace().map(String::from).collect();
                if alias_words.is_empty() {
                    result.remove(0);
                    if result.is_empty() { return result; }
                    continue;
                }
                let rest: Vec<String> = result[1..].to_vec();
                result = alias_words;
                result.extend(rest);
                seen += 1;
            } else {
                break;
            }
        }
        result
    }

    fn word_split(&self, words: &[String]) -> Vec<String> {
        let ifs = self.env.get("IFS").unwrap_or(" \t\n").to_string();
        let mut result = Vec::new();
        for word in words {

            if word.starts_with('\x01') && word.ends_with('\x01') && word.len() >= 2 {
                result.push(word[1..word.len()-1].to_string());
                continue;
            }
            if word.is_empty() {
                result.push(String::new());
                continue;
            }
            let mut split = Vec::new();
            let mut current = String::new();
            for ch in word.chars() {
                if ifs.contains(ch) {
                    if !current.is_empty() {
                        split.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(ch);
                }
            }
            if !current.is_empty() {
                split.push(current);
            }
            if split.is_empty() {
                result.push(String::new());
            } else {
                result.extend(split);
            }
        }
        result
    }

    fn expand_globs(&self, words: &[String]) -> Vec<String> {
        if self.opt_g() {
            return words.to_vec();
        }
        let mut result = Vec::new();
        for word in words {
            let expanded = expand_braces(word);
            for w in expanded {
                if has_glob_chars(&w) {
                    match glob::glob(&w) {
                        Ok(paths) => {
                            let mut matches: Vec<String> = paths
                                .filter_map(|e| e.ok())
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();
                            if matches.is_empty() {
                                result.push(w);
                            } else {
                                matches.sort();
                                result.extend(matches);
                            }
                        }
                        Err(_) => { result.push(w); }
                    }
                } else {
                    result.push(w);
                }
            }
        }
        result
    }

    fn source_file(&mut self, path: &str) -> i32 {
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("context: source: {}: {}", path, e);
                return 1;
            }
        };
        let tokens = crate::shell::lexer::tokenize(&contents);
        let ast = crate::shell::parser::parse(tokens);
        self.execute(&ast)
    }

    fn find_in_path(&self, cmd: &str) -> Option<String> {
        let path_env = if !self.cfg.execution.path_override.is_empty() {
            self.cfg.execution.path_override.clone()
        } else {
            self.env.get("PATH").unwrap_or("/system/local/bin:/system/bin:/bin").to_string()
        };
        for dir in path_env.split(':') {
            if dir.is_empty() { continue; }
            let full = Path::new(dir).join(cmd);
            if full.is_file() {
                return Some(full.to_string_lossy().to_string());
            }
        }
        None
    }

    fn cmd_enable(&mut self, args: &[String]) -> i32 {
        if args.is_empty() {
            let all = [
                "cd", "exit", "export", "unset", "alias", "unalias",
                "source", ".", "history", "set", "unsetenv", "env",
                "pwd", "type", "which", "echo", "printf", "true",
                "false", "test", "[", "let", "exec", "trap",
                "pushd", "popd", "dirs", "hash", "math", "regexmatch", "module",
                "readonly", "builtin", "shopt",
                "declare", "typeset", "local",
                "caller", "jobs", "fg", "bg",
                "wait", "kill", "umask", "command", "eval",
                "select", "getopts", "realpath", "complete", "compgen", "read",
                "bindkey"
            ];
            for name in &all {
                let status = if self.disabled_builtins.contains(*name) { "off" } else { "on" };
                println!("{}={}", name, status);
            }
            return 0;
        }
        let mut i = 0;
        let mut disable = false;
        while i < args.len() {
            if args[i] == "-n" {
                disable = true;
                i += 1;
            } else if args[i].starts_with('-') && args[i] != "-n" {
                eprintln!("context: enable: unknown option: {}", args[i]);
                return 1;
            } else {
                break;
            }
        }
        if i >= args.len() {
            eprintln!("context: enable: requires a builtin name");
            return 1;
        }
        let name = &args[i];
        if !builtin_is(name) {
            eprintln!("context: enable: {}: not a builtin", name);
            return 1;
        }
        if disable {
            self.disabled_builtins.insert(name.to_string());
        } else {
            self.disabled_builtins.remove(name);
        }
        0
    }

    fn audit_log(&self, cmd: &str) {
        let path_str = if self.cfg.security.audit_log_path.starts_with('~') {
            if let Ok(home) = std::env::var("HOME") {
                format!("{}{}", home, &self.cfg.security.audit_log_path[1..])
            } else {
                self.cfg.security.audit_log_path.clone()
            }
        } else {
            self.cfg.security.audit_log_path.clone()
        };
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path_str) {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(f, "[{}] {}", ts, cmd);
        }
    }

    fn opt_is(&self, var: &str) -> bool {
        self.env.get(var).map(|s| s == "1").unwrap_or(false)
    }

    fn opt_e(&self) -> bool { self.opt_is("_OPT_E") }
    #[allow(dead_code)]
    fn opt_u(&self) -> bool { self.opt_is("_OPT_U") }
    fn opt_x(&self) -> bool { self.opt_is("_OPT_X") }
    fn opt_n(&self) -> bool { self.opt_is("_OPT_N") }
    fn opt_g(&self) -> bool { self.opt_is("_OPT_G") }
    #[allow(dead_code)]
    fn opt_f(&self) -> bool { self.opt_is("_OPT_F") }
    fn opt_pipefail(&self) -> bool { self.opt_is("_OPT_PIPEFAIL") }
    #[allow(dead_code)]
    fn opt_notify(&self) -> bool { self.opt_is("_OPT_NOTIFY") }
    #[allow(dead_code)]
    fn opt_trace_format(&self) -> bool { self.opt_is("_OPT_TRACEFORMAT") }

    fn trace_print(&self, words: &[String]) {
        if self.opt_x() {
            let ps4 = self.env.get("PS4").unwrap_or("+ ");
            let cmd_str: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
            eprintln!("{}{}", ps4, cmd_str.join(" "));
        }
    }
}

fn builtin_is(cmd: &str) -> bool {
    matches!(cmd,
        "cd" | "exit" | "export" | "unset" | "alias" | "unalias" |
        "source" | "." | "history" | "set" | "unsetenv" | "env" |
        "pwd" | "type" | "which" | "echo" | "printf" | "true" |
        "false" | "test" | "[" | "let" | "exec" | "trap" |
        "pushd" | "popd" | "dirs" | "hash" | "math" | "regexmatch" | "module" |
        "readonly" | "builtin" | "shopt" | "enable" |
        "declare" | "typeset" | "local" |
        "caller" | "jobs" | "fg" | "bg" |
        "wait" | "kill" | "umask" | "command" | "eval" |
        "select" | "getopts" | "realpath" | "complete" | "compgen" | "read" |
        "bindkey"
    )
}

fn expand_braces(input: &str) -> Vec<String> {
    if let Some(start) = input.find('{') {
        let before = &input[..start];
        let rest = &input[start + 1..];
        let mut depth = 1;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end_idx) = end {
            let inner = &rest[..end_idx];
            let after = &rest[end_idx + 1..];
            let alternatives = split_brace_alternatives(inner);
            let mut result = Vec::new();
            for alt in alternatives {
                let expanded = format!("{}{}{}", before, alt, after);
                let sub = expand_braces(&expanded);
                result.extend(sub);
            }
            return result;
        }
    }
    vec![input.to_string()]
}

fn split_brace_alternatives(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => { depth += 1; current.push(c); }
            '}' => { depth -= 1; current.push(c); }
            ',' if depth == 0 => {
                result.push(current.clone());
                current.clear();
            }
            _ => { current.push(c); }
        }
    }
    result.push(current);
    result
}

fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() { return text.is_empty(); }
    if pattern[0] == '*' {
        for i in 0..=text.len() {
            if glob_match_inner(&pattern[1..], &text[i..]) { return true; }
        }
        return false;
    }
    if text.is_empty() { return false; }
    if pattern[0] == '?' || pattern[0] == text[0] {
        return glob_match_inner(&pattern[1..], &text[1..]);
    }
    if pattern[0] == '[' {
        if let Some(close) = pattern[1..].iter().position(|&c| c == ']') {
            let class = &pattern[2..close + 1];
            let negate = !class.is_empty() && (class[0] == '^' || class[0] == '!');
            let class_chars = if negate { &class[1..] } else { class };
            let mut matches = false;
            let mut i = 0;
            while i < class_chars.len() {
                if i + 2 < class_chars.len() && class_chars[i + 1] == '-' {
                    let lo = class_chars[i];
                    let hi = class_chars[i + 2];
                    if text[0] >= lo && text[0] <= hi {
                        matches = true;
                    }
                    i += 3;
                } else {
                    if class_chars[i] == text[0] {
                        matches = true;
                    }
                    i += 1;
                }
            }
            return if negate { !matches } else { matches }
                && glob_match_inner(&pattern[close + 2..], &text[1..]);
        }
    }
    false
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut d = vec![vec![0usize; b_len + 1]; a_len + 1];
    for (i, row) in d.iter_mut().enumerate().take(a_len + 1) { row[0] = i; }
    for (j, cell) in d[0].iter_mut().enumerate().take(b_len + 1) { *cell = j; }
    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a.as_bytes()[i - 1] == b.as_bytes()[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }
    d[a_len][b_len]
}

fn spell_correct(cmd: &str) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !name.contains('.') {
                            candidates.push(name);
                        }
                    }
                }
            }
        }
    }
    let threshold = if cmd.len() <= 4 { 1 } else { 2 };
    let mut best: Option<(String, usize)> = None;
    for c in &candidates {
        let dist = levenshtein(cmd, c);
        if dist <= threshold {
            match &best {
                Some((_, best_dist)) if dist < *best_dist => best = Some((c.clone(), dist)),
                None => best = Some((c.clone(), dist)),
                _ => {}
            }
        }
    }
    best.map(|(name, _)| name)
}

fn eval_test_bracket(tokens: &[String]) -> i32 {
    if tokens.is_empty() {
        return 2;
    }
    let result = eval_test_or(tokens);
    if result { 0 } else { 1 }
}

fn eval_test_or(tokens: &[String]) -> bool {
    let mut left = eval_test_and(tokens);
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "||" {
            i += 1;
            let right = eval_test_and(&tokens[i..]);
            left = left || right;
            while i < tokens.len() && tokens[i] != "||" && tokens[i] != "&&" { i += 1; }
        } else if tokens[i] == "&&" {
            i += 1;
            let right = eval_test_and(&tokens[i..]);
            left = left && right;
            while i < tokens.len() && tokens[i] != "||" && tokens[i] != "&&" { i += 1; }
        } else {
            i += 1;
        }
    }
    left
}

fn eval_test_and(tokens: &[String]) -> bool {
    let mut left = eval_test_primary(tokens);
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "&&" {
            i += 1;
            let right = eval_test_primary(&tokens[i..]);
            left = left && right;
            while i < tokens.len() && tokens[i] != "||" && tokens[i] != "&&" { i += 1; }
        } else {
            i += 1;
        }
    }
    left
}

fn eval_test_primary(tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    if tokens[0] == "!" {
        return !eval_test_primary(&tokens[1..]);
    }
    if tokens[0] == "(" {
        if let Some(close) = find_matching_paren(tokens) {
            let inner = &tokens[1..close];
            let result = eval_test_or(inner);
            return result;
        }
    }
    if tokens.len() >= 2 {
        let op = &tokens[0];
        if op == "-f" { return std::path::Path::new(&tokens[1]).is_file(); }
        if op == "-d" { return std::path::Path::new(&tokens[1]).is_dir(); }
        if op == "-e" { return std::path::Path::new(&tokens[1]).exists(); }
        if op == "-r" {
            let c = std::ffi::CString::new(tokens[1].as_str()).unwrap_or_default();
            return unsafe { libc::access(c.as_ptr(), libc::R_OK) == 0 };
        }
        if op == "-w" {
            let c = std::ffi::CString::new(tokens[1].as_str()).unwrap_or_default();
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                let euid = unsafe { libc::geteuid() };
                let egid = unsafe { libc::getegid() };
                return if euid == 0 { true }
                else if st.st_uid == euid { (st.st_mode & libc::S_IWUSR) != 0 }
                else if st.st_gid == egid { (st.st_mode & libc::S_IWGRP) != 0 }
                else { (st.st_mode & libc::S_IWOTH) != 0 };
            }
            return false;
        }
        if op == "-x" {
            let c = std::ffi::CString::new(tokens[1].as_str()).unwrap_or_default();
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                let euid = unsafe { libc::geteuid() };
                let egid = unsafe { libc::getegid() };
                return if euid == 0 { true }
                else if st.st_uid == euid { (st.st_mode & libc::S_IXUSR) != 0 }
                else if st.st_gid == egid { (st.st_mode & libc::S_IXGRP) != 0 }
                else { (st.st_mode & libc::S_IXOTH) != 0 };
            }
            return false;
        }
        if op == "-s" { return std::fs::metadata(&tokens[1]).map(|m| m.len() > 0).unwrap_or(false); }
        if op == "-L" || op == "-h" { return std::path::Path::new(&tokens[1]).is_symlink(); }
        if op == "-S" { return std::fs::metadata(&tokens[1]).map(|m| m.file_type().is_socket()).unwrap_or(false); }
        if op == "-p" { return std::fs::metadata(&tokens[1]).map(|m| m.file_type().is_fifo()).unwrap_or(false); }
        if op == "-c" { return std::fs::metadata(&tokens[1]).map(|m| m.file_type().is_char_device()).unwrap_or(false); }
        if op == "-n" { return !tokens[1].is_empty(); }
        if op == "-z" { return tokens[1].is_empty(); }
        if op == "=" || op == "==" { return glob_match(&tokens[2], &tokens[1]); }
        if op == "!=" { return !glob_match(&tokens[2], &tokens[1]); }
        if op == "-eq" { return tokens[1].parse::<i64>().unwrap_or(0) == tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "-ne" { return tokens[1].parse::<i64>().unwrap_or(0) != tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "-lt" { return tokens[1].parse::<i64>().unwrap_or(0) < tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "-le" { return tokens[1].parse::<i64>().unwrap_or(0) <= tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "-gt" { return tokens[1].parse::<i64>().unwrap_or(0) > tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "-ge" { return tokens[1].parse::<i64>().unwrap_or(0) >= tokens[2].parse::<i64>().unwrap_or(0); }
        if op == "=~" {
            let pattern = &tokens[2];
            if let Ok(re) = regex::Regex::new(pattern) {
                return re.is_match(&tokens[1]);
            }
            return false;
        }
    }
    if tokens.len() >= 3 {
        let a = &tokens[0];
        let op = &tokens[1];
        let b = &tokens[2];
        if op == "==" || op == "=" { return glob_match(b, a); }
        if op == "!=" { return !glob_match(b, a); }
        if op == "=~" {
            if let Ok(re) = regex::Regex::new(b) {
                return re.is_match(a);
            }
            return false;
        }
        if op == "-eq" { return a.parse::<i64>().unwrap_or(0) == b.parse::<i64>().unwrap_or(0); }
        if op == "-ne" { return a.parse::<i64>().unwrap_or(0) != b.parse::<i64>().unwrap_or(0); }
        if op == "-lt" { return a.parse::<i64>().unwrap_or(0) < b.parse::<i64>().unwrap_or(0); }
        if op == "-le" { return a.parse::<i64>().unwrap_or(0) <= b.parse::<i64>().unwrap_or(0); }
        if op == "-gt" { return a.parse::<i64>().unwrap_or(0) > b.parse::<i64>().unwrap_or(0); }
        if op == "-ge" { return a.parse::<i64>().unwrap_or(0) >= b.parse::<i64>().unwrap_or(0); }
    }
    if tokens.len() == 1 {
        return !tokens[0].is_empty();
    }
    false
}

fn find_matching_paren(tokens: &[String]) -> Option<usize> {
    let mut depth = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t == "(" { depth += 1; }
        if t == ")" { depth -= 1; if depth == 0 { return Some(i); } }
    }
    None
}
