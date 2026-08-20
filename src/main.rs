pub mod config;
pub mod shell;
pub mod terminal;

use std::io::{self, Write, BufRead, BufReader};
use std::fs::OpenOptions;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::config::Config;
use crate::shell::env::Env;
use crate::shell::signals;
use crate::terminal::prompt;
use crate::terminal::art;
use crate::terminal::editor;
use crate::terminal::raw::RawGuard;
use crate::terminal::color::set_color_mode;

fn cleanup(history_path: &std::path::Path, history: &[String], known_lines: usize, cfg: &Config, env: &mut Env) {
    env.unset_all_traps();
    if known_lines < history.len() {
        let new_entries = &history[known_lines..];
        let _ = append_history(history_path, new_entries, cfg);
    }
    if cfg.signals.reset_terminal_on_exit {
        let _ = crossterm::terminal::disable_raw_mode();
        print!("\x1b[?25h\x1b[0m");
        let _ = io::stdout().flush();
    }
    if cfg.startup.show_config_path {
        eprintln!("config: {}", config::loader::config_path().display());
    }
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        eprint!("\x1b[?25h\x1b[0m\x1b[2J\x1b[H");
        let _ = io::stderr().flush();
        original(info);
    }));
}

struct CtxReporter;

impl cps::Reporter for CtxReporter {
    fn info(&self, msg: &str) {
        eprintln!("ctx: {msg}");
    }
    fn warning(&self, msg: &str) {
        eprintln!("ctx: {msg}");
    }
    fn error(&self, msg: &str) {
        eprintln!("ctx: {msg}");
    }
}

