use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};
use crate::config::schema::SignalsConfig;

pub static SIGUSR1_CUSTOM_CMD: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
pub static SIGUSR2_CUSTOM_CMD: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static IGNORED_SIGNALS: LazyLock<Mutex<HashSet<i32>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn ignore_signal_trapped(sig: i32) {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_IGN;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(sig, &sa, std::ptr::null_mut());
    }
    if let Ok(mut set) = IGNORED_SIGNALS.lock() {
        set.insert(sig);
    }
}

pub fn restore_signal_default(sig: i32) {
    unsafe {
        default_signal(sig);
    }
    if let Ok(mut set) = IGNORED_SIGNALS.lock() {
        set.remove(&sig);
    }
}

pub fn is_signal_ignored(sig: i32) -> bool {
    IGNORED_SIGNALS.lock().map(|s| s.contains(&sig)).unwrap_or(false)
}

pub static CHILD_PID: AtomicI32 = AtomicI32::new(0);
pub static RUNNING: AtomicBool = AtomicBool::new(false);
pub static NEED_REDRAW: AtomicBool = AtomicBool::new(false);
pub static LAST_STATUS: AtomicI32 = AtomicI32::new(0);
pub static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
pub static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
pub static RELOAD_CONFIG: AtomicBool = AtomicBool::new(false);
pub static TRAP_SIGNAL: AtomicI32 = AtomicI32::new(0);
pub static BACKGROUND_PID: AtomicI32 = AtomicI32::new(0);
pub static NEED_REAP: AtomicBool = AtomicBool::new(false);

pub static SIGQUIT_ACTION: AtomicU32 = AtomicU32::new(0);
pub static SIGINT_ACTION: AtomicU32 = AtomicU32::new(0);
pub static SIGTSTP_ACTION: AtomicU32 = AtomicU32::new(0);
pub static SIGWINCH_ACTION: AtomicU32 = AtomicU32::new(0);
pub static SIGUSR1_ACTION: AtomicU32 = AtomicU32::new(0);
pub static SIGUSR2_ACTION: AtomicU32 = AtomicU32::new(0);
pub static FORWARD_SIGNALS: AtomicBool = AtomicBool::new(true);

unsafe fn setup_signal(sig: i32, handler: extern "C" fn(i32)) {
    let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
    sa.sa_sigaction = handler as usize;
    sa.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut sa.sa_mask); }
    unsafe { libc::sigaction(sig, &sa, std::ptr::null_mut()); }
}

unsafe fn ignore_signal(sig: i32) {
    let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
    sa.sa_sigaction = libc::SIG_IGN;
    sa.sa_flags = 0;
    unsafe { libc::sigemptyset(&mut sa.sa_mask); }
    unsafe { libc::sigaction(sig, &sa, std::ptr::null_mut()); }
}

unsafe fn default_signal(sig: i32) {
    let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
    sa.sa_sigaction = libc::SIG_DFL;
    sa.sa_flags = 0;
    unsafe { libc::sigemptyset(&mut sa.sa_mask); }
    unsafe { libc::sigaction(sig, &sa, std::ptr::null_mut()); }
}

pub fn init() {
    unsafe {
        ignore_signal(libc::SIGINT);
        ignore_signal(libc::SIGQUIT);
        ignore_signal(libc::SIGTSTP);
        ignore_signal(libc::SIGTTIN);
        ignore_signal(libc::SIGTTOU);
    }
}

pub fn setup_child_handlers() {
    unsafe {
        default_signal(libc::SIGINT);
        default_signal(libc::SIGQUIT);
        default_signal(libc::SIGTSTP);
        default_signal(libc::SIGTTIN);
        default_signal(libc::SIGTTOU);
        default_signal(libc::SIGCHLD);
        default_signal(libc::SIGTERM);
        default_signal(libc::SIGHUP);
        default_signal(libc::SIGUSR1);
        default_signal(libc::SIGUSR2);
        default_signal(libc::SIGALRM);
        default_signal(libc::SIGPIPE);
    }
}

