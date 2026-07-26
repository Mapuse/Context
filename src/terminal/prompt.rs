use crate::config::Config;
use crate::shell::env::Env;
use super::color::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};

fn effective_symbol_mode(cfg: &Config) -> String {
    if cfg.modes.symbol_mode != "unicode" {
        return cfg.modes.symbol_mode.clone();
    }
    cfg.display.utf8_mode.clone()
}

fn adapt_symbol(symbol: &str, mode: &str) -> String {
    if mode == "ascii" {
        match symbol {
            "\u{276f}" | "\u{276C}" | "\u{276E}" => ">".to_string(),
            "\u{2713}" => "+".to_string(),
            "\u{2718}" | "\u{2717}" | "\u{2716}" => "X".to_string(),
            "\u{26A0}" => "!".to_string(),
            "\u{25B8}" | "\u{25E6}" => ">".to_string(),
            "\u{2192}" => "->".to_string(),
            "\u{00B7}" => ".".to_string(),
            "\u{2500}" => "-".to_string(),
            "\u{2502}" => "|".to_string(),
            "\u{256D}" | "\u{256E}" | "\u{2570}" | "\u{256F}" => "+".to_string(),
            "\u{23FF}" => "~".to_string(),
            "\u{2026}" => "...".to_string(),
            "\u{2191}" => "^".to_string(),
            "\u{2193}" => "v".to_string(),
            "\u{2022}" => "*".to_string(),
            _ => symbol.to_string(),
        }
    } else {
        symbol.to_string()
    }
}

pub(crate) struct BorderChars {
    pub corner_tl: String,
    pub corner_tr: String,
    pub corner_bl: String,
    pub corner_br: String,
    pub horizontal: String,
    pub vertical: String,
}

pub(crate) fn resolve_border_chars(cfg: &Config) -> BorderChars {
    let style = if cfg.modes.border_mode != "unicode" {
        cfg.modes.border_mode.as_str()
    } else {
        cfg.box_config.border_style.as_str()
    };
    match style {
        "ascii" => BorderChars {
            corner_tl: "+".into(),
            corner_tr: "+".into(),
            corner_bl: "+".into(),
            corner_br: "+".into(),
            horizontal: "-".into(),
            vertical: "|".into(),
        },
        "double" => BorderChars {
            corner_tl: "\u{2554}".into(),
            corner_tr: "\u{2557}".into(),
            corner_bl: "\u{255a}".into(),
            corner_br: "\u{255d}".into(),
            horizontal: "\u{2550}".into(),
            vertical: "\u{2551}".into(),
        },
        "thick" => BorderChars {
            corner_tl: "\u{250f}".into(),
            corner_tr: "\u{2513}".into(),
            corner_bl: "\u{2517}".into(),
            corner_br: "\u{251b}".into(),
            horizontal: "\u{2501}".into(),
            vertical: "\u{2503}".into(),
        },
        "none" => BorderChars {
            corner_tl: " ".into(),
            corner_tr: " ".into(),
            corner_bl: " ".into(),
            corner_br: " ".into(),
            horizontal: " ".into(),
            vertical: " ".into(),
        },
        _ => BorderChars {
            corner_tl: cfg.box_config.corner_tl.clone(),
            corner_tr: cfg.box_config.corner_tr.clone(),
            corner_bl: cfg.box_config.corner_bl.clone(),
            corner_br: cfg.box_config.corner_br.clone(),
            horizontal: cfg.box_config.horizontal_char.clone(),
            vertical: cfg.box_config.vertical_char.clone(),
        },
    }
}

#[derive(Clone)]
pub struct PromptDisplay {
    pub lines_above: Vec<String>,
    pub input_prefix: String,
    pub lines_below: Vec<String>,
    pub right_prompt: String,
    pub right_prompt_color: String,
    pub right_prompt_hide_threshold: f64,
}