fn main() {
    install_panic_hook();

    cps::configure(
        cps::Options::new("context").with_reporter(Arc::new(CtxReporter)),
    );

    let args: Vec<String> = std::env::args().collect();
    let opts = CliOptions::parse(&args);

    if opts.help {
        print_help();
        std::process::exit(0);
    }
    if opts.version {
        let cfg = config::loader::load();
        eprintln!("{} {}", cfg.branding.app_name, cfg.branding.version);
        std::process::exit(0);
    }
    if opts.license {
        eprintln!("MIT License");
        std::process::exit(0);
    }
    if opts.authors {
        eprintln!("ctx shell authors");
        std::process::exit(0);
    }
    if opts.verbose_version {
        let cfg = config::loader::load();
        eprintln!("{} {} ({})", cfg.branding.app_name, cfg.branding.version, env!("CARGO_PKG_VERSION"));
        eprintln!("Rust edition 2024, compiled for {}", std::env::consts::ARCH);
        eprintln!("POSIX-compliant shell with modern features");
        std::process::exit(0);
    }
    if opts.print_config {
        let cfg = config::loader::load();
        let json = serde_json::to_string_pretty(&cfg).unwrap_or_default();
        println!("{}", json);
        std::process::exit(0);
    }
    if opts.dump_config {
        let cfg = config::loader::load();
        match toml::to_string_pretty(&cfg) {
            Ok(toml_str) => print!("{}", toml_str),
            Err(e) => eprintln!("ctx: failed to dump config: {}", e),
        }
        std::process::exit(0);
    }
    if opts.show_config_path {
        let path = config::loader::config_dir().join("c.toml");
        println!("{}", path.display());
        std::process::exit(0);
    }
    if opts.show_config_dir {
        println!("{}", config::loader::config_dir().display());
        std::process::exit(0);
    }
    if opts.gen_cfg {
        let cfg = Config::default();
        match config::loader::save(&cfg) {
            Ok(()) => {
                let path = config::loader::config_path();
                eprintln!("ctx: default config written to {}", path.display());
            }
            Err(e) => {
                eprintln!("ctx: failed to write config: {}", e);
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }
    if opts.gen_dycfg {
        let mut cfg = Config::default();
        cfg.dynamic.enabled = true;
        cfg.colors.syntax_use_dynamic_colors = true;
        cfg.colors.input_use_accent_color = true;
        match config::loader::save(&cfg) {
            Ok(()) => {
                let path = config::loader::config_path();
                eprintln!("ctx: dynamic (wallust) config written to {}", path.display());
            }
            Err(e) => {
                eprintln!("ctx: failed to write config: {}", e);
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }

    let mut command_to_run = opts.command.clone();
    let read_from_stdin = opts.stdin;

    if opts.dry_run {
        if let Some(ref cmd) = command_to_run {
            eprintln!("ctx: dry run: {}", cmd);
        } else if read_from_stdin {
            eprintln!("ctx: dry run: reading from stdin");
        } else if let Some(ref file) = opts.file {
            eprintln!("ctx: dry run: {}", file);
        } else {
            eprintln!("ctx: dry run mode");
        }
        std::process::exit(0);
    }

    if let Some(ref file) = opts.file {
        match std::fs::read_to_string(file) {
            Ok(contents) => {
                command_to_run = Some(contents);
            }
            Err(e) => {
                eprintln!("ctx: {}: {}", file, e);
                std::process::exit(1);
            }
        }
    }

    if let Some(ref code) = opts.eval {
        command_to_run = Some(code.clone());
    }

    if read_from_stdin {
        let mut cfg = config::loader::load();
        set_color_mode(&cfg.display.color_mode);
        terminal::dynamic::apply(&mut cfg);
        let env = Env::new();
        let mut executor = shell::executor::Executor::new(env, cfg.clone());

        signals::init();
        signals::setup_parent_handlers(&cfg.signals);

        executor.run_source_rc(opts.rcfile.as_deref());
        executor.run_integrations();

        let stdin = io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut last_status = 0;
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let line = line.trim().to_string();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let tokens = shell::lexer::tokenize(&line);
                    let ast = shell::parser::parse(tokens);
                    last_status = executor.execute(&ast);
                    if signals::SHOULD_EXIT.load(Ordering::SeqCst) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        executor.run_exit_trap();
        executor.env.unset_all_traps();
        std::process::exit(last_status);
    }

    if let Some(cmd) = command_to_run {
        let mut cfg = config::loader::load();
        set_color_mode(&cfg.display.color_mode);
        terminal::dynamic::apply(&mut cfg);
        let env = Env::new();
        let mut executor = shell::executor::Executor::new(env, cfg.clone());

        signals::init();
        signals::setup_parent_handlers(&cfg.signals);

        executor.run_source_rc(opts.rcfile.as_deref());
        executor.run_integrations();

        let tokens = shell::lexer::tokenize(&cmd);
        let ast = shell::parser::parse(tokens);
        let status = executor.execute(&ast);

        executor.run_exit_trap();
        executor.env.unset_all_traps();
        std::process::exit(status);
    }

    let mut cfg = config::loader::load();

    if let Some(ref kv) = opts.env_var
        && let Some((k, v)) = kv.split_once('=') {
            unsafe { std::env::set_var(k, v); }
        }
    if let Some(ref key) = opts.unset_var {
        unsafe { std::env::remove_var(key); }
    }

    if opts.no_color {
        cfg.display.color_mode = "0".into();
    }
    if let Some(ref mode) = opts.color_mode {
        cfg.display.color_mode = mode.clone();
    }
    if opts.posix {
        cfg.editor.expand_aliases = false;
    }
    if opts.restricted {
        cfg.security.restricted_mode = true;
    }
    if opts.interactive {
        cfg.editor.mode = "emacs".into();
    }
    if opts.no_interactive {
        cfg.editor.mode = "emacs".into();
    }
    if opts.norc {
        cfg.startup.run_commands.clear();
    }
    if opts.noprofile {
        cfg.startup.run_commands.clear();
    }
    if opts.no_welcome {
        cfg.startup.show_welcome = false;
    }
    if opts.no_startup {
        cfg.startup.run_commands.clear();
        cfg.startup.show_welcome = false;
    }
    if opts.no_plugins {
        cfg.python.enabled = false;
    }
    if opts.no_python {
        cfg.python.enabled = false;
    }
    if opts.no_integrations {
        cfg.integration.enable_fzf = false;
        cfg.integration.enable_zoxide = false;
    }
    if opts.no_dynamic {
        cfg.dynamic.enabled = false;
    }
    if opts.no_history {
        cfg.history.max_size = 0;
    }
    if let Some(ref f) = opts.history_file {
        cfg.history.file = f.clone();
    }
    if let Some(s) = opts.history_size {
        cfg.history.max_size = s;
    }
    if opts.no_history_share {
        cfg.history.share_across_sessions = false;
    }
    if opts.no_jobs {
        cfg.jobs.max_jobs = 0;
    }
    if opts.monitor {
        cfg.jobs.max_jobs = 128;
    }
    if opts.no_transient_prompt {
        cfg.prompt.transient_prompt = false;
    }
    if opts.instant_prompt {
        cfg.prompt.instant_prompt = true;
    }
    if opts.no_instant_prompt {
        cfg.prompt.instant_prompt = false;
    }
    if opts.no_signals {
        cfg.signals.forward_signals_to_child = false;
    }
    if opts.forward_signals {
        cfg.signals.forward_signals_to_child = true;
    }
    if opts.no_forward_signals {
        cfg.signals.forward_signals_to_child = false;
    }
    if opts.sandbox {
        cfg.security.restricted_mode = true;
        cfg.security.sanitize_path = true;
    }
    if opts.no_exec {
        cfg.security.restricted_mode = true;
    }
    if opts.no_blink {
        cfg.cursor.blink = false;
    }
    if opts.blink {
        cfg.cursor.blink = true;
    }
    if let Some(ref style) = opts.cursor_style {
        cfg.cursor.style = style.clone();
    }
    if let Some(ref name) = opts.app_name {
        cfg.branding.app_name = name.clone();
    }
    if let Some(ref name) = opts.shell_name {
        cfg.branding.shell_name = name.clone();
    }
    if let Some(ref text) = opts.tagline {
        cfg.branding.tagline = text.clone();
    }
    if opts.no_ascii {
        cfg.ascii.lines.clear();
        cfg.ascii.file = String::new();
        cfg.ascii.blocks.clear();
    }
    if opts.ascii {
        cfg.startup.show_welcome = true;
    }
    if let Some(ref path) = opts.chdir {
        let _ = std::env::set_current_dir(path);
    }
    if let Some(ref path) = opts.workdir {
        let _ = std::env::set_current_dir(path);
    }
    if opts.xtrace {
        cfg.editor.colorize_output = true;
    }
    if let Some(ref f) = opts.log_file {
        eprintln!("ctx: logging to {}", f);
    }
    if let Some(ref ch) = opts.prompt_char {
        cfg.symbols.prompt_char = ch.clone();
    }
    if let Some(ref cmd) = opts.run_command {
        cfg.startup.run_commands.push(cmd.clone());
    }
    if let Some(delay) = opts.startup_delay {
        cfg.startup.startup_delay_ms = delay as u32;
    }
    if opts.benchmark {
        eprintln!("ctx: benchmark mode");
    }

    let effective_color_mode = if cfg.modes.color_depth != "true_color" {
        cfg.modes.color_depth.clone()
    } else {
        cfg.display.color_mode.clone()
    };
    set_color_mode(&effective_color_mode);

    terminal::dynamic::apply(&mut cfg);

    if !cfg.colors.bg_primary.is_empty() {
        terminal::color::set_terminal_bg(&cfg.colors.bg_primary);
    }
    if !cfg.colors.primary.is_empty() {
        terminal::color::set_terminal_fg(&cfg.colors.primary);
    }
    if !cfg.cursor.color.is_empty() {
        terminal::color::set_terminal_cursor_color(&cfg.cursor.color);
    }

    let env = Env::new();
    let mut executor = shell::executor::Executor::new(env, cfg.clone());

    for (name, path) in &cfg.named_dirs {
        executor.env.set_named_dir(name, path);
    }

    let prompt_cache = if cfg.prompt.async_prompt {
        Some(prompt::PromptCache::new(cfg.prompt.async_prompt_cache_ms))
    } else {
        None
    };

    if cfg.prompt.instant_prompt {
        let instant_path = config::loader::config_dir().join(".instant_prompt");
        if let Ok(cached) = std::fs::read_to_string(&instant_path) {
            print!("\r\x1b[2K{}", cached);
            let _ = io::stdout().flush();
        }
    }

    signals::init();
    signals::setup_parent_handlers(&cfg.signals);

    executor.run_source_rc(opts.rcfile.as_deref());

    for cmd in &cfg.startup.run_commands {
        let tokens = shell::lexer::tokenize(cmd);
        let ast = shell::parser::parse(tokens);
        executor.execute(&ast);
    }

    executor.run_integrations();

    let py_engine = cps::PythonEngine::new(&cfg.python);
    py_engine.plugins.fire("on_startup", &std::collections::HashMap::new());

    if py_engine.plugins.count() > 0 {
        eprintln!("ctx: loaded {} python plugin(s): {}", py_engine.plugins.count(), py_engine.plugins.names().join(", "));
    }

    if py_engine.tui_mode {
        if let Some(ref theme) = py_engine.theme {
            if theme.has_run() {
                if cfg.startup.show_welcome {
                    print!("{}", art::render_startup(&cfg, &executor.env));
                    let _ = io::stdout().flush();
                    if cfg.startup.startup_delay_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(cfg.startup.startup_delay_ms as u64));
                    }
                }
                let _ = io::stdout().flush();
                let _ = crossterm::terminal::disable_raw_mode();
                print!("\x1b[?25h");
                let _ = io::stdout().flush();
                let tui_ok = theme.run();
                py_engine.plugins.fire("on_exit", &std::collections::HashMap::new());
                std::process::exit(if tui_ok { 0 } else { 1 });
            } else {
                eprintln!("ctx: tui_mode enabled but theme has no run() function");
            }
        } else {
            eprintln!("ctx: tui_mode enabled but no theme loaded");
        }
    }

    if cfg.startup.show_welcome {
        print!("{}", art::render_startup(&cfg, &executor.env));
        let _ = io::stdout().flush();
        if cfg.startup.startup_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(cfg.startup.startup_delay_ms as u64));
        }
    }

    let history_path = config::loader::history_path(&cfg);
    let mut history = load_history_with_expiry(&history_path, cfg.history.expire_days, cfg.performance.max_history_load_lines);
    let mut known_lines = history.len();

    let custom_keybindings = load_keybindings();

    {
        let cfg_leak: &'static Config = Box::leak(Box::new(cfg.clone()));
        let kb = custom_keybindings.clone();
        let _ = shell::builtin::READLINE_CB.set(Box::new(move |prompt: &str| {
            let pd = terminal::prompt::PromptDisplay {
                lines_above: vec![],
                input_prefix: prompt.to_string(),
                lines_below: vec![],
                right_prompt: String::new(),
                right_prompt_color: String::new(),
                right_prompt_hide_threshold: 0.0,
            };
            terminal::editor::read_line_editor(
                &pd,
                &[],
                &cfg_leak.editor,
                &cfg_leak.autosuggest,
                &cfg_leak.history,
                &cfg_leak.clipboard,
                &cfg_leak.cursor,
                &cfg_leak.prompt,
                &cfg_leak.colors,
                &cfg_leak.symbols,
                &kb,
            )
        }));
    }

    let mut running = true;
    while running {
        let trap_sig = signals::TRAP_SIGNAL.swap(0, Ordering::SeqCst);
        if trap_sig != 0 {
            let sig_name = match trap_sig {
                libc::SIGINT => "SIGINT",
                libc::SIGTERM => "SIGTERM",
                libc::SIGHUP => "SIGHUP",
                libc::SIGTSTP => "SIGTSTP",
                libc::SIGUSR1 => "SIGUSR1",
                libc::SIGUSR2 => "SIGUSR2",
                _ => "",
            };
            if !sig_name.is_empty()
                && let Some(cmd) = executor.env.get_trap(sig_name).map(|s| s.to_string())
                    && !cmd.is_empty() {
                        let tokens = shell::lexer::tokenize(&cmd);
                        let ast = shell::parser::parse(tokens);
                        executor.execute(&ast);
                    }
        }

        if signals::SHOULD_EXIT.load(Ordering::SeqCst) {
            break;
        }

        signals::reap_zombies(cfg.jobs.auto_report_stopped, &cfg.jobs.stopped_format);

        if signals::NEED_REDRAW.load(Ordering::SeqCst) {
            signals::NEED_REDRAW.store(false, Ordering::SeqCst);
            print!("\x1b[2J\x1b[H");
            let _ = io::stdout().flush();
        }

        if signals::RELOAD_CONFIG.load(Ordering::SeqCst) {
            signals::RELOAD_CONFIG.store(false, Ordering::SeqCst);
            let sig1_cmd = signals::SIGUSR1_CUSTOM_CMD.lock().ok().and_then(|g| g.clone());
            let sig2_cmd = signals::SIGUSR2_CUSTOM_CMD.lock().ok().and_then(|g| g.clone());
            if let Some(cmd) = sig1_cmd.or(sig2_cmd) {
                if !cmd.is_empty() {
                    let tokens = shell::lexer::tokenize(&cmd);
                    let ast = shell::parser::parse(tokens);
                    executor.execute(&ast);
                }
            } else {
                cfg = config::loader::load();
                executor.cfg = cfg.clone();
            }
        }

        if cfg.history.share_across_sessions {
            sync_history(&history_path, &mut history, &mut known_lines);
        }

        if let Some(cmd) = executor.env.get("PROMPT_COMMAND").map(|s| s.to_string())
            && !cmd.is_empty() {
                let tokens = shell::lexer::tokenize(&cmd);
                let ast = shell::parser::parse(tokens);
                executor.execute(&ast);
            }

        let mut prompt_display = if let Some(ref cache) = prompt_cache {
            cache.get_or_compute(&executor.env, &cfg, executor.last_status)
        } else {
            prompt::render_prompt(&executor.env, &cfg, executor.last_status)
        };

        if let Some(ps1) = executor.env.get("PS1").map(|s| s.to_string()) {
            eprint!("{}", ps1);
            let _ = io::stderr().flush();
            prompt_display.input_prefix = String::new();
            prompt_display.lines_above.clear();
            prompt_display.lines_below.clear();
            prompt_display.right_prompt.clear();
        }

        if let Some(ref theme) = py_engine.theme {
            let mut context = std::collections::HashMap::new();
            if let Some(v) = executor.env.get("PWD") { context.insert("cwd".into(), v.to_string()); }
            if let Some(v) = executor.env.get("USER") { context.insert("user".into(), v.to_string()); }
            if let Some(v) = executor.env.get("HOSTNAME") { context.insert("host".into(), v.to_string()); }
            if let Some(v) = executor.env.get("GIT_BRANCH") { context.insert("git_branch".into(), v.to_string()); }
            context.insert("exit_code".into(), executor.last_status.to_string());
            context.insert("shell_version".into(), cfg.branding.version.clone());
            context.insert("shell_name".into(), cfg.branding.shell_name.clone());
            context.insert("terminal_width".into(), crossterm::terminal::size().map(|(w, _)| w.to_string()).unwrap_or_default());
            context.insert("terminal_height".into(), crossterm::terminal::size().map(|(_, h)| h.to_string()).unwrap_or_default());
            for (k, v) in executor.env.all_vars() {
                context.insert(format!("env_{}", k), v.clone());
            }
            let tw = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
            context.insert("width".into(), tw.to_string());
            let theme_result = theme.render_prompt(&context);
            if !theme_result.lines_above.is_empty() || !theme_result.input_prefix.is_empty() {
                prompt_display.lines_above = theme_result.lines_above;
                prompt_display.input_prefix = theme_result.input_prefix;
            }
            if let Some(color) = theme_result.colors.get("accent") {
                prompt_display.right_prompt_color = color.clone();
            }
            if !theme_result.right_prompt.is_empty() {
                prompt_display.right_prompt = theme_result.right_prompt;
            } else {
                let rp = theme.render_right_prompt(&context);
                if !rp.is_empty() {
                    prompt_display.right_prompt = rp;
                }
            }
        }

        if cfg.display.title_bar {
            let cwd = executor.env.get("PWD").unwrap_or("~");
            let user = executor.env.get("USER").unwrap_or("user");
            let host = executor.env.get("HOSTNAME").unwrap_or("localhost");
            let title = cfg.display.title_bar_format.expand(&[
                ("cwd", cwd),
                ("user", user),
                ("host", host),
            ]);
            print!("\x1b]0;{}\x07", title);
            let _ = io::stdout().flush();
        } else if !cfg.branding.shell_name.is_empty() {
            let cwd = executor.env.get("PWD").unwrap_or("~");
            print!("\x1b]0;{} — {}\x07", cfg.branding.shell_name, cwd);
            let _ = io::stdout().flush();
        }

        if cfg.display.status_line {
            let cwd = executor.env.get("PWD").unwrap_or("~");
            let exit_code = executor.last_status.to_string();
            let line = cfg.display.status_line_format.expand(&[
                ("cwd", cwd),
                ("exit_code", &exit_code),
                ("pid", &std::process::id().to_string()),
            ]);
            eprintln!("{}", line);
            let _ = io::stderr().flush();
        }
        if cfg.display.show_session_info {
            let cwd = executor.env.get("PWD").unwrap_or("~");
            let info = cfg.display.session_info_format.expand(&[
                ("cwd", cwd),
                ("version", &cfg.branding.version),
            ]);
            eprintln!("{}", info);
            let _ = io::stderr().flush();
        }

        if cfg.prompt.instant_prompt {
            let instant_path = config::loader::config_dir().join(".instant_prompt");
            let prompt_text = format!("{}{}", prompt_display.lines_above.join("\n"), prompt_display.input_prefix);
            let _ = std::fs::write(&instant_path, &prompt_text);
        }

        let raw = RawGuard::enable();
        let result = editor::read_line_editor(
            &prompt_display,
            &history,
            &cfg.editor,
            &cfg.autosuggest,
            &cfg.history,
            &cfg.clipboard,
            &cfg.cursor,
            &cfg.prompt,
            &cfg.colors,
            &cfg.symbols,
            &custom_keybindings,
        );
        drop(raw);

        match result {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    if signals::SHOULD_EXIT.load(Ordering::SeqCst) {
                        break;
                    }
                    continue;
                }

                let dominated = !cfg.history.ignore_space || !line.starts_with(' ');
                let dominated = dominated && !cfg.history.ignore_patterns.iter().any(|p| line.contains(p.as_str()));
                if dominated {
                    let dominated = !cfg.history.deduplicate || !history.last().map(|s| s == &line).unwrap_or(false);
                    if dominated {
                        history.push(line.clone());
                        let _ = append_history(&history_path, std::slice::from_ref(&line), &cfg);
                        known_lines += 1;
                        if cfg.history.save_on_every_command {
                            let _ = write_history(&history_path, &history, &cfg);
                        }
                    }
                }

                prompt::set_hist_count(history.len());
                prompt::COMMAND_COUNT.fetch_add(1, Ordering::Relaxed);

                let tokens = shell::lexer::tokenize(&line);
                let ast = shell::parser::parse(tokens);
                let mut event_data = std::collections::HashMap::new();
                event_data.insert("command".into(), line.clone());
                py_engine.plugins.fire("on_preexec", &event_data);
                let cmd_start = std::time::Instant::now();
                if let Some(ps0) = executor.env.get("PS0").map(|s| s.to_string())
                    && !ps0.is_empty() {
                        let bg_pid = signals::BACKGROUND_PID.load(Ordering::SeqCst);
                        let mut expander = shell::expand::Expander::new(&mut executor.env, executor.last_status, vec![], bg_pid);
                        let expanded = expander.expand_word(&ps0);
                        drop(expander);
                        eprint!("{}", expanded);
                        let _ = io::stderr().flush();
                    }
                let status = executor.execute(&ast);
                let cmd_duration = cmd_start.elapsed();

                if cfg.display.show_command_duration && cmd_duration.as_millis() > 0 {
                    executor.env.set("_CMD_DURATION_MS", &cmd_duration.as_millis().to_string());
                }

                if cfg.history.sync_on_command && cfg.history.share_across_sessions {
                    sync_history(&history_path, &mut history, &mut known_lines);
                }

                if executor.clear_history {
                    executor.clear_history = false;
                    history.clear();
                    known_lines = 0;
                }

                signals::set_last_status(status);
                executor.last_status = status;

                let mut event_data = std::collections::HashMap::new();
                event_data.insert("command".into(), line.clone());
                event_data.insert("exit_code".into(), status.to_string());
                event_data.insert("duration_ms".into(), cmd_duration.as_millis().to_string());
                py_engine.plugins.fire("on_postexec", &event_data);

                py_engine.plugins.fire("on_precmd", &std::collections::HashMap::new());

                if let Some(ref cache) = prompt_cache {
                    cache.invalidate();
                }

                if cfg.prompt.transient_prompt {
                    let transient = prompt::render_transient_prompt(&executor.env, &cfg, status);
                    print!("\r\x1b[2K{}", transient);
                    let _ = io::stdout().flush();
                }

                if cfg.display.show_command_summary {
                    let cmd_str = line.trim();
                    let summary = if let Some(ref theme) = py_engine.theme {
                        let mut context = std::collections::HashMap::new();
                        context.insert("command".into(), cmd_str.to_string());
                        context.insert("exit_code".into(), status.to_string());
                        context.insert("duration_ms".into(), cmd_duration.as_millis().to_string());
                        context.insert("cwd".into(), executor.env.get("PWD").unwrap_or("~").to_string());
                        for (k, v) in executor.env.all_vars() {
                            context.insert(format!("env_{}", k), v.clone());
                        }
                        theme.render_command_summary(&context)
                    } else {
                        cfg.display.command_summary_format.expand(&[
                            ("status_char", if status == 0 { &cfg.symbols.success_char } else if (147..=150).contains(&status) { &cfg.symbols.warning_char } else { &cfg.symbols.error_char }),
                            ("command", cmd_str),
                            ("exit_code", &status.to_string()),
                            ("status", if status == 0 { "ok" } else { "error" }),
                        ])
                    };
                    let color = if status == 0 { &cfg.colors.success } else { &cfg.colors.err };
                    eprintln!("{}{}{}", crate::terminal::color::hex_to_ansi(color), summary, crate::terminal::color::reset());
                }

                if signals::SHOULD_EXIT.load(Ordering::SeqCst) {
                    break;
                }
            }
            Err(e) => {
                eprintln!("{}: read error: {}", cfg.branding.app_name, e);
                running = false;
            }
        }
    }

    py_engine.plugins.fire("on_exit", &std::collections::HashMap::new());
    executor.run_exit_trap();
    cleanup(&history_path, &history, known_lines, &cfg, &mut executor.env);
}