pub fn setup_parent_handlers(cfg: &SignalsConfig) {
    FORWARD_SIGNALS.store(cfg.forward_signals_to_child, Ordering::SeqCst);

    unsafe {
        SIGINT_ACTION.store(match cfg.sigint_action.as_str() {
            "cancel_line" => 1,
            "ignore" => 0,
            _ => 1,
        }, Ordering::SeqCst);
        match cfg.sigint_action.as_str() {
            "ignore" => ignore_signal(libc::SIGINT),
            _ => setup_signal(libc::SIGINT, handle_sigint),
        }

        SIGQUIT_ACTION.store(match cfg.sigquit_action.as_str() {
            "exit" => 1,
            "suspend" => 2,
            _ => 0,
        }, Ordering::SeqCst);
        match cfg.sigquit_action.as_str() {
            "exit" | "suspend" => setup_signal(libc::SIGQUIT, handle_sigquit),
            _ => ignore_signal(libc::SIGQUIT),
        }

        SIGTSTP_ACTION.store(match cfg.sigtstp_action.as_str() {
            "ignore" => 0,
            _ => 1,
        }, Ordering::SeqCst);
        match cfg.sigtstp_action.as_str() {
            "ignore" => ignore_signal(libc::SIGTSTP),
            _ => setup_signal(libc::SIGTSTP, handle_sigtstp),
        }

        SIGWINCH_ACTION.store(match cfg.sigwinch_action.as_str() {
            "ignore" => 0,
            _ => 1,
        }, Ordering::SeqCst);
        match cfg.sigwinch_action.as_str() {
            "ignore" => ignore_signal(libc::SIGWINCH),
            _ => setup_signal(libc::SIGWINCH, handle_sigwinch),
        }

        ignore_signal(libc::SIGTTIN);
        ignore_signal(libc::SIGTTOU);
        {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = handle_sigchld as *const () as usize;
            sa.sa_flags = 0;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut());
        }
        setup_signal(libc::SIGHUP, handle_sighup);
        setup_signal(libc::SIGTERM, handle_sigterm);
        ignore_signal(libc::SIGPIPE);

        SIGUSR1_ACTION.store(match cfg.sigusr1_action.as_str() {
            "ignore" => 0,
            "run_command" => 2,
            _ => 1,
        }, Ordering::SeqCst);
        if cfg.sigusr1_action.starts_with("run_command:") {
            SIGUSR1_ACTION.store(2, Ordering::SeqCst);
            if let Some(cmd) = cfg.sigusr1_action.strip_prefix("run_command:")
                && let Ok(mut guard) = SIGUSR1_CUSTOM_CMD.lock() {
                    *guard = Some(cmd.to_string());
                }
        }
        match cfg.sigusr1_action.as_str() {
            "ignore" => ignore_signal(libc::SIGUSR1),
            _ => setup_signal(libc::SIGUSR1, handle_sigusr1),
        }

        SIGUSR2_ACTION.store(match cfg.sigusr2_action.as_str() {
            "reload_config" => 1,
            "ignore" => 0,
            "run_command" => 2,
            _ => 0,
        }, Ordering::SeqCst);
        if cfg.sigusr2_action.starts_with("run_command:") {
            SIGUSR2_ACTION.store(2, Ordering::SeqCst);
            if let Some(cmd) = cfg.sigusr2_action.strip_prefix("run_command:")
                && let Ok(mut guard) = SIGUSR2_CUSTOM_CMD.lock() {
                    *guard = Some(cmd.to_string());
                }
        }
        match cfg.sigusr2_action.as_str() {
            "ignore" => ignore_signal(libc::SIGUSR2),
            _ => setup_signal(libc::SIGUSR2, handle_sigusr2),
        }
    }
}

pub fn signal_name_to_number(name: &str) -> Option<i32> {
    let name = name.strip_prefix("SIG").unwrap_or(name).to_uppercase();
    match name.as_str() {
        "HUP" => Some(libc::SIGHUP),
        "INT" => Some(libc::SIGINT),
        "QUIT" => Some(libc::SIGQUIT),
        "ILL" => Some(libc::SIGILL),
        "TRAP" => Some(libc::SIGTRAP),
        "ABRT" => Some(libc::SIGABRT),
        "BUS" => Some(libc::SIGBUS),
        "FPE" => Some(libc::SIGFPE),
        "KILL" => Some(libc::SIGKILL),
        "USR1" => Some(libc::SIGUSR1),
        "SEGV" => Some(libc::SIGSEGV),
        "USR2" => Some(libc::SIGUSR2),
        "PIPE" => Some(libc::SIGPIPE),
        "ALRM" => Some(libc::SIGALRM),
        "TERM" => Some(libc::SIGTERM),
        "STKFLT" => Some(libc::SIGSTKFLT),
        "CHLD" => Some(libc::SIGCHLD),
        "CONT" => Some(libc::SIGCONT),
        "STOP" => Some(libc::SIGSTOP),
        "TSTP" => Some(libc::SIGTSTP),
        "TTIN" => Some(libc::SIGTTIN),
        "TTOU" => Some(libc::SIGTTOU),
        "URG" => Some(libc::SIGURG),
        "XCPU" => Some(libc::SIGXCPU),
        "XFSZ" => Some(libc::SIGXFSZ),
        "VTALRM" => Some(libc::SIGVTALRM),
        "PROF" => Some(libc::SIGPROF),
        "WINCH" => Some(libc::SIGWINCH),
        "IO" => Some(libc::SIGIO),
        "PWR" => Some(libc::SIGPWR),
        "SYS" => Some(libc::SIGSYS),
        _ => {
            name.parse::<i32>().ok()
        }
    }
}

