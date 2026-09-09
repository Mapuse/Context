use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

use crate::config::Config;
use crate::shell::ast::*;
use crate::shell::builtin;
use crate::shell::env::Env;
use crate::shell::expand::Expander;
use crate::shell::signals;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

const MAX_ALIAS_EXPAND: usize = 10;

pub static BASH_REMATCH: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
pub static SOURCE_STACK: LazyLock<Mutex<Vec<String>>> =
    LazyLock::new(|| Mutex::new(vec![String::new()]));
pub type CoprocEntry = (Option<String>, RawFd, RawFd);
pub static COPROC_FDS: LazyLock<Mutex<Vec<CoprocEntry>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub pid: i32,
    pub cmd: String,
    pub running: bool,
}

type ExpandedCaseArm = (Vec<String>, Box<Node>, CaseTerminator);

pub struct Executor {
    pub env: Env,
    pub cfg: Config,
    pub last_status: i32,
    pub functions: Arc<std::sync::Mutex<std::collections::HashMap<String, Node>>>,
    pub jobs: Vec<Job>,
    pub clear_history: bool,
    prev_cwd: String,
    next_job_id: usize,
    fork_count: Arc<AtomicUsize>,
    loop_control: Option<LoopControl>,
    loop_depth: usize,
    return_value: Option<i32>,
    function_depth: usize,
    pipe_statuses: Vec<i32>,
    in_trap: bool,
    errexit_suppress: usize,
    in_background_job: bool,
    /// Set by the interactive session driver; a failed `exec` then returns
    /// an error status instead of replacing (or killing) the shell process.
    pub interactive: bool,
}

#[derive(Clone, Copy, Debug)]
enum LoopControl {
    Break(u32),
    Continue(u32),
}

/// Return value of a loop body evaluation with respect to `break`/`continue`.
enum LoopAction {
    None,
    Stop,
    Continue,
}

impl Executor {
    pub fn new(env: Env, cfg: Config) -> Self {
        builtin::set_mask_secrets(cfg.security.mask_secrets);

        if let Ok(mask) = u32::from_str_radix(cfg.execution.umask.trim_start_matches('0'), 8) {
            unsafe {
                libc::umask(mask as libc::mode_t);
            }
        }

        if cfg.security.sanitize_path
            && let Ok(path) = std::env::var("PATH")
        {
            let sanitized: String = path
                .split(':')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(":");
            unsafe {
                std::env::set_var("PATH", &sanitized);
            }
        }

        let mut env = env;
        let default_keys: Vec<String> = cfg
            .environment
            .set_defaults
            .iter()
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
        let functions_arc = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let executor = Self {
            env,
            cfg,
            last_status: 0,
            functions: functions_arc.clone(),
            jobs: Vec::new(),
            clear_history: false,
            prev_cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            next_job_id: 1,
            fork_count: Arc::new(AtomicUsize::new(0)),
            loop_control: None,
            loop_depth: 0,
            return_value: None,
            function_depth: 0,
            pipe_statuses: Vec::new(),
            in_trap: false,
            errexit_suppress: 0,
            in_background_job: false,
            interactive: false,
        };

        let _ = builtin::GET_FUNCTION_CB.set(std::sync::Mutex::new(Some(Box::new(
            move |name: &str| -> Option<String> {
                let map = functions_arc.lock().unwrap();
                map.get(name).map(|node| {
                    let src = node_to_source(node, 0);
                    // The stored body is a brace group; print it as `name () { … }`.
                    if src.starts_with('{') {
                        format!("{} () {}\n", name, src.trim_end())
                    } else {
                        format!("{} () {{\n{}}}\n", name, src)
                    }
                })
            },
        ))));

        executor
    }

    pub fn run_source_rc(&mut self, override_rc: Option<&str>) {
        let rc = if let Some(path) = override_rc {
            std::path::PathBuf::from(path)
        } else {
            crate::config::loader::rc_path()
        };
        if rc.is_file()
            && let Ok(contents) = std::fs::read_to_string(&rc)
        {
            let tokens = crate::shell::lexer::tokenize(&contents);
            let ast = crate::shell::parser::parse(tokens);
            self.execute(&ast);
        }
    }