fn load_history(path: &std::path::Path, max_lines: u32) -> Vec<String> {
    match OpenOptions::new().read(true).open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines()
                .map_while(Result::ok)
                .filter(|l| !l.is_empty())
                .collect();
            let max = max_lines as usize;
            if max > 0 && lines.len() > max {
                lines[lines.len() - max..].to_vec()
            } else {
                lines
            }
        }
        Err(_) => Vec::new(),
    }
}

fn load_history_with_expiry(path: &std::path::Path, expire_days: u32, max_lines: u32) -> Vec<String> {
    let mut entries = load_history(path, max_lines);
    if expire_days > 0
        && let Ok(meta) = std::fs::metadata(path)
            && let Ok(modified) = meta.modified() {
                let elapsed = modified.elapsed().unwrap_or_default();
                let max_age = std::time::Duration::from_secs(expire_days as u64 * 86400);
                if elapsed > max_age {
                    let keep = entries.len() / 2;
                    if keep > 0 {
                        entries = entries[entries.len() - keep..].to_vec();
                    }
                }
            }
    entries
}

fn write_history(path: &std::path::Path, history: &[String], cfg: &Config) -> io::Result<()> {
    let max = cfg.history.max_size as usize;
    let entries = if history.len() > max {
        &history[history.len() - max..]
    } else {
        history
    };

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;

    unsafe {
        libc::flock(file.as_raw_fd() as libc::c_int, libc::LOCK_EX);
    }

    let mut writer = io::BufWriter::new(&file);
    for entry in entries {
        writeln!(writer, "{}", entry)?;
    }
    writer.flush()?;

    unsafe {
        libc::flock(file.as_raw_fd() as libc::c_int, libc::LOCK_UN);
    }

    Ok(())
}