pub fn render_prompt(env: &Env, cfg: &Config, last_status: i32) -> PromptDisplay {
    let compact = cfg.display.compact_mode;
    let mut header = String::new();

    if !compact && cfg.prompt.newline_before_prompt {
        header.push('\n');
    }

    if !compact && cfg.prompt.gap_before_prompt > 0 {
        for _ in 0..cfg.prompt.gap_before_prompt {
            header.push('\n');
        }
    }

    if !compact && cfg.display.title_bar {
        let title = expand_prompt_vars(&cfg.display.title_bar_format.single(), env, cfg, last_status);
        let width = get_terminal_width();
        let separator = adapt_symbol(&"─".repeat(width), &effective_symbol_mode(cfg));
        let color = hex_to_ansi(&cfg.display.title_bar_color);
        header.push_str(&format!("{}{}{}", color, separator, reset()));
        header.push('\n');
        header.push_str(&format!("{}{}{}", color, title, reset()));
        header.push('\n');
        header.push_str(&format!("{}{}{}", color, separator, reset()));
        header.push('\n');
    }

    if !compact && cfg.display.show_session_info {
        let info = expand_prompt_vars(&cfg.display.session_info_format.single(), env, cfg, last_status);
        let color = hex_to_ansi(&cfg.colors.info);
        header.push_str(&format!("{}{}{}\n", color, info, reset()));
    }

    if !compact && cfg.box_config.gap_before > 0 {
        for _ in 0..cfg.box_config.gap_before {
            header.push('\n');
        }
    }

    if !compact && cfg.prompt.show_top_line && !cfg.box_config.show_title_only_when_busy {
        header.push_str(&render_top_line(env, cfg));
        header.push('\n');
        for _ in 0..cfg.prompt.gap_after_top {
            header.push('\n');
        }
    }

    let middle = render_middle(env, cfg);
    if !compact && !middle.is_empty() {
        header.push_str(&middle);
        header.push('\n');
        for _ in 0..cfg.prompt.gap_after_middle {
            header.push('\n');
        }
    }

    if !cfg.cursor.format.is_empty() {
        render_format_prompt(env, cfg, last_status, &header)
    } else {
        let mut prompt_line = header;
        prompt_line.push_str(&render_symbol_line(env, cfg, last_status));
        prompt_line.push(' ');
        if cfg.display.status_line {
            let status = expand_prompt_vars(&cfg.display.status_line_format.single(), env, cfg, last_status);
            let info_color = hex_to_ansi(&cfg.colors.info);
            prompt_line.push('\n');
            prompt_line.push_str(&format!("{}{}{}", info_color, status, reset()));
        }
        if !cfg.prompt.prompt_eol_escape.is_empty() {
            prompt_line.push_str(&interpret_escapes(&cfg.prompt.prompt_eol_escape));
        }
        let rprompt = if cfg.prompt.right_prompt.is_empty() {
            String::new()
        } else {
            let raw = expand_prompt_vars(&cfg.prompt.right_prompt.single(), env, cfg, last_status);
            let colored = if cfg.colors.rprompt_bg.is_empty() {
                raw
            } else {
                format!("{}{}{}", hex_to_ansi_bg(&cfg.colors.rprompt_bg), raw, reset())
            };
            if !cfg.prompt.rprompt_eol_escape.is_empty() {
                format!("{}{}", colored, interpret_escapes(&cfg.prompt.rprompt_eol_escape))
            } else {
                colored
            }
        };
        if !cfg.box_config.bottom_line_char.is_empty() {
            let width = get_terminal_width();
            let color = hex_to_ansi(&cfg.prompt.color_top);
            let bottom = cfg.box_config.bottom_line_char.repeat(width);
            prompt_line.push('\n');
            prompt_line.push_str(&format!("{}{}{}", color, bottom, reset()));
        }
        PromptDisplay {
            lines_above: vec![],
            input_prefix: prompt_line,
            lines_below: {
                let mut below = Vec::new();
                for _ in 0..cfg.box_config.gap_after {
                    below.push(String::new());
                }
                below
            },
            right_prompt: rprompt,
            right_prompt_color: cfg.prompt.right_prompt_color.clone(),
            right_prompt_hide_threshold: cfg.prompt.right_prompt_hide_threshold,
        }
    }
}