    pub fn run_integrations(&mut self) {
        let cfg = self.cfg.clone();

        if cfg.integration.enable_fzf && cfg.integration.fzf_key_bindings {
            let candidates = [
                dirs::home_dir().map(|h| h.join(".fzf/shell/key-bindings.bash")),
                Some(std::path::PathBuf::from(
                    "/system/share/fzf/key-bindings.bash",
                )),
                Some(std::path::PathBuf::from(
                    "/system/share/doc/fzf/examples/key-bindings.bash",
                )),
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

        if cfg.integration.enable_zoxide
            && cfg.integration.zoxide_init
            && let Ok(output) = std::process::Command::new("zoxide")
                .args(["init", "context"])
                .output()
            && output.status.success()
        {
            let init_script = String::from_utf8_lossy(&output.stdout).to_string();
            if !init_script.is_empty() {
                let tokens = crate::shell::lexer::tokenize(&init_script);
                let ast = crate::shell::parser::parse(tokens);
                self.execute(&ast);
            }
        }

        if cfg.integration.starship_prompt {
            if self.find_in_path("starship").is_some() {
                if let Ok(output) = std::process::Command::new("starship")
                    .args(["init", "bash"])
                    .output()
                    && output.status.success()
                {
                    let init_script = String::from_utf8_lossy(&output.stdout).to_string();
                    let tokens = crate::shell::lexer::tokenize(&init_script);
                    let ast = crate::shell::parser::parse(tokens);
                    self.execute(&ast);
                }
            } else {
                eprintln!("context: starship_prompt enabled but starship not found in PATH");
            }
        }
    }

    /// Run a trap body while suppressing further trap hooks (DEBUG/ERR) so
    /// that a trap whose own commands would fire the same trap cannot recurse.
    fn run_trap_body(&mut self, cmd: &str) -> i32 {
        let tokens = crate::shell::lexer::tokenize(cmd);
        let ast = crate::shell::parser::parse(tokens);
        let saved = self.in_trap;
        self.in_trap = true;
        let status = self.execute(&ast);
        self.in_trap = saved;
        status
    }

    pub fn execute(&mut self, node: &Node) -> i32 {
        // Once a failure requested termination (errexit / exit builtin),
        // skip the remaining commands of the current construct.
        if signals::SHOULD_EXIT.load(std::sync::atomic::Ordering::SeqCst)
            && !matches!(node, Node::Empty)
        {
            return self.last_status;
        }
        let status = self.execute_node(node);
        let sig = signals::TRAP_SIGNAL.swap(0, std::sync::atomic::Ordering::SeqCst);
        if sig != 0 && !self.in_trap {
            let signame = match sig {
                libc::SIGINT => "INT",
                libc::SIGQUIT => "QUIT",
                libc::SIGTSTP => "TSTP",
                libc::SIGHUP => "HUP",
                libc::SIGTERM => "TERM",
                _ => "UNKNOWN",
            };
            if let Some(cmd) = self.env.get_trap(signame).map(|s| s.to_string())
                && !cmd.is_empty()
            {
                let trap_status = self.run_trap_body(&cmd);
                self.last_status = trap_status;
                signals::set_last_status(trap_status);
                return trap_status;
            }
        }
        let current_cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if current_cwd != self.prev_cwd {
            self.prev_cwd = current_cwd;
        }
        if status != 0 {
            // errexit and the ERR trap apply to real command failures only:
            // not in condition contexts (if/while/until conditions, `!`
            // pipelines, non-final && / || operands), and not for aggregate
            // nodes that merely propagate a child's status.
            let ignorable = self.errexit_suppress > 0
                || matches!(
                    node,
                    Node::Empty
                        | Node::Compound { .. }
                        | Node::If { .. }
                        | Node::While { .. }
                        | Node::Until { .. }
                        | Node::Pipeline { bang: true, .. }
                        | Node::Arithmetic { .. }
                );
            if !ignorable
                && !self.in_trap
                && let Some(cmd) = self.env.get_trap("ERR").map(|s| s.to_string())
                && !cmd.is_empty()
            {
                self.run_trap_body(&cmd);
            }
            if self.opt_e()
                && !ignorable
                && !signals::SHOULD_EXIT.load(std::sync::atomic::Ordering::SeqCst)
            {
                eprintln!("context: terminating on errexit (status {})", status);
                signals::EXIT_CODE.store(status, std::sync::atomic::Ordering::SeqCst);
                signals::SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        status
    }

    /// Execute `node` in a condition context: failures inside it do not
    /// trigger errexit or the ERR trap.
    fn execute_condition(&mut self, node: &Node) -> i32 {
        self.errexit_suppress += 1;
        let status = self.execute(node);
        self.errexit_suppress -= 1;
        status
    }

    fn loop_action(&mut self) -> LoopAction {
        match self.loop_control.take() {
            Some(LoopControl::Break(n)) => {
                if n > 1 {
                    self.loop_control = Some(LoopControl::Break(n - 1));
                }
                LoopAction::Stop
            }
            Some(LoopControl::Continue(n)) => {
                if n > 1 {
                    self.loop_control = Some(LoopControl::Continue(n - 1));
                    LoopAction::Stop
                } else {
                    LoopAction::Continue
                }
            }
            None => LoopAction::None,
        }
    }

    fn run_simple_command(
        &mut self,
        words: &[String],
        redirects: &[Redirect],
        background: bool,
    ) -> i32 {
        if words.is_empty() {
            return 0;
        }

        if self.cfg.security.restricted_mode {
            let restricted = [
                "exec", "eval", "source", ".", "kill", "env", "export", "bash", "sh", "zsh",
                "fish", "command", "builtin", "enable",
            ];
            if restricted.contains(&words[0].as_str()) {
                eprintln!("context: restricted mode: {} not allowed", words[0]);
                return 1;
            }
            if words[0] == "cd"
                && words.len() > 1
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

        if let Some(func_body) = { self.functions.lock().unwrap().get(&words[0]).cloned() } {
            let saved = self.env.push_scope();
            let saved_positional = self.env.positional().to_vec();
            let nounset = self.opt_u();
            let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
            let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
            expander.set_nounset(nounset);
            let expanded_words: Vec<String> = words[1..]
                .iter()
                .flat_map(|w| expander.expand_words(w))
                .map(|w| strip_markers(&w))
                .collect();
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
            self.function_depth += 1;
            builtin::FUNCTION_DEPTH
                .store(self.function_depth, std::sync::atomic::Ordering::Relaxed);
            builtin::CALL_STACK
                .lock()
                .unwrap()
                .push(builtin::CallerFrame {
                    name: words[0].clone(),
                    line: 0,
                });
            let status = if nounset_err {
                1
            } else {
                self.execute(&func_body)
            };
            builtin::CALL_STACK.lock().unwrap().pop();
            self.function_depth -= 1;
            builtin::FUNCTION_DEPTH
                .store(self.function_depth, std::sync::atomic::Ordering::Relaxed);
            let status = self.return_value.take().unwrap_or(status);
            if let Some(cmd) = self.env.get_trap("RETURN").map(|s| s.to_string())
                && !cmd.is_empty()
            {
                self.run_trap_body(&cmd);
            }
            self.env.pop_scope(saved);
            self.env.set_positional(saved_positional);
            self.last_status = status;
            signals::set_last_status(status);
            return status;
        }
        let words = if self.cfg.editor.expand_aliases {
            self.expand_aliases(words)
        } else {
            words.to_vec()
        };
        let (words, ps_pids) = setup_process_sub(&words);
        for pid in &ps_pids {
            unsafe {
                libc::setpgid(*pid, *pid);
            }
        }
        let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
        let nounset = self.opt_u();
        let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
        expander.set_nounset(nounset);
        let words: Vec<String> = words
            .iter()
            .flat_map(|w| expander.expand_words(w))
            .collect();
        let pending = expander.take_pending_sets();
        let nounset_err = expander.had_nounset_error();
        drop(expander);
        for (k, v) in pending {
            self.env.set(&k, &v);
        }
        if nounset_err {
            return 1;
        }
        let words = self.word_split(&words);
        let (words, glob_status) = self.expand_globs(&words);
        if words.is_empty() {
            return glob_status;
        }
        self.trace_print(&words);

        if !self.in_trap
            && let Some(debug_cmd) = self.env.get_trap("DEBUG").map(|s| s.to_string())
            && !debug_cmd.is_empty()
        {
            self.run_trap_body(&debug_cmd);
        }

        let is_builtin = builtin_is(&words[0]) && !builtin::is_disabled(&words[0]);
        if is_builtin {
            match words[0].as_str() {
                "exec" => {
                    if words.len() > 1 {
                        // The shell survives only on the builtin and
                        // error paths; save std fds so redirections can
                        // be undone there. A real exec replaces the
                        // process image and needs no restore.
                        let save_fds: Vec<i32> = if redirects.is_empty() {
                            Vec::new()
                        } else {
                            (0..3).map(|fd| unsafe { libc::dup(fd) }).collect()
                        };
                        let mut redirect_failed = false;
                        for r in redirects {
                            if !self.apply_redirect(r) {
                                redirect_failed = true;
                                break;
                            }
                        }
                        if redirect_failed {
                            for (i, saved) in save_fds.iter().enumerate() {
                                unsafe {
                                    libc::dup2(*saved, i as i32);
                                    libc::close(*saved);
                                }
                            }
                            self.last_status = 1;
                            signals::set_last_status(1);
                            return 1;
                        }
                        if builtin_is(&words[1]) {
                            let status = builtin::run(
                                &words[1..],
                                &mut self.env,
                                &self.cfg,
                                self.last_status,
                            )
                            .status;
                            for (i, saved) in save_fds.iter().enumerate() {
                                unsafe {
                                    libc::dup2(*saved, i as i32);
                                    libc::close(*saved);
                                }
                            }
                            self.last_status = status;
                            signals::set_last_status(status);
                            return status;
                        }

                        let mut cmd_idx = 1;
                        let mut clear_env = false;
                        let mut login_shell = false;
                        let mut argv0: Option<String> = None;
                        while cmd_idx < words.len()
                            && words[cmd_idx].starts_with('-')
                            && words[cmd_idx].len() > 1
                        {
                            let flag = &words[cmd_idx][1..];
                            if flag == "c" {
                                clear_env = true;
                                cmd_idx += 1;
                            } else if flag == "l" || flag == "--login" {
                                login_shell = true;
                                cmd_idx += 1;
                            } else if flag == "a" || flag == "--argv0" {
                                cmd_idx += 1;
                                if cmd_idx < words.len() {
                                    argv0 = Some(words[cmd_idx].clone());
                                    cmd_idx += 1;
                                }
                            } else {
                                break;
                            }
                        }

                        if clear_env {
                            unsafe {
                                libc::clearenv();
                            }
                        }

                        if cmd_idx >= words.len() {
                            for (i, saved) in save_fds.iter().enumerate() {
                                unsafe {
                                    libc::dup2(*saved, i as i32);
                                    libc::close(*saved);
                                }
                            }
                            return 0;
                        }

                        let cmd = &words[cmd_idx];
                        let path = if cmd.contains('/') {
                            cmd.clone()
                        } else if let Some(p) = self.find_in_path(cmd) {
                            p
                        } else {
                            eprintln!("context: exec: {}: command not found", cmd);
                            for (i, saved) in save_fds.iter().enumerate() {
                                unsafe {
                                    libc::dup2(*saved, i as i32);
                                    libc::close(*saved);
                                }
                            }
                            self.last_status = 127;
                            signals::set_last_status(127);
                            return 127;
                        };

                        let c_args: Vec<CString> = words[cmd_idx..]
                            .iter()
                            .map(|w| strip_markers(w))
                            .filter_map(|w| CString::new(w).ok())
                            .collect();
                        let mut c_ptrs: Vec<*const libc::c_char> =
                            c_args.iter().map(|s| s.as_ptr()).collect();
                        c_ptrs.push(std::ptr::null());
                        let exec_path = CString::new(path.as_str()).unwrap_or_else(|_| {
                            CString::new("sh").expect("failed to create CString for sh")
                        });
                        if login_shell {
                            let mut login_name =
                                path.rsplit('/').next().unwrap_or("sh").to_string();
                            login_name.insert(0, '-');
                            unsafe {
                                let c_login = CString::new(login_name)
                                    .unwrap_or_else(|_| CString::new("-sh").unwrap());
                                libc::execvp(c_login.as_ptr(), c_ptrs.as_ptr());
                            }
                        } else if let Some(ref a0) = argv0 {
                            let mut new_args: Vec<CString> =
                                vec![CString::new(a0.as_str()).unwrap()];
                            new_args.extend(c_args.iter().cloned());
                            let mut new_ptrs: Vec<*const libc::c_char> =
                                new_args.iter().map(|s| s.as_ptr()).collect();
                            new_ptrs.push(std::ptr::null());
                            unsafe {
                                libc::execvp(exec_path.as_ptr(), new_ptrs.as_ptr());
                            }
                        } else {
                            unsafe {
                                libc::execvp(exec_path.as_ptr(), c_ptrs.as_ptr());
                            }
                        }
                        let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                        if err == libc::ENOENT {
                            eprintln!("context: exec: {}: command not found", cmd);
                            if self.interactive {
                                for (i, saved) in save_fds.iter().enumerate() {
                                    unsafe {
                                        libc::dup2(*saved, i as i32);
                                        libc::close(*saved);
                                    }
                                }
                                self.last_status = 127;
                                signals::set_last_status(127);
                                return 127;
                            }
                            unsafe {
                                libc::_exit(127);
                            }
                        } else {
                            eprintln!(
                                "context: exec: {}: {}",
                                cmd,
                                std::io::Error::last_os_error()
                            );
                            if self.interactive {
                                for (i, saved) in save_fds.iter().enumerate() {
                                    unsafe {
                                        libc::dup2(*saved, i as i32);
                                        libc::close(*saved);
                                    }
                                }
                                self.last_status = 126;
                                signals::set_last_status(126);
                                return 126;
                            }
                            unsafe {
                                libc::_exit(126);
                            }
                        }
                    }

                    for r in redirects {
                        if !self.apply_redirect(r) {
                            return 1;
                        }
                    }
                    return 0;
                }
                "jobs" => {
                    self.cleanup_jobs();
                    let mut show_running = false;
                    let mut show_stopped = false;
                    let mut long_form = false;
                    let mut pids_only = false;
                    for arg in &words[1..] {
                        match arg.as_str() {
                            "-r" => show_running = true,
                            "-s" => show_stopped = true,
                            "-l" => long_form = true,
                            "-p" => pids_only = true,
                            _ => {}
                        }
                    }
                    for job in &self.jobs {
                        if show_running && !job.running {
                            continue;
                        }
                        if show_stopped && job.running {
                            continue;
                        }
                        if pids_only {
                            println!("{}", job.pid);
                        } else if long_form {
                            let status = if job.running { "Running" } else { "Stopped" };
                            println!("[{}]  {} {}\t{}", job.id, status, job.pid, job.cmd);
                        } else {
                            let status = if job.running { "Running" } else { "Stopped" };
                            println!("[{}]  {} {}", job.id, status, job.cmd);
                        }
                    }
                    return 0;
                }
                "fg" => {
                    let job_id = words
                        .get(1)
                        .and_then(|s| s.strip_prefix('%'))
                        .and_then(|s| s.parse::<usize>().ok())
                        .or_else(|| words.get(1).and_then(|s| s.parse::<usize>().ok()));
                    let job_opt = if let Some(id) = job_id {
                        self.jobs
                            .iter()
                            .position(|j| j.id == id)
                            .map(|pos| self.jobs.remove(pos))
                    } else {
                        self.jobs
                            .iter()
                            .rposition(|j| j.running)
                            .map(|pos| self.jobs.remove(pos))
                    };
                    if let Some(job) = job_opt {
                        unsafe {
                            libc::kill(-job.pid, libc::SIGCONT);
                            libc::tcsetpgrp(libc::STDIN_FILENO, job.pid);
                        }
                        signals::CHILD_PID.store(job.pid, std::sync::atomic::Ordering::SeqCst);
                        signals::RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
                        let exit = self.wait_for_pid(job.pid);
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                        }
                        signals::RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                        signals::CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
                        signals::set_foreground(0);
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
                    let job_id = words
                        .get(1)
                        .and_then(|s| s.strip_prefix('%'))
                        .and_then(|s| s.parse::<usize>().ok())
                        .or_else(|| words.get(1).and_then(|s| s.parse::<usize>().ok()));
                    if let Some(id) = job_id {
                        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                            unsafe {
                                libc::kill(-job.pid, libc::SIGCONT);
                            }
                            job.running = true;
                            println!("[{}] {} &", job.id, job.cmd);
                            return 0;
                        }
                    } else if let Some(job) = self.jobs.iter_mut().rev().find(|j| !j.running) {
                        unsafe {
                            libc::kill(-job.pid, libc::SIGCONT);
                        }
                        job.running = true;
                        println!("[{}] {} &", job.id, job.cmd);
                        return 0;
                    }
                    eprintln!("context: bg: no such job");
                    return 1;
                }
                "break" => {
                    let n = words
                        .get(1)
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(1);
                    if self.loop_depth == 0 {
                        eprintln!("context: break: only meaningful in a loop");
                        self.last_status = 1;
                        signals::set_last_status(1);
                        return 1;
                    }
                    if n > self.loop_depth as u32 {
                        eprintln!("context: break: {}: loop levels exceeded", n);
                        self.last_status = 1;
                        signals::set_last_status(1);
                        return 1;
                    }
                    self.loop_control = Some(LoopControl::Break(n.max(1)));
                    self.last_status = 0;
                    signals::set_last_status(0);
                    return 0;
                }
                "continue" => {
                    let n = words
                        .get(1)
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(1);
                    if self.loop_depth == 0 {
                        eprintln!("context: continue: only meaningful in a loop");
                        self.last_status = 1;
                        signals::set_last_status(1);
                        return 1;
                    }
                    if n > self.loop_depth as u32 {
                        eprintln!("context: continue: {}: loop levels exceeded", n);
                        self.last_status = 1;
                        signals::set_last_status(1);
                        return 1;
                    }
                    self.loop_control = Some(LoopControl::Continue(n));
                    self.last_status = 0;
                    signals::set_last_status(0);
                    return 0;
                }
                "return" => {
                    if self.function_depth == 0 {
                        eprintln!(
                            "context: return: can only `return` from a function or sourced script"
                        );
                        self.last_status = 1;
                        signals::set_last_status(1);
                        return 1;
                    }
                    let n = words
                        .get(1)
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(self.last_status);
                    self.return_value = Some(n);
                    self.last_status = n;
                    signals::set_last_status(n);
                    return n;
                }
                _ => {}
            }
            let words = if words[0] == "kill" || words[0] == "wait" {
                let mut resolved = words.clone();
                let skip = 1;
                for arg in resolved.iter_mut().skip(skip) {
                    if let Some(job_id_str) = arg.strip_prefix('%')
                        && let Ok(job_id) = job_id_str.parse::<usize>()
                        && let Some(job) = self.jobs.iter().find(|j| j.id == job_id)
                    {
                        *arg = job.pid.to_string();
                    }
                }
                resolved
            } else {
                words.clone()
            };
            let mut expanded_redirects = redirects.to_vec();
            for r in &mut expanded_redirects {
                if let RedirKind::HereDocBody(ref body, expand) = r.kind
                    && expand
                {
                    let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                    let mut expander =
                        Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                    let expanded = strip_markers(&expander.expand_word(body));
                    r.kind = RedirKind::HereDocBody(expanded, false);
                }
            }
            let save_fds: Vec<i32> = if expanded_redirects.is_empty() {
                Vec::new()
            } else {
                (0..3).map(|fd| unsafe { libc::dup(fd) }).collect()
            };
            if !save_fds.is_empty() {
                let mut redirect_failed = false;
                for r in &expanded_redirects {
                    if !self.apply_redirect(r) {
                        redirect_failed = true;
                        break;
                    }
                }
                if redirect_failed {
                    for (i, saved) in save_fds.iter().enumerate() {
                        unsafe {
                            libc::dup2(*saved, i as i32);
                            libc::close(*saved);
                        }
                    }
                    self.last_status = 1;
                    signals::set_last_status(1);
                    return 1;
                }
            }
            let result = builtin::run(&words, &mut self.env, &self.cfg, self.last_status);
            if result.needs_executor {
                // Redirections are already applied under `save_fds`;
                // restore them whenever the shell survives this command.
                let restore_fds = |fds: &[i32]| {
                    for (i, saved) in fds.iter().enumerate() {
                        unsafe {
                            libc::dup2(*saved, i as i32);
                            libc::close(*saved);
                        }
                    }
                };
                if words.len() > 1 && builtin_is(&words[1]) {
                    let status =
                        builtin::run(&words[1..], &mut self.env, &self.cfg, self.last_status)
                            .status;
                    restore_fds(&save_fds);
                    self.last_status = status;
                    signals::set_last_status(status);
                    return status;
                }
                if words.len() > 1 {
                    let mut cmd_idx = 1;
                    let mut clear_env = false;
                    while cmd_idx < words.len()
                        && words[cmd_idx].starts_with('-')
                        && words[cmd_idx].len() > 1
                    {
                        let flag = &words[cmd_idx][1..];
                        if flag == "c" {
                            clear_env = true;
                            cmd_idx += 1;
                        } else {
                            break;
                        }
                    }
                    if clear_env {
                        unsafe {
                            libc::clearenv();
                        }
                    }
                    if cmd_idx >= words.len() {
                        restore_fds(&save_fds);
                        return 0;
                    }
                    let cmd = &words[cmd_idx];
                    let path = if cmd.contains('/') {
                        cmd.clone()
                    } else if let Some(p) = self.find_in_path(cmd) {
                        p
                    } else {
                        eprintln!("context: exec: {}: command not found", cmd);
                        restore_fds(&save_fds);
                        self.last_status = 127;
                        signals::set_last_status(127);
                        return 127;
                    };
                    let c_args: Vec<CString> = words[cmd_idx..]
                        .iter()
                        .map(|w| strip_markers(w))
                        .filter_map(|w| CString::new(w).ok())
                        .collect();
                    let mut c_ptrs: Vec<*const libc::c_char> =
                        c_args.iter().map(|s| s.as_ptr()).collect();
                    c_ptrs.push(std::ptr::null());
                    let c_path = CString::new(path.as_str()).unwrap_or_else(|_| {
                        CString::new("sh").expect("failed to create CString for sh")
                    });
                    // Replacing the shell: no fd restore by design.
                    unsafe {
                        libc::execvp(c_path.as_ptr(), c_ptrs.as_ptr());
                    }
                    let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                    if err == libc::ENOENT {
                        eprintln!("context: exec: {}: command not found", cmd);
                        if self.interactive {
                            restore_fds(&save_fds);
                            self.last_status = 127;
                            signals::set_last_status(127);
                            return 127;
                        }
                        unsafe {
                            libc::_exit(127);
                        }
                    } else {
                        eprintln!(
                            "context: exec: {}: {}",
                            cmd,
                            std::io::Error::last_os_error()
                        );
                        if self.interactive {
                            restore_fds(&save_fds);
                            self.last_status = 126;
                            signals::set_last_status(126);
                            return 126;
                        }
                        unsafe {
                            libc::_exit(126);
                        }
                    }
                }
                return 0;
            }
            if !save_fds.is_empty() {
                for (i, saved) in save_fds.iter().enumerate() {
                    unsafe {
                        libc::dup2(*saved, i as i32);
                        libc::close(*saved);
                    }
                }
            }
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
                if let Some(extra) = result.source_args {
                    let old_positional = self.env.positional().to_vec();
                    self.env.set_positional(extra);
                    let status = self.source_file(&path);
                    self.env.set_positional(old_positional);
                    self.last_status = status;
                    signals::set_last_status(status);
                } else {
                    let status = self.source_file(&path);
                    self.last_status = status;
                    signals::set_last_status(status);
                }
                return self.last_status;
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

        if background {
            self.exec_background(&words, redirects);
            return 0;
        }

        let mut expanded_redirects = redirects.to_vec();
        for r in &mut expanded_redirects {
            if let RedirKind::HereDocBody(ref body, expand) = r.kind
                && expand
            {
                let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                let expanded = strip_markers(&expander.expand_word(body));
                r.kind = RedirKind::HereDocBody(expanded, false);
            }
        }
        self.exec_external(&words, &expanded_redirects)
    }

    /// Apply `FOO=bar cmd` prefix assignments on top of the current state,
    /// exporting them so external children inherit them. Returns an undo log.
    fn apply_prefix_env(
        &mut self,
        assigns: &[(String, String)],
    ) -> Vec<(String, Option<(String, bool)>)> {
        if assigns.is_empty() {
            return Vec::new();
        }
        let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
        let mut expanded: Vec<(String, String)> = Vec::new();
        {
            let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
            for (k, v) in assigns {
                expanded.push((k.clone(), strip_markers(&expander.expand_word(v))));
            }
        }
        let mut saved = Vec::new();
        for (k, v) in expanded {
            let old = self.env.get(&k).map(|s| s.to_string());
            let was_exported = self.env.is_exported(&k);
            saved.push((k.clone(), old.map(|o| (o, was_exported))));
            self.env.set_exported(&k, &v, true);
        }
        saved
    }

    fn restore_prefix_env(&mut self, saved: Vec<(String, Option<(String, bool)>)>) {
        for (k, old) in saved.into_iter().rev() {
            match old {
                Some((v, was_exported)) => {
                    self.env.set_exported(&k, &v, was_exported);
                    if !was_exported {
                        self.env.unexport(&k);
                    }
                }
                None => {
                    self.env.unset(&k);
                }
            }
        }
    }

    fn execute_node(&mut self, node: &Node) -> i32 {
        match node {
            Node::Empty => 0,
            Node::Command {
                words,
                redirects,
                background,
                prefix_env,
            } => {
                // Prefix assignments (`FOO=bar cmd`) are scoped to this
                // command and exported so external children see them.
                let saved_prefix = self.apply_prefix_env(prefix_env);
                let status = self.run_simple_command(words, redirects, *background);
                self.restore_prefix_env(saved_prefix);
                status
            }
            Node::Pipeline { commands, bang } => {
                let n = commands.len();
                if n == 0 {
                    return 0;
                }
                if n == 1 {
                    // `! cmd` — the inverted pipeline is a condition.
                    return if *bang {
                        self.execute_condition(&commands[0])
                    } else {
                        self.execute(&commands[0])
                    };
                }
                self.env.set("PIPESTATUS", "0");
                let lastpipe = self
                    .env
                    .get("_SHOPT_LASTPIPE")
                    .map(|s| s == "1")
                    .unwrap_or(false);
                let last_is_simple =
                    lastpipe && matches!(commands.last(), Some(Node::Command { .. }));
                let mut pipes: Vec<[i32; 2]> = Vec::new();
                for _ in 0..n - 1 {
                    let mut fds = [0i32; 2];
                    unsafe {
                        libc::pipe(fds.as_mut_ptr());
                    }
                    pipes.push(fds);
                }
                let mut children: Vec<i32> = Vec::new();
                let fork_count = if last_is_simple { n - 1 } else { n };
                // The whole pipeline shares one process group so terminal
                // control can be handed to it as a unit.
                let mut pgid: i32 = 0;
                for (i, cmd) in commands.iter().take(fork_count).enumerate() {
                    match unsafe { libc::fork() } {
                        -1 => {
                            for p in &pipes {
                                unsafe {
                                    libc::close(p[0]);
                                    libc::close(p[1]);
                                }
                            }
                            for pid in &children {
                                unsafe {
                                    libc::kill(*pid, libc::SIGTERM);
                                }
                                let mut status: i32 = 0;
                                unsafe {
                                    libc::waitpid(*pid, &mut status, 0);
                                }
                            }
                            return 1;
                        }
                        0 => {
                            unsafe {
                                if i > 0 {
                                    libc::dup2(pipes[i - 1][0], libc::STDIN_FILENO);
                                }
                                if i < n - 1 {
                                    libc::dup2(pipes[i][1], libc::STDOUT_FILENO);
                                }
                                for p in &pipes {
                                    libc::close(p[0]);
                                    libc::close(p[1]);
                                }
                                if pgid == 0 {
                                    libc::setpgid(0, 0);
                                } else {
                                    libc::setpgid(0, pgid);
                                }
                            }
                            signals::setup_child_handlers();
                            let status = self.execute(cmd);
                            unsafe {
                                libc::_exit(status);
                            }
                        }
                        pid => {
                            if pgid == 0 {
                                pgid = pid;
                            }
                            children.push(pid);
                            unsafe {
                                // Parent-side setpgid closes the race with
                                // tcsetpgrp and child-side calls.
                                libc::setpgid(pid, pgid);
                                if i > 0 {
                                    libc::close(pipes[i - 1][0]);
                                }
                                if i < n - 1 {
                                    libc::close(pipes[i][1]);
                                }
                            }
                        }
                    }
                }
                let foreground_tty =
                    !self.in_background_job && unsafe { libc::isatty(libc::STDIN_FILENO) } == 1;
                if foreground_tty && pgid > 0 {
                    unsafe {
                        libc::tcsetpgrp(libc::STDIN_FILENO, pgid);
                    }
                }
                signals::CHILD_PID.store(pgid, std::sync::atomic::Ordering::SeqCst);
                signals::RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
                let lastpipe_status = if last_is_simple {
                    let saved_stdin = unsafe { libc::dup(libc::STDIN_FILENO) };
                    unsafe {
                        libc::dup2(pipes[n - 2][0], libc::STDIN_FILENO);
                    }
                    for p in &pipes {
                        unsafe {
                            libc::close(p[0]);
                            libc::close(p[1]);
                        }
                    }
                    let status = self.execute(commands.last().unwrap());
                    unsafe {
                        libc::dup2(saved_stdin, libc::STDIN_FILENO);
                        libc::close(saved_stdin);
                    }
                    Some(status)
                } else {
                    for p in &pipes {
                        unsafe {
                            libc::close(p[0]);
                            libc::close(p[1]);
                        }
                    }
                    None
                };
                let mut last_status = 0;
                let mut any_nonzero = 0;
                self.pipe_statuses.clear();
                for pid in children {
                    let mut status: i32 = 0;
                    // WUNTRACED so a stopped child never hangs the wait.
                    loop {
                        let ret = unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) };
                        if ret != -1
                            || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR)
                        {
                            break;
                        }
                    }
                    let exit = if libc::WIFEXITED(status) {
                        libc::WEXITSTATUS(status)
                    } else if libc::WIFSIGNALED(status) {
                        128 + libc::WTERMSIG(status)
                    } else if libc::WIFSTOPPED(status) {
                        128 + libc::WSTOPSIG(status)
                    } else {
                        1
                    };
                    self.pipe_statuses.push(exit);
                    if exit != 0 {
                        any_nonzero = exit;
                    }
                    last_status = exit;
                }
                if foreground_tty {
                    unsafe {
                        libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                    }
                }
                signals::RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                signals::CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
                signals::set_foreground(0);
                if let Some(lp_status) = lastpipe_status {
                    self.pipe_statuses.push(lp_status);
                    if lp_status != 0 {
                        any_nonzero = lp_status;
                    }
                    last_status = lp_status;
                }
                if (self.opt_pipefail() || self.cfg.execution.exit_on_pipefail) && any_nonzero != 0
                {
                    last_status = any_nonzero;
                }
                if *bang {
                    last_status = if last_status == 0 { 1 } else { 0 };
                }
                let pipe_str = self
                    .pipe_statuses
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                self.env.set("PIPESTATUS", &pipe_str);
                self.last_status = last_status;
                signals::set_last_status(last_status);
                last_status
            }
            Node::Compound { kind, left, right } => {
                match kind {
                    CompoundKind::And => {
                        let s = self.execute_condition(left);
                        let status = if s == 0 { self.execute(right) } else { s };
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                    CompoundKind::Or => {
                        let s = self.execute_condition(left);
                        let status = if s != 0 { self.execute(right) } else { s };
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                    CompoundKind::Semicolon => {
                        let ls = self.execute(left);
                        if self.return_value.is_some() {
                            self.last_status = ls;
                            signals::set_last_status(ls);
                            return ls;
                        }
                        if self.loop_control.is_some() && self.loop_depth > 0 {
                            self.last_status = ls;
                            signals::set_last_status(ls);
                            return ls;
                        }
                        let status = self.execute(right);
                        self.last_status = status;
                        signals::set_last_status(status);
                        status
                    }
                    CompoundKind::Background => {
                        // Fork before evaluating so the shell never blocks on
                        // the left side; the parent continues immediately.
                        match unsafe { libc::fork() } {
                            -1 => {
                                eprintln!("context: fork failed");
                                self.execute(left);
                                let status = self.execute(right);
                                self.last_status = status;
                                signals::set_last_status(status);
                                status
                            }
                            0 => {
                                unsafe {
                                    libc::setsid();
                                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                                    let devnull = libc::open(
                                        c"/dev/null".as_ptr() as *const _,
                                        libc::O_RDONLY,
                                    );
                                    libc::dup2(devnull, libc::STDIN_FILENO);
                                    libc::close(devnull);
                                }
                                signals::setup_child_handlers();
                                self.in_background_job = true;
                                let status = self.execute(left);
                                unsafe {
                                    libc::_exit(status);
                                }
                            }
                            pid => {
                                let desc = describe_node(left);
                                self.register_background_job(pid, &desc);
                                let status = self.execute(right);
                                self.last_status = status;
                                signals::set_last_status(status);
                                status
                            }
                        }
                    }
                }
            }
            Node::Subshell { body } => match unsafe { libc::fork() } {
                -1 => {
                    eprintln!("context: fork failed");
                    1
                }
                0 => {
                    signals::setup_child_handlers();
                    let status = self.execute(body);
                    unsafe {
                        libc::_exit(status);
                    }
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
                        if ret != -1
                            || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR)
                        {
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
                    } else {
                        1
                    };
                    self.last_status = exit;
                    signals::set_last_status(exit);
                    exit
                }
            },
            Node::BraceGroup { body } => {
                let status = self.execute(body);
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::Assignment { name, value } => {
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                let value = strip_markers(&expander.expand_word(value));
                let pending = expander.take_pending_sets();
                drop(expander);
                for (k, v) in pending {
                    self.env.set(&k, &v);
                }
                // Array element assignment: `arr[expr]=value`.
                if name.contains('[') && name.ends_with(']') {
                    let bracket = name.find('[').unwrap();
                    let arr_name = &name[..bracket];
                    let key = &name[bracket + 1..name.len() - 1];
                    let key_expanded = {
                        let mut expander =
                            Expander::new(&mut self.env, self.last_status, vec![], 0);
                        strip_markers(&expander.expand_word(key))
                    };
                    self.env.create_assoc_array(arr_name);
                    if !key_expanded.is_empty() && key_expanded.chars().all(|c| c.is_ascii_digit())
                    {
                        self.env.indexed_array_set(arr_name, &key_expanded, &value);
                        self.last_status = 0;
                        signals::set_last_status(0);
                        return 0;
                    }
                    self.env.assoc_set(arr_name, &key_expanded, &value);
                    self.last_status = 0;
                    signals::set_last_status(0);
                    return 0;
                }
                if !self.env.set(name, &value) {
                    self.last_status = 1;
                    signals::set_last_status(1);
                    return 1;
                }
                if self.opt_is("_OPT_A") {
                    self.env.export(name);
                }
                0
            }
            Node::Arithmetic { expr } => {
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], 0);
                let expanded = strip_markers(&expander.expand_word(expr));
                drop(expander);
                let value = crate::shell::builtin::eval_arith_assign(&expanded, &mut self.env);
                let status = if value == 0 { 1 } else { 0 };
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::For { var, values, body } => {
                let positional: Vec<String> = if values.is_empty() {
                    (1..)
                        .map_while(|i| self.env.get(&i.to_string()).map(|s| s.to_string()))
                        .collect()
                } else {
                    vec![]
                };
                let iter_values: Vec<String> = if values.is_empty() {
                    positional
                } else {
                    let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                    let mut expander =
                        Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                    let words: Vec<String> = values
                        .iter()
                        .flat_map(|v| expander.expand_words(v))
                        .collect();
                    drop(expander);
                    let words = self.word_split(&words);
                    self.expand_globs(&words).0
                };
                let mut last = 0;
                self.loop_depth += 1;
                for val in &iter_values {
                    self.env.set(var, val);
                    last = self.execute(body);
                    match self.loop_action() {
                        LoopAction::None => {}
                        LoopAction::Stop => break,
                        LoopAction::Continue => continue,
                    }
                }
                self.loop_depth = self.loop_depth.saturating_sub(1);
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::ForArith {
                init,
                cond,
                incr,
                body,
            } => {
                let apply = |e: &str, env: &mut crate::shell::env::Env| {
                    crate::shell::builtin::eval_arith_assign(e, env);
                };
                if let Some(init_expr) = init {
                    apply(init_expr, &mut self.env);
                }
                let mut last = 0;
                self.loop_depth += 1;
                loop {
                    let cond_true = match cond {
                        Some(c) => {
                            let v = crate::shell::builtin::eval_arith_assign(c, &mut self.env);
                            v != 0
                        }
                        None => true,
                    };
                    if !cond_true {
                        break;
                    }
                    last = self.execute(body);
                    match self.loop_action() {
                        LoopAction::None => {}
                        LoopAction::Stop => break,
                        LoopAction::Continue => {}
                    }
                    if let Some(incr_expr) = incr {
                        apply(incr_expr, &mut self.env);
                    }
                }
                self.loop_depth = self.loop_depth.saturating_sub(1);
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::While { condition, body } => {
                let mut last = 0;
                self.loop_depth += 1;
                loop {
                    let status = self.execute_condition(condition);
                    if status != 0 {
                        break;
                    }
                    last = self.execute(body);
                    match self.loop_action() {
                        LoopAction::None => {}
                        LoopAction::Stop => break,
                        LoopAction::Continue => continue,
                    }
                }
                self.loop_depth = self.loop_depth.saturating_sub(1);
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::Until { condition, body } => {
                let mut last = 0;
                self.loop_depth += 1;
                loop {
                    let status = self.execute_condition(condition);
                    if status == 0 {
                        break;
                    }
                    last = self.execute(body);
                    match self.loop_action() {
                        LoopAction::None => {}
                        LoopAction::Stop => break,
                        LoopAction::Continue => continue,
                    }
                }
                self.loop_depth = self.loop_depth.saturating_sub(1);
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::If {
                condition,
                then_body,
                elif,
                else_body,
            } => {
                let status = self.execute_condition(condition);
                if status == 0 {
                    let s = self.execute(then_body);
                    self.last_status = s;
                    signals::set_last_status(s);
                    return s;
                }
                for (cond, body) in elif {
                    let s = self.execute_condition(cond);
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
                    let word = strip_markers(&expander.expand_word(word));
                    let arms_expanded: Vec<(Vec<String>, Box<Node>, CaseTerminator)> = arms
                        .iter()
                        .map(|(patterns, body, term)| {
                            let expanded: Vec<String> = patterns
                                .iter()
                                .map(|p| strip_markers(&expander.expand_word(p)))
                                .collect();
                            (expanded, body.clone(), term.clone())
                        })
                        .collect();
                    (word, arms_expanded)
                };
                let mut i = 0;
                let mut fall_through = false;
                while i < expanded_arms.len() {
                    let (ref patterns, ref body, ref terminator) = expanded_arms[i];
                    if !fall_through {
                        let mut matched = false;
                        for pat in patterns {
                            if pat == &expanded_word
                                || crate::shell::expand::glob_match_str(pat, &expanded_word)
                            {
                                matched = true;
                                break;
                            }
                        }
                        if !matched {
                            i += 1;
                            continue;
                        }
                    }
                    fall_through = false;
                    let s = self.execute(body);
                    self.last_status = s;
                    signals::set_last_status(s);
                    match terminator {
                        CaseTerminator::DoubleSemi => return s,
                        CaseTerminator::AmpSemi => {
                            fall_through = true;
                            i += 1;
                        }
                        CaseTerminator::SemiAmp => {
                            i += 1;
                        }
                    }
                }
                0
            }
            Node::Function { name, body } => {
                self.functions
                    .lock()
                    .unwrap()
                    .insert(name.clone(), *body.clone());
                0
            }
            Node::TestDoubleBracket { tokens } => {
                let nounset = self.opt_u();
                let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                let mut expander = Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                expander.set_nounset(nounset);
                let expanded: Vec<String> = tokens
                    .iter()
                    .map(|t| strip_markers(&expander.expand_word(t)))
                    .collect();
                let nounset_err = expander.had_nounset_error();
                drop(expander);
                if nounset_err {
                    return 2;
                }
                let status = eval_test_bracket(&expanded);
                self.last_status = status;
                signals::set_last_status(status);
                status
            }
            Node::Select { var, values, body } => {
                let iter_values: Vec<String> = if values.is_empty()
                    || (values.len() == 1 && values[0] == "$@")
                {
                    self.env.positional().to_vec()
                } else {
                    let bg_pid = signals::BACKGROUND_PID.load(std::sync::atomic::Ordering::SeqCst);
                    let mut expander =
                        Expander::new(&mut self.env, self.last_status, vec![], bg_pid);
                    let words: Vec<String> = values
                        .iter()
                        .flat_map(|v| expander.expand_words(v))
                        .collect();
                    drop(expander);
                    let words = self.word_split(&words);
                    self.expand_globs(&words).0
                };
                let use_editor = unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
                    && builtin::READLINE_CB.get().is_some();
                let stdin = std::io::stdin();
                let mut last = 0;
                self.loop_depth += 1;
                loop {
                    for (i, item) in iter_values.iter().enumerate() {
                        println!("  {}) {}", i + 1, item);
                    }
                    let ps3 = self.env.get("PS3").unwrap_or("#? ").to_string();
                    eprint!("{}", ps3);
                    let _ = std::io::stderr().flush();
                    let line = if use_editor {
                        match builtin::READLINE_CB.get().unwrap()(&ps3) {
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
                    self.env.set("REPLY", &line);
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(n) = line.parse::<usize>()
                        && n > 0
                        && n <= iter_values.len()
                    {
                        self.env.set(var, &iter_values[n - 1]);
                        last = self.execute(body);
                        match self.loop_action() {
                            LoopAction::None => {}
                            LoopAction::Stop => break,
                            LoopAction::Continue => continue,
                        }
                        continue;
                    }
                    eprintln!("context: select: invalid selection");
                }
                self.loop_depth = self.loop_depth.saturating_sub(1);
                self.last_status = last;
                signals::set_last_status(last);
                last
            }
            Node::Coproc { name, body } => {
                let mut pipe_in = [0i32; 2];
                let mut pipe_out = [0i32; 2];
                unsafe {
                    libc::pipe(pipe_in.as_mut_ptr());
                    libc::pipe(pipe_out.as_mut_ptr());
                }
                match unsafe { libc::fork() } {
                    -1 => {
                        unsafe {
                            libc::close(pipe_in[0]);
                            libc::close(pipe_in[1]);
                            libc::close(pipe_out[0]);
                            libc::close(pipe_out[1]);
                        }
                        eprintln!("context: coproc: fork failed");
                        1
                    }
                    0 => {
                        unsafe {
                            libc::close(pipe_in[1]);
                            libc::close(pipe_out[0]);
                            libc::dup2(pipe_in[0], libc::STDIN_FILENO);
                            libc::close(pipe_in[0]);
                            libc::dup2(pipe_out[1], libc::STDOUT_FILENO);
                            libc::close(pipe_out[1]);
                            libc::setpgid(0, 0);
                            libc::signal(libc::SIGHUP, libc::SIG_IGN);
                        }
                        signals::setup_child_handlers();
                        let status = self.execute(body);
                        unsafe {
                            libc::_exit(status);
                        }
                    }
                    pid => {
                        unsafe {
                            libc::close(pipe_in[0]);
                            libc::close(pipe_out[1]);
                        }
                        let read_fd = pipe_out[0];
                        let write_fd = pipe_in[1];
                        COPROC_FDS
                            .lock()
                            .unwrap()
                            .push((name.clone(), read_fd, write_fd));
                        if let Some(n) = name {
                            self.env.set(&format!("COPROC_{}_PID", n), &pid.to_string());
                            self.env.set("COPROC_PID", &pid.to_string());
                            self.env.set("COPROC", n);
                        } else {
                            self.env.set("COPROC_PID", &pid.to_string());
                            self.env.set("COPROC", "");
                        }
                        0
                    }
                }
            }
        }
    }

    fn format_done_msg(&self, command: &str, exit: i32) -> String {
        self.cfg
            .jobs
            .done_format
            .expand(&[("command", command), ("exit_code", &exit.to_string())])
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
            let hook = &self.cfg.execution.command_not_found_hook;
            if !hook.is_empty() {
                use std::process::Command;
                let status = Command::new(hook).arg(cmd).status();
                return match status {
                    Ok(s) => s.code().unwrap_or(127),
                    Err(_) => 127,
                };
            }
            if self.cfg.execution.cdspell {
                if let Some(suggestion) = spell_correct(cmd) {
                    eprintln!(
                        "context: {}: command not found. Did you mean '{}'?",
                        cmd, suggestion
                    );
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
            eprintln!(
                "context: job limit reached (max {})",
                self.cfg.jobs.max_jobs
            );
            return 125;
        }

        if self.cfg.security.no_exec_commands.contains(&words[0]) {
            eprintln!("context: {}: command blocked by security policy", words[0]);
            return 1;
        }

        // posix_spawn only handles plain file redirections. Here-docs,
        // here-strings, fd-duplication and fd-opening need the fork path —
        // silently dropping them loses stdin/stdout for the command.
        let use_posix_spawn = self.cfg.execution.fork_method == "spawn"
            && !redirects.iter().any(|r| {
                matches!(
                    r.kind,
                    RedirKind::HereDocBody(_, _)
                        | RedirKind::HereString(_)
                        | RedirKind::RedirectFd
                        | RedirKind::RedirectOpen
                )
            });

        if use_posix_spawn {
            let mut c_args: Vec<CString> = words
                .iter()
                .map(|w| strip_markers(w))
                .filter_map(|w| CString::new(w).ok())
                .collect();
            let mut c_ptrs: Vec<*mut libc::c_char> = c_args
                .iter_mut()
                .map(|s| s.as_ptr() as *mut libc::c_char)
                .collect();
            c_ptrs.push(std::ptr::null_mut());

            let mut env_vars: Vec<CString> = self
                .env
                .all_vars()
                .iter()
                .filter_map(|(k, v)| CString::new(format!("{}={}", k, v)).ok())
                .collect();
            let mut env_ptrs: Vec<*mut libc::c_char> = env_vars
                .iter_mut()
                .map(|s| s.as_ptr() as *mut libc::c_char)
                .collect();
            env_ptrs.push(std::ptr::null_mut());

            let c_path = CString::new(path.as_str())
                .unwrap_or_else(|_| CString::new("sh").expect("failed to create CString for sh"));

            let mut file_actions: libc::posix_spawn_file_actions_t = unsafe { std::mem::zeroed() };
            let mut attr: libc::posix_spawnattr_t = unsafe { std::mem::zeroed() };
            let mut redir_files: Vec<File> = Vec::new();
            let mut owned_fds: Vec<RawFd> = Vec::new();
            unsafe {
                libc::posix_spawn_file_actions_init(&mut file_actions);
                libc::posix_spawnattr_init(&mut attr);
                for r in redirects {
                    match r.kind {
                        RedirKind::Output | RedirKind::OutputFd | RedirKind::Clobber => {
                            if let Ok(file) = File::create(&r.target) {
                                let fd = file.as_raw_fd();
                                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                                libc::posix_spawn_file_actions_adddup2(
                                    &mut file_actions,
                                    fd,
                                    libc::STDOUT_FILENO,
                                );
                                if matches!(r.kind, RedirKind::OutputFd | RedirKind::Clobber) {
                                    libc::posix_spawn_file_actions_adddup2(
                                        &mut file_actions,
                                        fd,
                                        libc::STDERR_FILENO,
                                    );
                                }
                                redir_files.push(file);
                            }
                        }
                        RedirKind::OutputAppend | RedirKind::OutputFdAppend => {
                            if let Ok(file) =
                                OpenOptions::new().create(true).append(true).open(&r.target)
                            {
                                let fd = file.as_raw_fd();
                                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                                libc::posix_spawn_file_actions_adddup2(
                                    &mut file_actions,
                                    fd,
                                    libc::STDOUT_FILENO,
                                );
                                if matches!(r.kind, RedirKind::OutputFdAppend) {
                                    libc::posix_spawn_file_actions_adddup2(
                                        &mut file_actions,
                                        fd,
                                        libc::STDERR_FILENO,
                                    );
                                }
                                redir_files.push(file);
                            }
                        }
                        RedirKind::Input | RedirKind::InputFd => {
                            if let RedirKind::InputFd = r.kind {
                                let fd = r.target.parse::<i32>().unwrap_or(-1);
                                if fd >= 0 {
                                    libc::posix_spawn_file_actions_adddup2(
                                        &mut file_actions,
                                        fd,
                                        libc::STDIN_FILENO,
                                    );
                                }
                            } else if let Ok(file) = File::open(&r.target) {
                                let fd = file.as_raw_fd();
                                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                                libc::posix_spawn_file_actions_adddup2(
                                    &mut file_actions,
                                    fd,
                                    libc::STDIN_FILENO,
                                );
                                redir_files.push(file);
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
            for f in &redir_files {
                owned_fds.push(f.as_raw_fd());
            }
            drop(redir_files);
            for fd in &owned_fds {
                unsafe {
                    libc::close(*fd);
                }
            }
            unsafe {
                libc::posix_spawn_file_actions_destroy(&mut file_actions);
                libc::posix_spawnattr_destroy(&mut attr);
            }
            if ret != 0 {
                self.fork_count.fetch_sub(1, Ordering::SeqCst);
                eprintln!(
                    "context: posix_spawn failed: {}",
                    std::io::Error::from_raw_os_error(ret)
                );
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
                eprintln!("context: fork failed");
                1
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
            eprintln!(
                "context: job limit reached (max {})",
                self.cfg.jobs.max_jobs
            );
            return;
        }

        if self.cfg.security.no_exec_commands.contains(&words[0]) {
            eprintln!("context: {}: command blocked by security policy", words[0]);
            return;
        }

        match unsafe { libc::fork() } {
            -1 => {
                eprintln!("context: fork failed");
            }
            0 => {
                unsafe {
                    libc::setsid();
                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                    let devnull = libc::open(c"/dev/null".as_ptr() as *const _, libc::O_RDONLY);
                    libc::dup2(devnull, libc::STDIN_FILENO);
                    libc::close(devnull);
                }
                for r in redirects {
                    if !self.apply_redirect(r) {
                        eprintln!("context: {}: redirection failed", r.target);
                        unsafe {
                            libc::_exit(1);
                        }
                    }
                }
                self.child_exec(words, redirects, &path);
            }
            pid => {
                self.register_background_job(pid, &words.join(" "));
            }
        }
    }

    /// Parent-side bookkeeping for a freshly forked background job: timeout
    /// arming, reaper thread, `$!`, job table entry and the `[id] pid` line.
    fn register_background_job(&mut self, pid: i32, cmd: &str) {
        self.fork_count.fetch_add(1, Ordering::SeqCst);
        self.kill_after_timeout(pid, self.cfg.execution.timeout_seconds);
        let fork_count_clone = Arc::clone(&self.fork_count);
        let notify = self.opt_is("_OPT_B");
        let job_cmd = cmd.to_string();
        let done_msg = self
            .cfg
            .jobs
            .done_format
            .expand(&[("command", &job_cmd), ("pid", &pid.to_string())]);
        // Register before spawning so the between-prompts reaper
        // (which runs on this same thread) can never see an
        // unregistered pid for this job.
        signals::watch_register(pid);
        std::thread::spawn(move || {
            let mut status: i32 = 0;
            if let Some(st) = signals::claim_reaped(pid) {
                status = st;
            } else {
                loop {
                    let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
                    if ret == pid {
                        break;
                    }
                    let err = std::io::Error::last_os_error().raw_os_error();
                    if err == Some(libc::EINTR) {
                        continue;
                    }
                    if err == Some(libc::ECHILD) {
                        // Reaper won the race after our first check.
                        status = signals::claim_reaped(pid).unwrap_or(0);
                    }
                    break;
                }
            }
            signals::watch_unregister(pid);
            fork_count_clone.fetch_sub(1, Ordering::SeqCst);
            if notify {
                let exit = if libc::WIFEXITED(status) {
                    libc::WEXITSTATUS(status)
                } else if libc::WIFSIGNALED(status) {
                    128 + libc::WTERMSIG(status)
                } else {
                    1
                };
                let msg = done_msg.replace("{exit_code}", &exit.to_string());
                eprintln!("{}", msg);
            }
        });
        signals::BACKGROUND_PID.store(pid, Ordering::SeqCst);
        self.env.set("!", &pid.to_string());
        let job = Job {
            id: self.next_job_id,
            pid,
            cmd: job_cmd.clone(),
            running: true,
        };
        self.next_job_id += 1;
        println!("[{}] {} {}", job.id, job.pid, job.cmd);
        self.jobs.push(job);
    }

    fn child_exec(&self, words: &[String], redirects: &[Redirect], path: &str) -> ! {
        signals::setup_child_handlers();
        for r in redirects {
            if !self.apply_redirect(r) {
                eprintln!("context: {}: redirection failed", r.target);
                unsafe {
                    libc::_exit(1);
                }
            }
        }
        if self.cfg.execution.strip_env_on_exec {
            unsafe {
                libc::clearenv();
            }
        }
        if self.cfg.execution.bash_compat
            && let Ok(ck) = CString::new("BASH_COMPAT")
            && let Ok(cv) = CString::new("5.2")
        {
            unsafe {
                libc::setenv(ck.as_ptr(), cv.as_ptr(), 1);
            }
        }
        let passthrough: Vec<String> = self.cfg.environment.passthrough.to_vec();
        let filter: Vec<String> = self.cfg.environment.filter.to_vec();
        for (k, v) in self.env.passthrough_env(&passthrough, &filter) {
            if let (Ok(ck), Ok(cv)) = (CString::new(k.as_str()), CString::new(v.as_str())) {
                unsafe {
                    libc::setenv(ck.as_ptr(), cv.as_ptr(), 1);
                }
            }
        }
        for key in &self.cfg.environment.strip_on_exit {
            if let Ok(ck) = CString::new(key.as_str()) {
                unsafe {
                    libc::unsetenv(ck.as_ptr());
                }
            }
        }
        let c_args: Vec<CString> = words
            .iter()
            .map(|w| strip_markers(w))
            .filter_map(|w| CString::new(w).ok())
            .collect();
        let mut c_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
        c_ptrs.push(std::ptr::null());
        let c_cmd = CString::new(path)
            .unwrap_or_else(|_| CString::new("sh").expect("failed to create CString for sh"));
        unsafe {
            libc::execvp(c_cmd.as_ptr(), c_ptrs.as_ptr());
        }
        let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if err == libc::ENOENT {
            eprintln!("context: {}: command not found", words[0]);
            unsafe {
                libc::_exit(127);
            }
        } else {
            eprintln!("context: {}: {}", words[0], std::io::Error::last_os_error());
            unsafe {
                libc::_exit(126);
            }
        }
    }

    fn kill_after_timeout(&self, pid: i32, timeout_secs: u32) {
        if timeout_secs == 0 {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(timeout_secs as u64));
            // Probe aliveness with kill(pid, 0) — a WNOHANG waitpid here
            // would reap the job's zombie and race the job's watcher.
            unsafe {
                let alive = libc::kill(pid, 0) == 0;
                if alive {
                    eprintln!(
                        "context: command timed out after {}s, sending SIGKILL",
                        timeout_secs
                    );
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        });
    }

    fn wait_for_pid(&self, pid: libc::pid_t) -> i32 {
        let tty_out = unsafe { libc::isatty(libc::STDOUT_FILENO) } == 1;
        if self.cfg.editor.hide_cursor_on_exec && tty_out {
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
        // A background watcher may still be parked on this pid (e.g. `fg`);
        // publish what we harvested so its ECHILD fallback can recover.
        signals::note_probe_result(pid, status);
        if self.cfg.editor.hide_cursor_on_exec && tty_out {
            print!("\x1b[?25h");
            let _ = std::io::stdout().flush();
        }
        if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else if libc::WIFSIGNALED(status) {
            128 + libc::WTERMSIG(status)
        } else if libc::WIFSTOPPED(status) {
            128 + libc::WSTOPSIG(status)
        } else if libc::WIFCONTINUED(status) {
            0
        } else {
            1
        }
    }

    fn cleanup_jobs(&mut self) {
        self.jobs.retain(|job| {
            let mut status: i32 = 0;
            let flags = if job.running {
                libc::WNOHANG
            } else {
                libc::WNOHANG | libc::WUNTRACED
            };
            let ret = unsafe { libc::waitpid(job.pid, &mut status, flags) };
            if ret == job.pid {
                // We reaped it here; publish the status so the job's
                // watcher thread can claim it instead of seeing ECHILD.
                signals::note_probe_result(job.pid, status);
                if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
                    return false;
                }
            }
            true
        });
    }

    fn apply_redirect(&self, redirect: &Redirect) -> bool {
        let target_fd: i32 = redirect.fd.unwrap_or(match redirect.kind {
            RedirKind::Output
            | RedirKind::OutputAppend
            | RedirKind::OutputFd
            | RedirKind::OutputFdAppend
            | RedirKind::Clobber
            | RedirKind::RedirectFd => libc::STDOUT_FILENO as u32,
            RedirKind::Input
            | RedirKind::InputFd
            | RedirKind::HereDocBody(_, _)
            | RedirKind::HereString(_)
            | RedirKind::RedirectOpen => libc::STDIN_FILENO as u32,
        }) as i32;
        match redirect.kind {
            RedirKind::Output => {
                if self.opt_n() && Path::new(&redirect.target).exists() {
                    eprintln!(
                        "context: {}: cannot overwrite existing file",
                        redirect.target
                    );
                    return false;
                }
                match File::create(&redirect.target) {
                    Ok(file) => unsafe {
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                        libc::dup2(file.as_raw_fd(), target_fd);
                    },
                    Err(e) => {
                        eprintln!("context: {}: {}", redirect.target, e);
                        return false;
                    }
                }
            }
            RedirKind::OutputAppend => {
                match OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&redirect.target)
                {
                    Ok(file) => unsafe {
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                        libc::dup2(file.as_raw_fd(), target_fd);
                    },
                    Err(e) => {
                        eprintln!("context: {}: {}", redirect.target, e);
                        return false;
                    }
                }
            }
            RedirKind::Input => match File::open(&redirect.target) {
                Ok(file) => unsafe {
                    libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                    libc::dup2(file.as_raw_fd(), target_fd);
                },
                Err(e) => {
                    eprintln!("context: {}: {}", redirect.target, e);
                    return false;
                }
            },
            RedirKind::OutputFd => {
                // `&>` sends stdout and stderr to the file.
                match File::create(&redirect.target) {
                    Ok(file) => unsafe {
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                        libc::dup2(file.as_raw_fd(), target_fd);
                        if target_fd != libc::STDERR_FILENO {
                            libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
                        }
                    },
                    Err(e) => {
                        eprintln!("context: {}: {}", redirect.target, e);
                        return false;
                    }
                }
            }
            RedirKind::OutputFdAppend => {
                // `&>>` appends both stdout and stderr.
                match OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&redirect.target)
                {
                    Ok(file) => unsafe {
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                        libc::dup2(file.as_raw_fd(), target_fd);
                        if target_fd != libc::STDERR_FILENO {
                            libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
                        }
                    },
                    Err(e) => {
                        eprintln!("context: {}: {}", redirect.target, e);
                        return false;
                    }
                }
            }
            RedirKind::RedirectFd => {
                if redirect.target == "-" {
                    unsafe {
                        libc::close(target_fd);
                    }
                } else if let Ok(fd) = redirect.target.parse::<i32>() {
                    unsafe {
                        libc::dup2(fd, target_fd);
                    }
                } else {
                    eprintln!("context: {}: bad file descriptor", redirect.target);
                    return false;
                }
            }
            RedirKind::HereDocBody(ref body, _) => {
                let mut fds = [0i32; 2];
                unsafe {
                    libc::pipe(fds.as_mut_ptr());
                }
                let (r, w) = (fds[0], fds[1]);
                unsafe {
                    libc::dup2(r, target_fd);
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
                unsafe {
                    libc::pipe(fds.as_mut_ptr());
                }
                let (r, w) = (fds[0], fds[1]);
                unsafe {
                    libc::dup2(r, target_fd);
                    libc::close(r);
                }
                let data = format!("{}\n", word);
                std::thread::spawn(move || unsafe {
                    libc::write(w, data.as_ptr() as *const libc::c_void, data.len());
                    libc::close(w);
                });
            }
            RedirKind::Clobber => match File::create(&redirect.target) {
                Ok(file) => unsafe {
                    libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
                    libc::dup2(file.as_raw_fd(), target_fd);
                },
                Err(e) => {
                    eprintln!("context: {}: {}", redirect.target, e);
                    return false;
                }
            },
            RedirKind::InputFd => {
                if redirect.target == "-" {
                    unsafe {
                        libc::close(target_fd);
                    }
                } else if let Ok(fd) = redirect.target.parse::<i32>() {
                    unsafe {
                        libc::dup2(fd, target_fd);
                    }
                } else {
                    eprintln!("context: {}: bad file descriptor", redirect.target);
                    return false;
                }
            }
            RedirKind::RedirectOpen => {
                if let Ok(c_target) = CString::new(redirect.target.as_str()) {
                    unsafe {
                        let fd = libc::open(c_target.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o666);
                        if fd >= 0 {
                            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                            libc::dup2(fd, target_fd);
                            libc::close(fd);
                        } else {
                            eprintln!(
                                "context: {}: {}",
                                redirect.target,
                                std::io::Error::last_os_error()
                            );
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn expand_aliases(&self, words: &[String]) -> Vec<String> {
        if words.is_empty() {
            return words.to_vec();
        }
        let mut result = words.to_vec();
        let mut seen = 0;
        while seen < MAX_ALIAS_EXPAND {
            if let Some(alias_val) = self.env.get_alias(&result[0]) {
                // Re-tokenize the alias value so quoting inside it is
                // preserved (marker-wrapped) instead of blindly splitting.
                let mut alias_words: Vec<String> = Vec::new();
                for t in crate::shell::lexer::tokenize(alias_val) {
                    match t {
                        crate::shell::lexer::Token::Word(w) => alias_words.push(w),
                        crate::shell::lexer::Token::SingleQuoted(s) => {
                            alias_words.push(format!("\x02{}\x02", s));
                        }
                        crate::shell::lexer::Token::DoubleQuoted(s) => {
                            alias_words.push(format!("\x01{}\x01", s));
                        }
                        crate::shell::lexer::Token::Backtick(s) => {
                            alias_words.push(format!("`{}`", s));
                        }
                        crate::shell::lexer::Token::Eof => {}
                        _ => {}
                    }
                }
                if alias_words.is_empty() {
                    result.remove(0);
                    if result.is_empty() {
                        return result;
                    }
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
            if word.is_empty() {
                result.push(String::new());
                continue;
            }
            let mut split = Vec::new();
            let mut current = String::new();
            let mut in_marker = false;
            for ch in word.chars() {
                match ch {
                    '\x01' | '\x02' => in_marker = !in_marker,
                    _ => {
                        if ifs.contains(ch) && !in_marker {
                            if !current.is_empty() {
                                split.push(std::mem::take(&mut current));
                            }
                        } else {
                            current.push(ch);
                        }
                    }
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

    fn expand_globs(&self, words: &[String]) -> (Vec<String>, i32) {
        if self.opt_g() {
            return (words.to_vec(), 0);
        }
        let dotglob = self
            .env
            .get("_SHOPT_DOTGLOB")
            .map(|s| s == "1")
            .unwrap_or(false);
        let nullglob = self
            .env
            .get("_SHOPT_NULLGLOB")
            .map(|s| s == "1")
            .unwrap_or(false);
        let failglob = self
            .env
            .get("_SHOPT_FAILGLOB")
            .map(|s| s == "1")
            .unwrap_or(false);
        let globstar = self
            .env
            .get("_SHOPT_GLOBSTAR")
            .map(|s| s == "1")
            .unwrap_or(false);
        let mut result = Vec::new();
        let mut fail = false;
        for word in words {
            let expanded = expand_braces(word);
            for w in expanded {
                if has_glob_chars(&w) {
                    let matches = if globstar && w.contains("**") {
                        globstar_expand(&w, dotglob)
                    } else {
                        glob_expand_with(&w, dotglob)
                    };
                    if matches.is_empty() {
                        if failglob {
                            eprintln!("context: {}: no match", w);
                            fail = true;
                        } else if !nullglob {
                            result.push(w);
                        }
                    } else {
                        let mut sorted = matches;
                        sorted.sort();
                        result.extend(sorted);
                    }
                } else {
                    result.push(w);
                }
            }
        }
        if fail { (Vec::new(), 1) } else { (result, 0) }
    }

    fn source_file(&mut self, path: &str) -> i32 {
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("context: source: {}: {}", path, e);
                return 1;
            }
        };
        SOURCE_STACK.lock().unwrap().push(path.to_string());
        crate::shell::expand::CURRENT_LINE.store(1, std::sync::atomic::Ordering::Relaxed);
        let tokens = crate::shell::lexer::tokenize(&contents);
        let ast = crate::shell::parser::parse(tokens);
        let status = self.execute(&ast);
        SOURCE_STACK.lock().unwrap().pop();
        status
    }

    fn find_in_path(&self, cmd: &str) -> Option<String> {
        let path_env = if !self.cfg.execution.path_override.is_empty() {
            self.cfg.execution.path_override.clone()
        } else {
            self.env
                .get("PATH")
                .unwrap_or("/system/local/bin:/system/bin:/bin")
                .to_string()
        };
        for dir in path_env.split(':') {
            if dir.is_empty() {
                continue;
            }
            let full = Path::new(dir).join(cmd);
            if full.is_file() {
                return Some(full.to_string_lossy().to_string());
            }
        }
        None
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

    fn opt_e(&self) -> bool {
        self.opt_is("_OPT_E")
    }
    fn opt_u(&self) -> bool {
        self.opt_is("_OPT_U")
    }
    fn opt_x(&self) -> bool {
        self.opt_is("_OPT_X")
    }
    fn opt_n(&self) -> bool {
        self.opt_is("_OPT_N")
    }
    fn opt_g(&self) -> bool {
        self.opt_is("_OPT_G")
    }
    fn opt_pipefail(&self) -> bool {
        self.opt_is("_OPT_PIPEFAIL")
    }

    pub fn run_exit_trap(&mut self) {
        if let Some(cmd) = self.env.get_trap("EXIT").map(|s| s.to_string())
            && !cmd.is_empty()
        {
            let tokens = crate::shell::lexer::tokenize(&cmd);
            let ast = crate::shell::parser::parse(tokens);
            self.execute(&ast);
        }
    }

    fn trace_print(&self, words: &[String]) {
        if self.opt_x() {
            let ps4 = self.env.get("PS4").unwrap_or("+ ");
            let cmd_str: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
            eprintln!("{}{}", ps4, cmd_str.join(" "));
        }
    }
}

fn setup_process_sub(raw_words: &[String]) -> (Vec<String>, Vec<i32>) {
    let mut result = Vec::new();
    let mut child_pids = Vec::new();
    for word in raw_words {
        if let Some(inner) = word.strip_prefix("<(").and_then(|s| s.strip_suffix(')')) {
            let mut fds = [0i32; 2];
            unsafe {
                libc::pipe(fds.as_mut_ptr());
            }
            let (read_fd, write_fd) = (fds[0], fds[1]);
            match unsafe { libc::fork() } {
                -1 => {
                    unsafe {
                        libc::close(read_fd);
                        libc::close(write_fd);
                    }
                    result.push(word.clone());
                }
                0 => {
                    unsafe {
                        libc::close(read_fd);
                        libc::dup2(write_fd, libc::STDOUT_FILENO);
                        libc::close(write_fd);
                        signals::setup_child_handlers();
                    }
                    let tokens = crate::shell::lexer::tokenize(inner);
                    let ast = crate::shell::parser::parse(tokens);
                    let mut exec = Executor::new(Env::new(), Config::default());
                    let status = exec.execute(&ast);
                    unsafe {
                        libc::_exit(status);
                    }
                }
                pid => {
                    unsafe {
                        libc::close(write_fd);
                    }
                    child_pids.push(pid);
                    result.push(format!("/dev/fd/{}", read_fd));
                }
            }
        } else if let Some(inner) = word.strip_prefix(">(").and_then(|s| s.strip_suffix(')')) {
            let mut fds = [0i32; 2];
            unsafe {
                libc::pipe(fds.as_mut_ptr());
            }
            let (read_fd, write_fd) = (fds[0], fds[1]);
            match unsafe { libc::fork() } {
                -1 => {
                    unsafe {
                        libc::close(read_fd);
                        libc::close(write_fd);
                    }
                    result.push(word.clone());
                }
                0 => {
                    unsafe {
                        libc::close(write_fd);
                        libc::dup2(read_fd, libc::STDIN_FILENO);
                        libc::close(read_fd);
                        signals::setup_child_handlers();
                    }
                    let tokens = crate::shell::lexer::tokenize(inner);
                    let ast = crate::shell::parser::parse(tokens);
                    let mut exec = Executor::new(Env::new(), Config::default());
                    let status = exec.execute(&ast);
                    unsafe {
                        libc::_exit(status);
                    }
                }
                pid => {
                    unsafe {
                        libc::close(read_fd);
                    }
                    child_pids.push(pid);
                    result.push(format!("/dev/fd/{}", write_fd));
                }
            }
        } else {
            result.push(word.clone());
        }
    }
    (result, child_pids)
}

fn strip_markers(s: &str) -> String {
    s.chars().filter(|&c| c != '\x01' && c != '\x02').collect()
}

/// Short source-like description of a node, for job table entries.
fn describe_node(node: &Node) -> String {
    match node {
        Node::Command { words, .. } => words.join(" "),
        Node::Pipeline { commands, .. } => commands
            .iter()
            .map(describe_node)
            .collect::<Vec<_>>()
            .join(" | "),
        Node::Compound { kind, left, right } => {
            let sep = match kind {
                CompoundKind::And => " && ",
                CompoundKind::Or => " || ",
                _ => "; ",
            };
            format!("{}{}{}", describe_node(left), sep, describe_node(right))
        }
        Node::Subshell { body } => format!("( {} )", describe_node(body)),
        Node::BraceGroup { body } => format!("{{ {}; }}", describe_node(body)),
        _ => "...".to_string(),
    }
}

/// Reconstruct approximate shell source for a node (used by `typeset -f`).
fn node_to_source(node: &Node, depth: usize) -> String {
    let pad = "    ".repeat(depth);
    let inner = |n: &Node| node_to_source(n, depth + 1);
    match node {
        Node::Command {
            words, redirects, ..
        } => {
            let mut line = words.join(" ");
            for r in redirects {
                let fd = r.fd.map(|f| f.to_string()).unwrap_or_default();
                match &r.kind {
                    RedirKind::HereDocBody(body, _) => {
                        let dash = if body.contains('\t') { "-" } else { "" };
                        line.push_str(&format!(" <<{} _CTX_EOF_\n{}", dash, body));
                        line.push_str("_CTX_EOF_");
                    }
                    RedirKind::HereString(w) => {
                        line.push_str(&format!(" <<< {}", w));
                    }
                    kind => {
                        let op = match kind {
                            RedirKind::Output => ">",
                            RedirKind::OutputAppend => ">>",
                            RedirKind::Input => "<",
                            RedirKind::Clobber => ">|",
                            RedirKind::OutputFd => "&>",
                            RedirKind::OutputFdAppend => "&>>",
                            RedirKind::InputFd => "<&",
                            RedirKind::RedirectFd => ">&",
                            RedirKind::RedirectOpen => "<>",
                            _ => ">",
                        };
                        line.push_str(&format!(" {}{} {}", fd, op, r.target));
                    }
                }
            }
            format!("{}{}\n", pad, line)
        }
        Node::Pipeline { commands, bang } => {
            let joined = commands
                .iter()
                .map(|c| node_to_source(c, depth).trim().to_string())
                .collect::<Vec<_>>()
                .join(" | ");
            format!("{}{}{}\n", pad, if *bang { "! " } else { "" }, joined)
        }
        Node::Compound { kind, left, right } => {
            let sep = match kind {
                CompoundKind::And => " && ",
                CompoundKind::Or => " || ",
                _ => "\n",
            };
            let l = node_to_source(left, depth).trim_end().to_string();
            let r = node_to_source(right, depth).trim().to_string();
            format!("{}{}{} {}\n", pad, l, sep, r)
        }
        Node::Subshell { body } => {
            let b = node_to_source(body, depth + 1)
                .trim_end_matches('\n')
                .to_string();
            format!("{}(\n{})\n", pad, b)
        }
        Node::BraceGroup { body } => {
            let b = node_to_source(body, depth + 1)
                .trim_end_matches('\n')
                .to_string();
            format!("{}{{\n{}\n{}}}\n", pad, b, "    ".repeat(depth))
        }
        Node::If {
            condition,
            then_body,
            elif,
            else_body,
        } => {
            let mut out = format!(
                "{}if {} ; then\n{}\n",
                pad,
                node_to_source(condition, depth).trim(),
                inner(then_body).trim_end_matches('\n')
            );
            for (cond, body) in elif {
                out.push_str(&format!(
                    "{}elif {} ; then\n{}\n",
                    pad,
                    node_to_source(cond, depth).trim(),
                    inner(body).trim_end_matches('\n')
                ));
            }
            if let Some(e) = else_body {
                out.push_str(&format!(
                    "{}else\n{}\n",
                    pad,
                    inner(e).trim_end_matches('\n')
                ));
            }
            out.push_str(&format!("{}fi\n", pad));
            out
        }
        Node::While { condition, body } => format!(
            "{}while {}; do\n{}\n{pad}done\n",
            pad,
            node_to_source(condition, depth).trim(),
            inner(body).trim_end(),
            pad = pad
        ),
        Node::Until { condition, body } => format!(
            "{}until {}; do\n{}\n{pad}done\n",
            pad,
            node_to_source(condition, depth).trim(),
            inner(body).trim_end(),
            pad = pad
        ),
        Node::For { var, values, body } => {
            let vals = if values.is_empty() {
                "$@".to_string()
            } else {
                values.join(" ")
            };
            format!(
                "{}for {} in {}; do\n{}\n{pad}done\n",
                pad,
                var,
                vals,
                inner(body).trim_end_matches('\n'),
                pad = pad
            )
        }
        Node::Case { word, arms } => {
            let mut out = format!("{}case {} in\n", pad, word);
            for (patterns, body, term) in arms {
                let terminator = match term {
                    CaseTerminator::AmpSemi => ";&",
                    CaseTerminator::SemiAmp => ";;&",
                    _ => ";;",
                };
                out.push_str(&format!(
                    "{pad}    {})\n{pad}        {}\n{pad}{})\n",
                    patterns.join("|"),
                    node_to_source(body, depth + 2).trim_end_matches('\n'),
                    terminator,
                    pad = pad
                ));
            }
            out.push_str(&format!("{}esac\n", pad));
            out
        }
        Node::Function { name, body } => {
            // Avoid double-wrapping when the stored body is a brace group.
            if let Node::BraceGroup { body: inner_body } = &**body {
                format!(
                    "{pad}{} () {{\n{}\n{pad}}}\n",
                    name,
                    node_to_source(inner_body, depth).trim_end_matches('\n'),
                    pad = pad
                )
            } else {
                format!(
                    "{pad}{} () {{\n{}\n{pad}}}\n",
                    name,
                    inner(body).trim_end_matches('\n'),
                    pad = pad
                )
            }
        }
        Node::Assignment { name, value } => format!("{}{}={}\n", pad, name, value),
        Node::Arithmetic { expr } => format!("{}(( {} ))\n", pad, expr),
        Node::TestDoubleBracket { tokens } => {
            format!("{}[[ {} ]]\n", pad, tokens.join(" "))
        }
        Node::Coproc { name, body } => format!(
            "{}coproc{} {};\n",
            pad,
            name.clone().map(|n| format!(" {}", n)).unwrap_or_default(),
            node_to_source(body, depth).trim()
        ),
        Node::Empty => String::new(),
        other => format!("{}{:?}\n", pad, other),
    }
}

fn builtin_is(cmd: &str) -> bool {
    crate::shell::builtin::BUILTINS.contains(&cmd)
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
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                result.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    result.push(current);
    result
}

fn has_glob_chars(s: &str) -> bool {
    s.contains('*')
        || s.contains('?')
        || s.contains('[')
        || s.contains("+(")
        || s.contains("!(")
        || s.contains("@(")
}

fn glob_expand_with(pattern: &str, dotglob: bool) -> Vec<String> {
    let options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: !dotglob,
    };
    match glob::glob_with(pattern, options) {
        Ok(paths) => paths
            .filter_map(|e| e.ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn globstar_expand(pattern: &str, dotglob: bool) -> Vec<String> {
    let mut results = Vec::new();
    let star_pos = match pattern.find("**") {
        Some(p) => p,
        None => return glob_expand_with(pattern, dotglob),
    };
    let base_str = pattern[..star_pos].trim_end_matches('/');
    let base_path = if base_str.is_empty() {
        std::path::PathBuf::from(".")
    } else {
        std::path::PathBuf::from(base_str)
    };
    if !base_path.is_dir() {
        return Vec::new();
    }
    let after_star = &pattern[star_pos + 2..];
    let suffix = after_star.strip_prefix('/').unwrap_or(after_star);
    globstar_walk(&base_path, &base_path, suffix, dotglob, &mut results);
    results
}

fn globstar_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    suffix: &str,
    dotglob: bool,
    results: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !dotglob && name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            globstar_walk(root, &path, suffix, dotglob, results);
        }
        if suffix.is_empty() {
            if let Ok(rel) = path.strip_prefix(root) {
                results.push(rel.to_string_lossy().to_string());
            }
            continue;
        }
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: false,
            require_literal_leading_dot: !dotglob,
        };
        let matched = if !suffix.contains('/') {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            glob::Pattern::new(suffix)
                .ok()
                .map(|p| p.matches_with(&file_name, options))
                .unwrap_or(false)
        } else {
            if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy();
                glob::Pattern::new(suffix)
                    .ok()
                    .map(|p| p.matches_with(&rel_str, options))
                    .unwrap_or(false)
            } else {
                false
            }
        };
        if matched {
            if let Ok(rel) = path.strip_prefix(root) {
                results.push(rel.to_string_lossy().to_string());
            } else {
                results.push(path.to_string_lossy().to_string());
            }
        }
    }
}

/// Char-indexed Levenshtein distance — shared with builtin spell-check.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    let mut d = vec![vec![0usize; b_len + 1]; a_len + 1];
    for (i, row) in d.iter_mut().enumerate().take(a_len + 1) {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate().take(b_len + 1) {
        *cell = j;
    }
    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
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
    let mut pos = 0;
    let (mut left, next) = eval_test_and_at(tokens, pos);
    pos = next;
    while pos < tokens.len() && tokens[pos] == "||" {
        pos += 1;
        let (right, next) = eval_test_and_at(tokens, pos);
        left = left || right;
        pos = next;
    }
    left
}

fn eval_test_and_at(tokens: &[String], start: usize) -> (bool, usize) {
    let (mut left, mut pos) = eval_test_primary_at(tokens, start);
    while pos < tokens.len() && tokens[pos] == "&&" {
        pos += 1;
        let (right, next) = eval_test_primary_at(tokens, pos);
        left = left && right;
        pos = next;
    }
    (left, pos)
}

fn eval_test_primary_at(tokens: &[String], start: usize) -> (bool, usize) {
    if start >= tokens.len() {
        return (false, start);
    }
    if tokens[start] == "!" {
        let (val, next) = eval_test_primary_at(tokens, start + 1);
        return (!val, next);
    }
    if tokens[start] == "(" {
        let (val, next) = eval_test_or_at(tokens, start + 1);
        let next = if next < tokens.len() && tokens[next] == ")" {
            next + 1
        } else {
            next
        };
        return (val, next);
    }
    if start + 1 < tokens.len() {
        let op = &tokens[start];
        if op == "-f" {
            return (
                std::path::Path::new(&tokens[start + 1]).is_file(),
                start + 2,
            );
        }
        if op == "-d" {
            return (std::path::Path::new(&tokens[start + 1]).is_dir(), start + 2);
        }
        if op == "-e" {
            return (std::path::Path::new(&tokens[start + 1]).exists(), start + 2);
        }
        if op == "-r" {
            let c = std::ffi::CString::new(tokens[start + 1].as_str()).unwrap_or_default();
            return (
                unsafe { libc::access(c.as_ptr(), libc::R_OK) == 0 },
                start + 2,
            );
        }
        if op == "-w" {
            let c = std::ffi::CString::new(tokens[start + 1].as_str()).unwrap_or_default();
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                let euid = unsafe { libc::geteuid() };
                let egid = unsafe { libc::getegid() };
                let r = if euid == 0 {
                    true
                } else if st.st_uid == euid {
                    (st.st_mode & libc::S_IWUSR) != 0
                } else if st.st_gid == egid {
                    (st.st_mode & libc::S_IWGRP) != 0
                } else {
                    (st.st_mode & libc::S_IWOTH) != 0
                };
                return (r, start + 2);
            }
            return (false, start + 2);
        }
        if op == "-x" {
            let c = std::ffi::CString::new(tokens[start + 1].as_str()).unwrap_or_default();
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::stat(c.as_ptr(), &mut st) } == 0 {
                let euid = unsafe { libc::geteuid() };
                let egid = unsafe { libc::getegid() };
                let r = if euid == 0 {
                    true
                } else if st.st_uid == euid {
                    (st.st_mode & libc::S_IXUSR) != 0
                } else if st.st_gid == egid {
                    (st.st_mode & libc::S_IXGRP) != 0
                } else {
                    (st.st_mode & libc::S_IXOTH) != 0
                };
                return (r, start + 2);
            }
            return (false, start + 2);
        }
        if op == "-s" {
            return (
                std::fs::metadata(&tokens[start + 1])
                    .map(|m| m.len() > 0)
                    .unwrap_or(false),
                start + 2,
            );
        }
        if op == "-L" || op == "-h" {
            return (
                std::path::Path::new(&tokens[start + 1]).is_symlink(),
                start + 2,
            );
        }
        if op == "-S" {
            return (
                std::fs::metadata(&tokens[start + 1])
                    .map(|m| m.file_type().is_socket())
                    .unwrap_or(false),
                start + 2,
            );
        }
        if op == "-p" {
            return (
                std::fs::metadata(&tokens[start + 1])
                    .map(|m| m.file_type().is_fifo())
                    .unwrap_or(false),
                start + 2,
            );
        }
        if op == "-c" {
            return (
                std::fs::metadata(&tokens[start + 1])
                    .map(|m| m.file_type().is_char_device())
                    .unwrap_or(false),
                start + 2,
            );
        }
        if op == "-n" {
            return (!tokens[start + 1].is_empty(), start + 2);
        }
        if op == "-z" {
            return (tokens[start + 1].is_empty(), start + 2);
        }
    }
    if start + 2 < tokens.len() {
        let a = &tokens[start];
        let op = &tokens[start + 1];
        let b = &tokens[start + 2];
        if op == "==" || op == "=" {
            return (crate::shell::expand::glob_match_str(b, a), start + 3);
        }
        if op == "!=" {
            return (!crate::shell::expand::glob_match_str(b, a), start + 3);
        }
        if op == "=~" {
            if let Ok(re) = regex::Regex::new(b) {
                if let Some(caps) = re.captures(a) {
                    let mut rematch = BASH_REMATCH.lock().unwrap();
                    rematch.clear();
                    for m in caps.iter() {
                        rematch.push(m.map(|c| c.as_str().to_string()).unwrap_or_default());
                    }
                    return (true, start + 3);
                }
                return (false, start + 3);
            }
            return (false, start + 3);
        }
        if op == "-eq" {
            return (
                a.parse::<i64>().unwrap_or(0) == b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
        if op == "-ne" {
            return (
                a.parse::<i64>().unwrap_or(0) != b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
        if op == "-lt" {
            return (
                a.parse::<i64>().unwrap_or(0) < b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
        if op == "-le" {
            return (
                a.parse::<i64>().unwrap_or(0) <= b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
        if op == "-gt" {
            return (
                a.parse::<i64>().unwrap_or(0) > b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
        if op == "-ge" {
            return (
                a.parse::<i64>().unwrap_or(0) >= b.parse::<i64>().unwrap_or(0),
                start + 3,
            );
        }
    }
    (false, start + 1)
}

fn eval_test_or_at(tokens: &[String], start: usize) -> (bool, usize) {
    let (mut left, mut pos) = eval_test_and_at(tokens, start);
    while pos < tokens.len() && tokens[pos] == "||" {
        pos += 1;
        let (right, next) = eval_test_and_at(tokens, pos);
        left = left || right;
        pos = next;
    }
    (left, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    // C4: brace expansion sees the full `{a,b,c}` text (braces stay inside
    // words in the lexer instead of becoming discarded tokens).
    #[test]
    fn test_expand_braces() {
        assert_eq!(expand_braces("{a,b,c}"), vec!["a", "b", "c"]);
        assert_eq!(expand_braces("pre{1,2}post"), vec!["pre1post", "pre2post"]);
        assert_eq!(expand_braces("{a,b}{x,y}"), vec!["ax", "ay", "bx", "by"]);
    }
}