fn sync_history(path: &std::path::Path, history: &mut Vec<String>, known_lines: &mut usize) {
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let reader = BufReader::new(file);
    let mut line_count = 0usize;
    for line in reader.lines() {
        line_count += 1;
        if line_count > *known_lines
            && let Ok(entry) = line
                && !entry.is_empty() && history.last().map(|s| s != &entry).unwrap_or(true) {
                    history.push(entry);
                }
    }
    *known_lines = line_count;
}

fn append_history(path: &std::path::Path, history: &[String], _cfg: &Config) -> io::Result<()> {
    if let Some(entry) = history.last() {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        unsafe {
            libc::flock(file.as_raw_fd() as libc::c_int, libc::LOCK_EX);
        }

        let mut writer = io::BufWriter::new(&file);
        writeln!(writer, "{}", entry)?;
        writer.flush()?;

        unsafe {
            libc::flock(file.as_raw_fd() as libc::c_int, libc::LOCK_UN);
        }
    }

    Ok(())
}

use std::os::unix::io::AsRawFd;

fn load_keybindings() -> std::collections::HashMap<String, String> {
    let mut bindings = std::collections::HashMap::new();
    let bindings_dir = config::loader::config_dir().join("keybindings");
    if let Ok(entries) = std::fs::read_dir(&bindings_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && let Ok(widget) = std::fs::read_to_string(entry.path()) {
                    let widget = widget.trim().to_string();
                    let key = name.replace('_', " ");
                    bindings.insert(key, widget);
                }
        }
    }
    bindings
}