fn render_format_prompt(env: &Env, cfg: &Config, last_status: i32, header: &str) -> PromptDisplay {
    let lines = cfg.cursor.format.lines();
    let num_lines = lines.len();
    let prompt_color = if last_status == 0 {
        &cfg.cursor.color
    } else {
        &cfg.cursor.color_error
    };
    let color = hex_to_ansi(prompt_color);

    let input_line_idx = if cfg.cursor.format_input_line < 0 {
        num_lines as i32 - 1
    } else {
        cfg.cursor.format_input_line.min(num_lines as i32 - 1).max(0)
    } as usize;

    let cwd = env.get("PWD").unwrap_or("~").to_string();
    let cwd_short = shorten_cwd(&cwd, &env.home(), cfg.prompt.cwd_max_depth);

    let mut lines_above: Vec<String> = Vec::new();
    let mut input_prefix = String::new();
    let mut lines_below: Vec<String> = Vec::new();

    if !header.is_empty() {
        for h_line in header.lines() {
            lines_above.push(h_line.to_string());
        }
    }

    for (i, fmt_line) in lines.iter().enumerate() {
        let expanded = expand_prompt_vars(fmt_line, env, cfg, last_status);
        let colored = if cfg.cursor.format_colorize {
            format!("{}{}{}", color, expanded, reset())
        } else {
            expanded
        };

        let mut full_line = String::new();

        if i == 0 && header.is_empty() {
            full_line.push_str(&format!("{}{}{}{}",
                bold(),
                hex_to_ansi(&cfg.prompt.color_cwd),
                cwd_short,
                reset(),
            ));
            full_line.push(' ');
        }

        full_line.push_str(&colored);

        let mut padded_line = if cfg.cursor.format_padding > 0 {
            let pad = " ".repeat(cfg.cursor.format_padding as usize);
            match cfg.cursor.format_align.as_str() {
                "right" => format!("{}{}", pad, full_line),
                "center" => {
                    let half = cfg.cursor.format_padding / 2;
                    let p1 = " ".repeat(half as usize);
                    let p2 = " ".repeat((cfg.cursor.format_padding - half) as usize);
                    format!("{}{}{}", p1, full_line, p2)
                }
                _ => format!("{}{}", full_line, pad),
            }
        } else {
            full_line
        };

        if i == input_line_idx {
            padded_line.push(' ');
            input_prefix = padded_line;
        } else if i < input_line_idx {
            lines_above.push(padded_line);
        } else {
            lines_below.push(padded_line);
        }
    }

    if cfg.display.status_line {
        let status = expand_prompt_vars(&cfg.display.status_line_format.single(), env, cfg, last_status);
        let info_color = hex_to_ansi(&cfg.colors.info);
        lines_below.push(format!("{}{}{}", info_color, status, reset()));
    }

    if !cfg.box_config.bottom_line_char.is_empty() {
        let width = get_terminal_width();
        let color = hex_to_ansi(&cfg.prompt.color_top);
        let bottom = cfg.box_config.bottom_line_char.repeat(width);
        lines_below.push(format!("{}{}{}", color, bottom, reset()));
    }

    let rprompt = if cfg.prompt.right_prompt.is_empty() {
        String::new()
    } else {
        let raw = expand_prompt_vars(&cfg.prompt.right_prompt.single(), env, cfg, last_status);
        if !cfg.prompt.rprompt_eol_escape.is_empty() {
            format!("{}{}", raw, interpret_escapes(&cfg.prompt.rprompt_eol_escape))
        } else {
            raw
        }
    };

    if !cfg.prompt.prompt_eol_escape.is_empty() {
        input_prefix.push_str(&interpret_escapes(&cfg.prompt.prompt_eol_escape));
    }

    PromptDisplay {
        lines_above,
        input_prefix,
        lines_below,
        right_prompt: rprompt,
        right_prompt_color: cfg.prompt.right_prompt_color.clone(),
        right_prompt_hide_threshold: cfg.prompt.right_prompt_hide_threshold,
    }
}

