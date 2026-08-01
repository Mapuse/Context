use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use crate::config::schema::SignalsConfig;

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
        setup_signal(libc::SIGCHLD, handle_sigchld);
        setup_signal(libc::SIGHUP, handle_sighup);
        setup_signal(libc::SIGTERM, handle_sigterm);
        ignore_signal(libc::SIGPIPE);

        SIGUSR1_ACTION.store(match cfg.sigusr1_action.as_str() {
            "ignore" => 0,
            _ => 1,
        }, Ordering::SeqCst);
        match cfg.sigusr1_action.as_str() {
            "ignore" => ignore_signal(libc::SIGUSR1),
            _ => setup_signal(libc::SIGUSR1, handle_sigusr1),
        }

        SIGUSR2_ACTION.store(match cfg.sigusr2_action.as_str() {
            "reload_config" => 1,
            _ => 0,
        }, Ordering::SeqCst);
        match cfg.sigusr2_action.as_str() {
            "reload_config" => setup_signal(libc::SIGUSR2, handle_sigusr2),
            _ => ignore_signal(libc::SIGUSR2),
        }
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
                unsafe { libc::kill(pid, libc::SIGINT); }
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
    if SIGUSR1_ACTION.load(Ordering::SeqCst) == 1 {
        RELOAD_CONFIG.store(true, Ordering::SeqCst);
    }
}

extern "C" fn handle_sigusr2(_sig: i32) {
    if SIGUSR2_ACTION.load(Ordering::SeqCst) == 1 {
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