struct CliOptions {
    help: bool,
    version: bool,
    license: bool,
    authors: bool,
    verbose_version: bool,
    command: Option<String>,
    stdin: bool,
    file: Option<String>,
    eval: Option<String>,
    env_var: Option<String>,
    unset_var: Option<String>,
    chdir: Option<String>,
    workdir: Option<String>,
    rcfile: Option<String>,
    norc: bool,
    noprofile: bool,
    posix: bool,
    restricted: bool,
    interactive: bool,
    no_interactive: bool,
    bash_compat: bool,
    quiet: bool,
    verbose: bool,
    debug: bool,
    trace: bool,
    xtrace: bool,
    log_file: Option<String>,
    no_log: bool,
    print_config: bool,
    dump_config: bool,
    show_config_path: bool,
    show_config_dir: bool,
    no_color: bool,
    color: bool,
    color_mode: Option<String>,
    utf8: bool,
    no_utf8: bool,
    width: Option<u32>,
    height: Option<u32>,
    raw: bool,
    no_raw: bool,
    no_history: bool,
    history_file: Option<String>,
    history_size: Option<u32>,
    no_history_share: bool,
    no_prompt: bool,
    prompt_format: Option<String>,
    no_transient_prompt: bool,
    instant_prompt: bool,
    no_instant_prompt: bool,
    prompt_char: Option<String>,
    no_jobs: bool,
    monitor: bool,
    no_signals: bool,
    forward_signals: bool,
    no_forward_signals: bool,
    sandbox: bool,
    no_sandbox: bool,
    no_exec: bool,
    dry_run: bool,
    no_dynamic: bool,
    no_plugins: bool,
    no_python: bool,
    no_integrations: bool,
    no_startup: bool,
    no_welcome: bool,
    startup_delay: Option<u64>,
    run_command: Option<String>,
    no_blink: bool,
    blink: bool,
    cursor_style: Option<String>,
    app_name: Option<String>,
    shell_name: Option<String>,
    tagline: Option<String>,
    no_ascii: bool,
    ascii: bool,
    config_file: Option<String>,
    no_config: bool,
    benchmark: bool,
    gen_cfg: bool,
    gen_dycfg: bool,
}