fn render_symbol_line(env: &Env, cfg: &Config, last_status: i32) -> String {
    let cwd = env.get("PWD").unwrap_or("~").to_string();
    let cwd_short = shorten_cwd(&cwd, &env.home(), cfg.prompt.cwd_max_depth);

    let prefix = expand_prompt_vars(&cfg.prompt.prompt_prefix.single(), env, cfg, last_status);
    let suffix = expand_prompt_vars(&cfg.prompt.prompt_suffix.single(), env, cfg, last_status);

    let mut line = String::new();

    if !prefix.is_empty() {
        let adapted_prefix = adapt_symbol(&prefix, &effective_symbol_mode(cfg));
        line.push_str(&format!("{}{}{}", hex_to_ansi(&cfg.prompt.color_prompt), adapted_prefix, reset()));
        line.push(' ');
    }

    if cfg.prompt.show_path {
        let segments: Vec<&str> = cwd_short.split('/').filter(|s| !s.is_empty()).collect();
        let num_segments = segments.len().max(1);
        for (i, seg) in segments.iter().enumerate() {
            let t = if num_segments <= 1 { 0.0 } else { i as f64 / (num_segments - 1) as f64 };
            let color = if cfg.colors.cwd_gradient_start.is_empty() || cfg.colors.cwd_gradient_end.is_empty() {
                if cfg.colors.gradient_start.is_empty() || cfg.colors.gradient_end.is_empty() {
                    hex_to_ansi(&cfg.prompt.color_cwd)
                } else if cfg.colors.gradient_mid.is_empty() {
                    gradient_color(&cfg.colors.gradient_start, &cfg.colors.gradient_end, t)
                } else {
                    gradient_color_3(&cfg.colors.gradient_start, &cfg.colors.gradient_mid, &cfg.colors.gradient_end, t)
                }
            } else if cfg.colors.gradient_mid.is_empty() {
                gradient_color(&cfg.colors.cwd_gradient_start, &cfg.colors.cwd_gradient_end, t)
            } else {
                gradient_color_3(&cfg.colors.cwd_gradient_start, &cfg.colors.gradient_mid, &cfg.colors.cwd_gradient_end, t)
            };
            line.push_str(&color);
            line.push_str(bold());
            line.push_str(seg);
            line.push_str(reset());
            if i < segments.len() - 1 {
                let use_nerd = match cfg.modes.nerd_mode.as_str() {
                    "minimal" | "full" => true,
                    "off" => false,
                    _ => cfg.display.nerd_fonts,
                };
                let path_sep = if use_nerd {
                    "\u{f07b}".to_string()
                } else {
                    "/".to_string()
                };
                line.push_str(&format!("{}{}", hex_to_ansi(&cfg.prompt.color_cwd), path_sep));
            }
        }
        if segments.is_empty() {
            line.push_str(&format!("{}{}{}", bold(), cwd_short, reset()));
        }
    }

    if !suffix.is_empty() {
        let adapted_suffix = adapt_symbol(&suffix, &effective_symbol_mode(cfg));
        line.push(' ');
        line.push_str(&format!("{}{}{}", hex_to_ansi(&cfg.prompt.color_cwd), adapted_suffix, reset()));
    }

    line
}

fn render_top_line(env: &Env, cfg: &Config) -> String {
    let left = expand_prompt_vars(&cfg.prompt.top_line_left.single(), env, cfg, 0);
    let right = expand_prompt_vars(&cfg.prompt.top_line_right.single(), env, cfg, 0);
    let border = resolve_border_chars(cfg);
    let title_color = if cfg.box_config.title_color.is_empty() { &cfg.prompt.color_top } else { &cfg.box_config.title_color };

    let term_width = get_terminal_width();
    let left_vis = visible_len(&left);
    let right_vis = visible_len(&right);

    let fill_char = &border.horizontal;
    let fill_count = match cfg.box_config.box_width_mode.as_str() {
        "fit" => (cfg.box_config.padding_left + cfg.box_config.padding_right) as usize,
        "fixed" => {
            let fixed_width = cfg.box_config.min_width as usize;
            if fixed_width > left_vis + right_vis + 2 {
                fixed_width - left_vis - right_vis - 2
            } else {
                2
            }
        }
        _ => {
            if term_width > left_vis + right_vis + 2 {
                term_width - left_vis - right_vis - 2
            } else {
                20
            }
        }
    };
    let fill: String = fill_char.repeat(fill_count);

    let padding = " ".repeat(cfg.box_config.top_line_padding as usize);

    format!(
        "{}{}{}{}{}{}{}{}{}",
        hex_to_ansi(title_color),
        bold(),
        padding,
        left,
        reset(),
        hex_to_ansi(title_color),
        fill,
        right,
        reset(),
    )
}