pub fn signal_number_to_name(num: i32) -> Option<&'static str> {
    match num {
        libc::SIGHUP => Some("HUP"),
        libc::SIGINT => Some("INT"),
        libc::SIGQUIT => Some("QUIT"),
        libc::SIGILL => Some("ILL"),
        libc::SIGTRAP => Some("TRAP"),
        libc::SIGABRT => Some("ABRT"),
        libc::SIGBUS => Some("BUS"),
        libc::SIGFPE => Some("FPE"),
        libc::SIGKILL => Some("KILL"),
        libc::SIGUSR1 => Some("USR1"),
        libc::SIGSEGV => Some("SEGV"),
        libc::SIGUSR2 => Some("USR2"),
        libc::SIGPIPE => Some("PIPE"),
        libc::SIGALRM => Some("ALRM"),
        libc::SIGTERM => Some("TERM"),
        libc::SIGSTKFLT => Some("STKFLT"),
        libc::SIGCHLD => Some("CHLD"),
        libc::SIGCONT => Some("CONT"),
        libc::SIGSTOP => Some("STOP"),
        libc::SIGTSTP => Some("TSTP"),
        libc::SIGTTIN => Some("TTIN"),
        libc::SIGTTOU => Some("TTOU"),
        libc::SIGURG => Some("URG"),
        libc::SIGXCPU => Some("XCPU"),
        libc::SIGXFSZ => Some("XFSZ"),
        libc::SIGVTALRM => Some("VTALRM"),
        libc::SIGPROF => Some("PROF"),
        libc::SIGWINCH => Some("WINCH"),
        libc::SIGIO => Some("IO"),
        libc::SIGPWR => Some("PWR"),
        libc::SIGSYS => Some("SYS"),
        _ => None,
    }
}

pub fn set_foreground(_pid: i32) {
    unsafe {
        let shell_pgid = libc::getpgrp();
        libc::tcsetpgrp(libc::STDIN_FILENO, shell_pgid);
        RUNNING.store(false, Ordering::SeqCst);
        CHILD_PID.store(0, Ordering::SeqCst);
    }
}

extern "C" fn handle_sigint(_sig: i32) {
    if SIGINT_ACTION.load(Ordering::SeqCst) == 0 {
        return;
    }
    TRAP_SIGNAL.store(libc::SIGINT, Ordering::SeqCst);
    if RUNNING.load(Ordering::SeqCst) {
        if FORWARD_SIGNALS.load(Ordering::SeqCst) {
            let pid = CHILD_PID.load(Ordering::SeqCst);
            if pid > 0 {
                unsafe { libc::kill(-pid, libc::SIGINT); }
            }
        }
    } else {
        NEED_REDRAW.store(true, Ordering::SeqCst);
    }
}

extern "C" fn handle_sigquit(_sig: i32) {
    match SIGQUIT_ACTION.load(Ordering::SeqCst) {
        1 => {
            TRAP_SIGNAL.store(libc::SIGQUIT, Ordering::SeqCst);
            SHOULD_EXIT.store(true, Ordering::SeqCst);
        }
        2 => {
            unsafe { libc::kill(libc::getpid(), libc::SIGTSTP); }
        }
        _ => {}
    }
}

extern "C" fn handle_sigtstp(_sig: i32) {
    TRAP_SIGNAL.store(libc::SIGTSTP, Ordering::SeqCst);
    if RUNNING.load(Ordering::SeqCst) && FORWARD_SIGNALS.load(Ordering::SeqCst) {
        let pid = CHILD_PID.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe { libc::kill(pid, libc::SIGTSTP); }
        }
    }
}

extern "C" fn handle_sigchld(_sig: i32) {
    NEED_REAP.store(true, Ordering::SeqCst);
}

extern "C" fn handle_sigwinch(_sig: i32) {
    if SIGWINCH_ACTION.load(Ordering::SeqCst) == 1 {
        NEED_REDRAW.store(true, Ordering::SeqCst);
    }
}

extern "C" fn handle_sighup(_sig: i32) {
    TRAP_SIGNAL.store(libc::SIGHUP, Ordering::SeqCst);
    SHOULD_EXIT.store(true, Ordering::SeqCst);
}

extern "C" fn handle_sigterm(_sig: i32) {
    TRAP_SIGNAL.store(libc::SIGTERM, Ordering::SeqCst);
    SHOULD_EXIT.store(true, Ordering::SeqCst);
}

extern "C" fn handle_sigusr1(_sig: i32) {
    if SIGUSR1_ACTION.load(Ordering::SeqCst) != 0 {
        RELOAD_CONFIG.store(true, Ordering::SeqCst);
    }
}

extern "C" fn handle_sigusr2(_sig: i32) {
    if SIGUSR2_ACTION.load(Ordering::SeqCst) != 0 {
        RELOAD_CONFIG.store(true, Ordering::SeqCst);
    }
}

pub fn set_last_status(status: i32) {
    LAST_STATUS.store(status, Ordering::SeqCst);
}

pub fn reap_zombies(auto_report_stopped: bool, stopped_format: &crate::config::schema::MultiLineText) {
    if !NEED_REAP.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut status: i32 = 0;
    loop {
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid <= 0 { break; }
        if auto_report_stopped && libc::WIFSTOPPED(status) {
            let sig = libc::WSTOPSIG(status);
            let msg = stopped_format.expand(&[
                ("pid", &pid.to_string()),
                ("signal", &sig.to_string()),
            ]);
            eprintln!("{}", msg);
        }
    }
}