impl CliOptions {
    fn parse(args: &[String]) -> Self {
        let mut opts = CliOptions {
            help: false,
            version: false,
            license: false,
            authors: false,
            verbose_version: false,
            command: None,
            stdin: false,
            file: None,
            eval: None,
            env_var: None,
            unset_var: None,
            chdir: None,
            workdir: None,
            rcfile: None,
            norc: false,
            noprofile: false,
            posix: false,
            restricted: false,
            interactive: false,
            no_interactive: false,
            bash_compat: false,
            quiet: false,
            verbose: false,
            debug: false,
            trace: false,
            xtrace: false,
            log_file: None,
            no_log: false,
            print_config: false,
            dump_config: false,
            show_config_path: false,
            show_config_dir: false,
            no_color: false,
            color: false,
            color_mode: None,
            utf8: false,
            no_utf8: false,
            width: None,
            height: None,
            raw: false,
            no_raw: false,
            no_history: false,
            history_file: None,
            history_size: None,
            no_history_share: false,
            no_prompt: false,
            prompt_format: None,
            no_transient_prompt: false,
            instant_prompt: false,
            no_instant_prompt: false,
            prompt_char: None,
            no_jobs: false,
            monitor: false,
            no_signals: false,
            forward_signals: false,
            no_forward_signals: false,
            sandbox: false,
            no_sandbox: false,
            no_exec: false,
            dry_run: false,
            no_dynamic: false,
            no_plugins: false,
            no_python: false,
            no_integrations: false,
            no_startup: false,
            no_welcome: false,
            startup_delay: None,
            run_command: None,
            no_blink: false,
            blink: false,
            cursor_style: None,
            app_name: None,
            shell_name: None,
            tagline: None,
            no_ascii: false,
            ascii: false,
            config_file: None,
            no_config: false,
            benchmark: false,
            gen_cfg: false,
            gen_dycfg: false,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => opts.help = true,
                "-v" | "--version" => opts.version = true,
                "-l" | "--license" => opts.license = true,
                "-a" | "--authors" => opts.authors = true,
                "-V" | "--verbose" => opts.verbose_version = true,
                "-c" | "--command" => {
                    i += 1;
                    opts.command = args.get(i).cloned();
                }
                "-s" | "--stdin" => opts.stdin = true,
                "-f" | "--file" => {
                    i += 1;
                    opts.file = args.get(i).cloned();
                }
                "-e" | "--eval" => {
                    i += 1;
                    opts.eval = args.get(i).cloned();
                }
                "-E" | "--env" => {
                    i += 1;
                    opts.env_var = args.get(i).cloned();
                }
                "-U" | "--unset" => {
                    i += 1;
                    opts.unset_var = args.get(i).cloned();
                }
                "-C" | "--chdir" => {
                    i += 1;
                    opts.chdir = args.get(i).cloned();
                }
                "-W" | "--workdir" => {
                    i += 1;
                    opts.workdir = args.get(i).cloned();
                }
                "-n" | "--norc" => opts.norc = true,
                "--rcfile" => {
                    i += 1;
                    opts.rcfile = args.get(i).cloned();
                }
                "-N" | "--noprofile" => opts.noprofile = true,
                "-p" | "--posix" => opts.posix = true,
                "-r" | "--restricted" => opts.restricted = true,
                "-i" | "--interactive" => opts.interactive = true,
                "-I" | "--no-interactive" => opts.no_interactive = true,
                "-b" | "--bash" => opts.bash_compat = true,
                "-q" | "--quiet" => opts.quiet = true,
                "-Q" => opts.verbose = true,
                "-d" | "--debug" => opts.debug = true,
                "-t" | "--trace" => opts.trace = true,
                "-x" | "--xtrace" => opts.xtrace = true,
                "-g" | "--log-file" => {
                    i += 1;
                    opts.log_file = args.get(i).cloned();
                }
                "-G" | "--gen-cfg" => opts.gen_cfg = true,
                "-X" | "--gen-dycfg" => opts.gen_dycfg = true,
                "-o" | "--print-config" => opts.print_config = true,
                "-O" | "--dump-config" => opts.dump_config = true,
                "-P" | "--show-config-path" => opts.show_config_path = true,
                "-D" | "--show-config-dir" => opts.show_config_dir = true,
                "-M" | "--no-color" => opts.no_color = true,
                "-F" | "--color" => opts.color = true,
                "-R" | "--color-mode" => {
                    i += 1;
                    opts.color_mode = args.get(i).cloned();
                }
                "-u" | "--utf8" => opts.utf8 = true,
                "--no-utf8" => opts.no_utf8 = true,
                "-w" | "--width" => {
                    i += 1;
                    opts.width = args.get(i).and_then(|s| s.parse().ok());
                }
                "-j" | "--height" => {
                    i += 1;
                    opts.height = args.get(i).and_then(|s| s.parse().ok());
                }
                "-J" | "--raw" => opts.raw = true,
                "--no-raw" => opts.no_raw = true,
                "-y" | "--no-history" => opts.no_history = true,
                "-Y" | "--history-file" => {
                    i += 1;
                    opts.history_file = args.get(i).cloned();
                }
                "-z" | "--history-size" => {
                    i += 1;
                    opts.history_size = args.get(i).and_then(|s| s.parse().ok());
                }
                "-Z" | "--no-history-share" => opts.no_history_share = true,
                "--no-prompt" => opts.no_prompt = true,
                "-T" | "--prompt-format" => {
                    i += 1;
                    opts.prompt_format = args.get(i).cloned();
                }
                "--no-transient-prompt" => opts.no_transient_prompt = true,
                "-L" | "--instant-prompt" => opts.instant_prompt = true,
                "--no-instant-prompt" => opts.no_instant_prompt = true,
                "-A" | "--prompt-char" => {
                    i += 1;
                    opts.prompt_char = args.get(i).cloned();
                }
                "-k" | "--no-jobs" => opts.no_jobs = true,
                "-K" | "--monitor" => opts.monitor = true,
                "--no-signals" => opts.no_signals = true,
                "-S" | "--forward-signals" => opts.forward_signals = true,
                "--no-forward-signals" => opts.no_forward_signals = true,
                "-B" | "--sandbox" => opts.sandbox = true,
                "--no-sandbox" => opts.no_sandbox = true,
                "--no-exec" => opts.no_exec = true,
                "--dry-run" => opts.dry_run = true,
                "--no-dynamic" => opts.no_dynamic = true,
                "--no-plugins" => opts.no_plugins = true,
                "--no-python" => opts.no_python = true,
                "--no-integrations" => opts.no_integrations = true,
                "--no-startup" => opts.no_startup = true,
                "--no-welcome" => opts.no_welcome = true,
                "-H" | "--startup-delay" => {
                    i += 1;
                    opts.startup_delay = args.get(i).and_then(|s| s.parse().ok());
                }
                "--run-command" => {
                    i += 1;
                    opts.run_command = args.get(i).cloned();
                }
                "--no-blink" => opts.no_blink = true,
                "--blink" => opts.blink = true,
                "--cursor-style" => {
                    i += 1;
                    opts.cursor_style = args.get(i).cloned();
                }
                "--app-name" => {
                    i += 1;
                    opts.app_name = args.get(i).cloned();
                }
                "--shell-name" => {
                    i += 1;
                    opts.shell_name = args.get(i).cloned();
                }
                "--tagline" => {
                    i += 1;
                    opts.tagline = args.get(i).cloned();
                }
                "--no-ascii" => opts.no_ascii = true,
                "--ascii" => opts.ascii = true,
                "--config-file" => {
                    i += 1;
                    opts.config_file = args.get(i).cloned();
                }
                "--no-config" => opts.no_config = true,
                "--benchmark" => opts.benchmark = true,
                "--no-log" => opts.no_log = true,
                other => {
                    eprintln!("ctx: unknown option: {}", other);
                    eprintln!("Try 'ctx --help' for more information.");
                    std::process::exit(1);
                }
            }
            i += 1;
        }
        opts
    }
}