fn render_middle(env: &Env, cfg: &Config) -> String {
    if !cfg.prompt.show_user_host {
        return String::new();
    }
    let formatted = expand_prompt_vars(&cfg.prompt.user_host_format, env, cfg, 0);
    let user_color = hex_to_ansi(&cfg.prompt.color_user);
    let host_color = hex_to_ansi(&cfg.prompt.color_host);
    let user = env.user();
    let hostname = env.hostname();
    let result = formatted
        .replace(&user, &format!("{}{}{}", user_color, user, reset()))
        .replace(&hostname, &format!("{}{}{}", host_color, hostname, reset()));
    result
}

pub fn shorten_cwd(cwd: &str, home: &str, max_depth: u32) -> String {
    if let Some(rest) = cwd.strip_prefix(home) {
        if rest.is_empty() {
            return "~".into();
        }
        return format!("~{}", rest);
    }
    if max_depth > 0 {
        let parts: Vec<&str> = cwd.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() as u32 > max_depth {
            let skip = parts.len() - max_depth as usize;
            let abbreviated: Vec<&str> = parts[skip..].to_vec();
            return format!("…/{}", abbreviated.join("/"));
        }
    }
    cwd.to_string()
}

pub fn get_terminal_width() -> usize {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0 {
            winsize.ws_col as usize
        } else {
            80
        }
    }
}

fn find_git_branch() -> String {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..5 {
        let head = dir.join(".git/HEAD");
        if let Ok(content) = std::fs::read_to_string(&head) {
            let content = content.trim();
            if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
                return branch.to_string();
            }
            if !content.is_empty() {
                return content[..7].to_string();
            }
        }
        if !dir.pop() {
            break;
        }
    }
    String::new()
}

struct GitStatus {
    dirty: bool,
    staged: u32,
    untracked: u32,
    ahead: u32,
    behind: u32,
}

fn find_git_status() -> GitStatus {
    let mut status = GitStatus {
        dirty: false,
        staged: 0,
        untracked: 0,
        ahead: 0,
        behind: 0,
    };
    let output = match Command::new("git")
        .args(["status", "--porcelain=v1", "-b"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return status,
    };
    for line in output.lines() {
        if line.starts_with("## ") {
            if let Some(pos) = line.find('[') {
                let counts = &line[pos + 1..];
                if let Some(end) = counts.find(']') {
                    for part in counts[..end].split(", ") {
                        if let Some(n) = part.strip_suffix(" ahead") {
                            status.ahead = n.trim().parse().unwrap_or(0);
                        }
                        if let Some(n) = part.strip_suffix(" behind") {
                            status.behind = n.trim().parse().unwrap_or(0);
                        }
                    }
                }
            }
        } else if line.len() >= 2 {
            let index_status = line.as_bytes()[0] as char;
            let worktree_status = line.as_bytes()[1] as char;
            if index_status != ' ' || worktree_status != ' ' {
                status.dirty = true;
            }
            if index_status != ' ' && index_status != '?' {
                status.staged += 1;
            }
            if worktree_status == '?' {
                status.untracked += 1;
            }
        }
    }
    status
}

fn interpret_escapes(s: &str) -> String {
    s.replace("\\033", "\x1b")
        .replace("\\e", "\x1b")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\\", "\\")
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let total_secs = ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}m{}s", mins, secs)
    }
}

pub fn expand_prompt_vars(s: &str, env: &Env, cfg: &Config, last_status: i32) -> String {
    let cwd = env.get("PWD").unwrap_or("~").to_string();
    let cwd_short = shorten_cwd(&cwd, &env.home(), cfg.prompt.cwd_max_depth);
    let cwd_parts: Vec<&str> = cwd.split('/').filter(|s| !s.is_empty()).collect();
    let short_cwd = if cwd_parts.len() <= 2 {
        cwd.clone()
    } else {
        let n = cwd_parts.len();
        format!("…/{}", cwd_parts[n - 2..].join("/"))
    };
    let (time_str, time_full_str, date_str) = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
        let hh = tm.tm_hour;
        let mm = tm.tm_min;
        let ss = tm.tm_sec;
        let mon = tm.tm_mon + 1;
        let day = tm.tm_mday;
        let year = tm.tm_year + 1900;

        let format_time = |fmt: &str| -> String {
            fmt.replace("%H", &format!("{:02}", hh))
                .replace("%M", &format!("{:02}", mm))
                .replace("%S", &format!("{:02}", ss))
                .replace("%Y", &format!("{:04}", year))
                .replace("%m", &format!("{:02}", mon))
                .replace("%d", &format!("{:02}", day))
        };

        (
            format_time(&cfg.display.timestamp_format),
            format!("{:02}:{:02}:{:02}", hh, mm, ss),
            format!("{:04}-{:02}-{:02}", year, mon, day),
        )
    };
    let git_status = find_git_status();
    let git = if cfg.prompt.show_git_branch {
        let branch = find_git_branch();
        if branch.is_empty() || (cfg.prompt.show_branch_only_when_dirty && !git_status.dirty) {
            String::new()
        } else {
            let branch_color = hex_to_ansi(&cfg.prompt.git_branch_color);
            format!("{}{}{}", branch_color, branch, reset())
        }
    } else {
        String::new()
    };
    let git_dirty = if git_status.dirty { cfg.prompt.git_dirty_char.clone() } else { String::new() };
    let git_clean = if !git_status.dirty { cfg.prompt.git_clean_char.clone() } else { String::new() };
    let git_staged = if git_status.staged > 0 { cfg.prompt.git_staged_char.clone() } else { String::new() };
    let git_untracked = if git_status.untracked > 0 { cfg.prompt.git_untracked_char.clone() } else { String::new() };
    let git_ahead = if git_status.ahead > 0 { cfg.prompt.git_ahead_char.clone() } else { String::new() };
    let git_behind = if git_status.behind > 0 { cfg.prompt.git_behind_char.clone() } else { String::new() };
    let jobs = {
        let mut count = 0u32;
        let procs_dir = Path::new("/proc/self/task");
        if let Ok(entries) = std::fs::read_dir(procs_dir) {
            for entry in entries.flatten() {
                let fd_dir = entry.path().join("children");
                if let Ok(children) = std::fs::read_to_string(&fd_dir) {
                    let trimmed = children.trim();
                    if !trimmed.is_empty() {
                        count += trimmed.split_whitespace().count() as u32;
                    }
                }
            }
        }
        count
    };
    let status_str = if last_status == 0 { "ok" } else { "error" };

    let exit_code_str = if cfg.box_config.show_exit_code {
        if cfg.display.show_exit_code_on_error_only && last_status == 0 {
            String::new()
        } else {
            let color = hex_to_ansi(&cfg.box_config.exit_code_color);
            format!("{}{}{}", color, last_status, reset())
        }
    } else {
        String::new()
    };

    let pid_str = if cfg.display.show_pid {
        std::process::id().to_string()
    } else {
        String::new()
    };

    let time_color = hex_to_ansi(&cfg.display.timestamp_color);

    let time_display = if cfg.display.show_timestamp {
        format!("{}{}{}", time_color, time_str, reset())
    } else {
        String::new()
    };

    let time_full_display = if cfg.display.show_timestamp {
        format!("{}{}{}", time_color, time_full_str, reset())
    } else {
        String::new()
    };

    let date_display = if cfg.display.show_timestamp {
        format!("{}{}{}", time_color, date_str, reset())
    } else {
        String::new()
    };

    let use_powerline = match cfg.modes.powerline_mode.as_str() {
        "flat" | "round" | "slanted" | "double" => true,
        "off" => false,
        _ => cfg.display.powerline_symbols,
    };
    let success_char = if use_powerline {
        "\u{2713}".to_string()
    } else {
        cfg.symbols.success_char.clone()
    };
    let warning_char = if use_powerline {
        "\u{26A0}".to_string()
    } else {
        cfg.symbols.warning_char.clone()
    };
    let error_char = if use_powerline {
        "\u{2718}".to_string()
    } else {
        cfg.symbols.error_char.clone()
    };
    let status_char = if last_status == 0 {
        success_char.clone()
    } else if (147..=150).contains(&last_status) {
        warning_char.clone()
    } else {
        error_char.clone()
    };
    let arrow_char = if use_powerline {
        "\u{e0b0}".to_string()
    } else {
        cfg.symbols.arrow_char.clone()
    };
    let separator_char = if use_powerline {
        "\u{e0b0}".to_string()
    } else {
        cfg.symbols.separator_char.clone()
    };

    let use_nerd = match cfg.modes.nerd_mode.as_str() {
        "minimal" | "full" => true,
        "off" => false,
        _ => cfg.display.nerd_fonts,
    };
    let git_branch_char = if use_nerd {
        "\u{e0a0}".to_string()
    } else {
        cfg.symbols.git_branch_char.clone()
    };
    let directory_char = if use_nerd {
        "\u{f07b}".to_string()
    } else {
        cfg.symbols.directory_char.clone()
    };
    let file_char = if use_nerd {
        "\u{f15b}".to_string()
    } else {
        cfg.symbols.file_char.clone()
    };
    let executable_char = if use_nerd {
        "\u{f0e7}".to_string()
    } else {
        cfg.symbols.executable_char.clone()
    };
    let link_char = if use_nerd {
        "\u{f0c1}".to_string()
    } else {
        cfg.symbols.link_char.clone()
    };
    let pipe_char = if use_nerd {
        "\u{253c}".to_string()
    } else {
        cfg.symbols.pipe_char.clone()
    };
    let socket_char = if use_nerd {
        "\u{f1e6}".to_string()
    } else {
        cfg.symbols.socket_char.clone()
    };

    let python_venv = if cfg.prompt.show_python_venv {
        std::env::var("VIRTUAL_ENV").ok()
            .map(|v| std::path::Path::new(&v).file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let node_version = if cfg.prompt.show_node_version {
        Command::new("node")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let rust_version = if cfg.prompt.show_rust_version {
        Command::new("rustc")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let duration_str = if cfg.display.show_command_duration {
        env.get("_CMD_DURATION_MS")
            .map(|ms| {
                let ms: u64 = ms.parse().unwrap_or(0);
                let formatted = format_duration(ms);
                let color = hex_to_ansi(&cfg.display.duration_color);
                format!("{}{}{}", color, formatted, reset())
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    s.replace("{cwd}", &cwd_short)
        .replace("{short_cwd}", &short_cwd)
        .replace("{user}", &env.user())
        .replace("{host}", &env.hostname())
        .replace("{exit_code}", &exit_code_str)
        .replace("{pid}", &pid_str)
        .replace("{time}", &time_display)
        .replace("{time_full}", &time_full_display)
        .replace("{date}", &date_display)
        .replace("{git}", &git)
        .replace("{jobs}", &jobs.to_string())
        .replace("{status}", status_str)
        .replace("{status_char}", &status_char)
        .replace("{success_char}", &success_char)
        .replace("{warning_char}", &warning_char)
        .replace("{error_char}", &error_char)
        .replace("{arrow_char}", &arrow_char)
        .replace("{separator_char}", &separator_char)
        .replace("{exit_prefix}", &cfg.symbols.exit_prefix)
        .replace("{git_branch_char}", &git_branch_char)
        .replace("{directory_char}", &directory_char)
        .replace("{file_char}", &file_char)
        .replace("{executable_char}", &executable_char)
        .replace("{link_char}", &link_char)
        .replace("{pipe_char}", &pipe_char)
        .replace("{socket_char}", &socket_char)
        .replace("{python_venv}", &python_venv)
        .replace("{node_version}", &node_version)
        .replace("{rust_version}", &rust_version)
        .replace("{git_dirty_char}", &git_dirty)
        .replace("{git_clean_char}", &git_clean)
        .replace("{git_staged_char}", &git_staged)
        .replace("{git_untracked_char}", &git_untracked)
        .replace("{git_ahead_char}", &git_ahead)
        .replace("{git_behind_char}", &git_behind)
        .replace("{note_char}", &cfg.symbols.note_char)
        .replace("{continuation_char}", &cfg.symbols.continuation_char)
        .replace("{job_char}", &cfg.symbols.job_char)
        .replace("{prompt_char}", &cfg.symbols.prompt_char)
        .replace("{exit_label}", &cfg.box_config.exit_label)
        .replace("{duration}", &duration_str)
        .replace("{accent}", &cfg.colors.accent)
        .replace("{bg_err}", &cfg.colors.bg_err)
        .replace("{bg_info}", &cfg.colors.bg_info)
        .replace("{bg_primary}", &cfg.colors.bg_primary)
        .replace("{bg_success}", &cfg.colors.bg_success)
        .replace("{bg_warning}", &cfg.colors.bg_warning)
        .replace("{exit_code_color}", &cfg.box_config.exit_code_color)
        .replace("{app_name}", &cfg.branding.app_name)
        .replace("{tagline}", &cfg.branding.tagline)
        .replace("{version}", &cfg.branding.version)
        .replace("{author}", &cfg.branding.author)
        .replace("{config_path}", &crate::config::loader::config_path().display().to_string())
        .replace("{shell_name}", &cfg.branding.shell_name)
        .replace("{terminal_width}", &get_terminal_width().to_string())
}

pub fn render_transient_prompt(env: &Env, cfg: &Config, last_status: i32) -> String {
    let color = if cfg.colors.transient.is_empty() {
        String::new()
    } else {
        hex_to_ansi(&cfg.colors.transient)
    };
    let text = expand_prompt_vars(&cfg.prompt.transient_prompt_format.single(), env, cfg, last_status);
    if color.is_empty() {
        text
    } else {
        format!("{}{}{}", color, text, reset())
    }
}

pub struct PromptCache {
    cached_prompt: Arc<Mutex<Option<(PromptDisplay, Instant, i32)>>>,
    cache_duration: Duration,
}

impl PromptCache {
    pub fn new(cache_ms: u64) -> Self {
        Self {
            cached_prompt: Arc::new(Mutex::new(None)),
            cache_duration: Duration::from_millis(cache_ms),
        }
    }

    pub fn get_or_compute(&self, env: &Env, cfg: &Config, last_status: i32) -> PromptDisplay {
        {
            let cache = self.cached_prompt.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((ref prompt, time, cached_status)) = *cache {
                if time.elapsed() < self.cache_duration && cached_status == last_status {
                    return prompt.clone();
                }
            }
        }
        let prompt = render_prompt(env, cfg, last_status);
        let mut cache = self.cached_prompt.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some((prompt.clone(), Instant::now(), last_status));
        prompt
    }

    pub fn invalidate(&self) {
        let mut cache = self.cached_prompt.lock().unwrap_or_else(|e| e.into_inner());
        *cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_cwd_home() {
        let result = shorten_cwd("/home/user", "/home/user", 0);
        assert_eq!(result, "~");
    }

    #[test]
    fn test_shorten_cwd_subdir() {
        let result = shorten_cwd("/home/user/projects/foo", "/home/user", 0);
        assert_eq!(result, "~/projects/foo");
    }

    #[test]
    fn test_shorten_cwd_max_depth() {
        let result = shorten_cwd("/system/local/bin/zsh", "/home/user", 2);
        assert_eq!(result, "…/bin/zsh");
    }
}