fn print_help() {
    eprintln!("Usage: ctx [OPTIONS] [COMMAND]");
    eprintln!();
    eprintln!("Info:");
    eprintln!("  -h, --help               Show this help message");
    eprintln!("  -v, --version            Show version number");
    eprintln!("  -V, --verbose            Show detailed version and build info");
    eprintln!("  -l, --license            Show license information");
    eprintln!("  -a, --authors            Show authors");
    eprintln!();
    eprintln!("Execution:");
    eprintln!("  -c, --command [CMD]          Execute CMD as a command string, then exit");
    eprintln!("  -s, --stdin                  Read commands from standard input, then exit");
    eprintln!("  -f, --file [FILE]            Read and execute commands from a file, then exit");
    eprintln!("  -e, --eval [CODE]            Evaluate code as a shell command, then exit");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  -E, --env [KEY]=[VALUE]      Set environment variable key to its value");
    eprintln!("  -U, --unset [KEY]            Remove environment variable's key");
    eprintln!("  -C, --chdir [DIR]            Change to DIR before executing commands");
    eprintln!("  -W, --workdir [DIR]          Set working directory to DIR");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  -p, --posix              Run in POSIX mode");
    eprintln!("  -r, --restricted         Run in restricted mode (no cd, no export, etc.)");
    eprintln!("  -i, --interactive        Force interactive mode");
    eprintln!("  -I, --no-interactive     Force non-interactive mode");
    eprintln!("  -b, --bash               Enable bash compatibility");
    eprintln!();
    eprintln!("Startup:");
    eprintln!("  -n, --norc                 Don't read the rc file (default: ~/.config/ctx/c.toml)");
    eprintln!("  -N, --noprofile            Don't read profile or startup scripts");
    eprintln!("      --no-startup           Skip all startup scripts and run commands");
    eprintln!("      --no-welcome           Suppress the welcome message / ASCII art");
    eprintln!("      --no-integrations      Skip loading fzf/zoxide integrations");
    eprintln!("      --no-plugins           Skip loading Python plugins");
    eprintln!("      --no-python            Skip Python plugin subsystem entirely");
    eprintln!("      --no-dynamic           Disable dynamic wallpaper colors (wallust)");
    eprintln!("      --no-ascii             Suppress ASCII art on startup");
    eprintln!("      --ascii                Force ASCII art on startup");
    eprintln!("      --run-command [CMD]    Run a command after rc loading, before prompt");
    eprintln!("  -H, --startup-delay [MS]   Delay milliseconds before first prompt");
    eprintln!();
    eprintln!("Output & Debug:");
    eprintln!("  -q, --quiet              Suppress informational output");
    eprintln!("  -Q, --verbose            Enable verbose output");
    eprintln!("  -d, --debug              Enable debug mode with extra diagnostics");
    eprintln!("  -t, --trace              Enable execution tracing");
    eprintln!("  -x, --xtrace             Enable command tracing (like set -x)");
    eprintln!("  -g, --log-file [FILE]    Write log output to a file");
    eprintln!("      --no-log             Disable logging");
    eprintln!("      --benchmark          Run in benchmark mode");
    eprintln!();
    eprintln!("Terminal:");
    eprintln!("  -M, --no-color               Disable all color output");
    eprintln!("  -F, --color                  Force color output even when not a TTY");
    eprintln!("  -R, --color-mode [MODE]      Set color mode: true_color, 256, 16, 0");
    eprintln!("  -u, --utf8                   Enable full UTF-8 character support");
    eprintln!("      --no-utf8                Disable UTF-8, use ASCII only");
    eprintln!("  -w, --width [COLS]           Override detected terminal width");
    eprintln!("  -j, --height [ROWS]          Override detected terminal height");
    eprintln!("  -J, --raw                    Enable raw terminal mode");
    eprintln!("      --no-raw                 Disable raw terminal mode");
    eprintln!("      --cursor-style [STYLE]   Set cursor style: block, beam, underline");
    eprintln!("      --no-blink               Disable cursor blinking");
    eprintln!("      --blink                  Enable cursor blinking");
    eprintln!();
    eprintln!("History:");
    eprintln!("  -y, --no-history             Disable history recording entirely");
    eprintln!("  -Y, --history-file [FILE]    Use a file as the history file");
    eprintln!("  -z, --history-size [SIZE]    Set max history size for entries");
    eprintln!("  -Z, --no-history-share       Don't share history across sessions");
    eprintln!();
    eprintln!("Prompting:");
    eprintln!("      --no-prompt              Disable the interactive prompt");
    eprintln!("  -T, --prompt-format [FMT]    Override prompt format string");
    eprintln!("  -A, --prompt-char [CHAR]     Override prompt character (default: ❯)");
    eprintln!("      --no-transient-prompt    Disable transient prompt rewriting");
    eprintln!("  -L, --instant-prompt         Enable instant prompt from cache");
    eprintln!("      --no-instant-prompt      Disable instant prompt");
    eprintln!();
    eprintln!("Jobs Control:");
    eprintln!("  -k, --no-jobs            Disable job control");
    eprintln!("  -K, --monitor            Enable job monitoring (like set -m)");
    eprintln!();
    eprintln!("Signals:");
    eprintln!("      --no-signals            Disable signal handling");
    eprintln!("  -S, --forward-signals       Forward signals to child processes");
    eprintln!("      --no-forward-signals    Don't forward signals to children");
    eprintln!();
    eprintln!("Security:");
    eprintln!("  -B, --sandbox            Enable sandbox mode");
    eprintln!("      --no-sandbox         Disable sandbox mode");
    eprintln!("      --no-exec            Disable command execution");
    eprintln!("      --dry-run            Dry run mode, don't execute commands");
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  -o, --print-config           Print current config as JSON and exit");
    eprintln!("  -O, --dump-config            Print current config as TOML and exit");
    eprintln!("  -P, --show-config-path       Print config file path and exit");
    eprintln!("  -D, --show-config-dir        Print config directory path and exit");
    eprintln!("      --config-file [FILE]     Use a file as the config file");
    eprintln!("      --no-config              Don't load any config file");
    eprintln!();
    eprintln!("Config Generating:");
    eprintln!("  -G, --gen-cfg            Generate default c.toml config and exit");
    eprintln!("  -X, --gen-dycfg          Generate dynamic-colored c.toml and exit");
    eprintln!();
    eprintln!("Branding:");
    eprintln!("      --app-name [NAME]      Override the application name");
    eprintln!("      --shell-name [NAME]    Override the shell name");
    eprintln!("      --tagline [TEXT]       Override the tagline text");
}
