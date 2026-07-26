use std::io::Write;
use crate::config::Config;
use crate::shell::env::Env;
use crate::terminal::color::*;
use super::prompt::{resolve_border_chars, expand_prompt_vars};

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}

fn has_content(cfg: &Config) -> bool {
    !cfg.ascii.lines.is_empty()
        || !cfg.ascii.file.is_empty()
        || !cfg.ascii.blocks.is_empty()
}

pub fn render_ascii_art(cfg: &Config) -> String {
    if !has_content(cfg) {
        return String::new();
    }

    let art_text = if !cfg.ascii.lines.is_empty() {
        cfg.ascii.lines.join("\n")
    } else if !cfg.ascii.file.is_empty() {
        let expanded = expand_tilde(&cfg.ascii.file);
        match std::fs::read_to_string(&expanded) {
            Ok(content) => content,
            Err(_) => {
                return format!("{}{}[ascii art not found: {}]{}",
                    hex_to_ansi(&cfg.colors.dim),
                    italic(),
                    cfg.ascii.file,
                    reset(),
                );
            }
        }
    } else {
        return String::new();
    };

    let color_override = if cfg.ascii.color.is_empty() {
        None
    } else {
        Some(hex_to_ansi(&cfg.ascii.color))
    };

    let has_block_colors = !cfg.ascii.blocks.is_empty();

    let mut result = String::new();

    for _ in 0..cfg.ascii.margin_top {
        result.push('\n');
    }

    let lines: Vec<&str> = art_text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let color = if let Some(ref c) = color_override {
            c.clone()
        } else if has_block_colors {
            cfg.ascii.blocks.get(i)
                .map(|b| hex_to_ansi(&b.color))
                .unwrap_or_else(|| hex_to_ansi(&cfg.ascii.box_color))
        } else if cfg.ascii.box_color.is_empty() {
            hex_to_ansi(&cfg.colors.primary)
        } else {
            hex_to_ansi(&cfg.ascii.box_color)
        };

        if cfg.ascii.center {
            let width = get_terminal_width();
            let line_len = visible_len(line);
            let padding = if width > line_len { (width - line_len) / 2 } else { 0 };
            result.push_str(&format!("{}{}{}{}", " ".repeat(padding), color, line, reset()));
        } else {
            let margin_left = " ".repeat(cfg.ascii.margin_left as usize);
            result.push_str(&format!("{}{}{}{}", margin_left, color, line, reset()));
        }
        if i < lines.len() - 1 {
            result.push('\n');
        }
    }

    for _ in 0..cfg.ascii.margin_bottom {
        result.push('\n');
    }

    result
}

pub fn render_welcome(cfg: &Config, env: &Env) -> String {
    let mut result = String::new();

    if has_content(cfg) {
        if cfg.ascii.show_in_box {
            let art = render_ascii_art(cfg);
            let art_lines: Vec<&str> = art.lines().collect();
            let border = resolve_border_chars(cfg);
            let width = get_terminal_width().max(cfg.box_config.min_width as usize);
            let border_color = hex_to_ansi(&cfg.box_config.border_color);
            let content_color = hex_to_ansi(&cfg.box_config.content_color);

            result.push_str(&format!("{}{}{}", border_color, border.corner_tl, reset()));
            let fill: String = border.horizontal.repeat(width.saturating_sub(2));
            result.push_str(&format!("{}{}{}", border_color, fill, reset()));
            result.push_str(&format!("{}{}{}", border_color, border.corner_tr, reset()));
            result.push('\n');

            for line in &art_lines {
                let line_vis = visible_len(line);
                let inner = width.saturating_sub(2);
                let pad_right = inner.saturating_sub(line_vis);
                result.push_str(&format!("{}{}{}{}{}{}{}",
                    border_color, border.vertical, reset(),
                    content_color, line, reset(),
                    " ".repeat(pad_right),
                ));
                result.push_str(&format!("{}{}{}", border_color, border.vertical, reset()));
                result.push('\n');
            }

            result.push_str(&format!("{}{}{}", border_color, border.corner_bl, reset()));
            result.push_str(&format!("{}{}{}", border_color, fill, reset()));
            result.push_str(&format!("{}{}{}", border_color, border.corner_br, reset()));
            result.push('\n');
        } else {
            result.push_str(&render_ascii_art(cfg));
        }
    }

    if !cfg.branding.app_name.is_empty() || !cfg.branding.tagline.is_empty() {
        let name_color = hex_to_ansi(&cfg.colors.primary);
        let tag_color = hex_to_ansi(&cfg.colors.dim);
        if !cfg.branding.app_name.is_empty() {
            result.push_str(&format!("{}{}{}", name_color, cfg.branding.app_name, reset()));
        }
        if !cfg.branding.tagline.is_empty() {
            if !cfg.branding.app_name.is_empty() {
                result.push(' ');
            }
            result.push_str(&format!("{}{}", tag_color, cfg.branding.tagline));
        }
        result.push_str(reset());
        result.push('\n');
    }

    if !cfg.startup.welcome_message.is_empty() {
        let msg_color = hex_to_ansi(&cfg.startup.welcome_color);
        for line in cfg.startup.welcome_message.lines() {
            let expanded = expand_prompt_vars(line, env, cfg, 0);
            result.push_str(&format!("{}{}{}", msg_color, expanded, reset()));
            result.push('\n');
        }
    }

    if cfg.branding.show_version_on_start {
        result.push_str(&format!("{}{}v{}{}", hex_to_ansi(&cfg.colors.dim), italic(), cfg.branding.version, reset()));
        result.push('\n');
    }

    if cfg.branding.show_config_path_on_start {
        let config_path = crate::config::loader::config_path();
        result.push_str(&format!("{}config: {}{}", hex_to_ansi(&cfg.colors.dim), config_path.display(), reset()));
        result.push('\n');
    }

    if !cfg.branding.author.is_empty() {
        result.push_str(&format!("{}by {}{}", hex_to_ansi(&cfg.colors.dim), cfg.branding.author, reset()));
        result.push('\n');
    }

    result
}

fn get_terminal_width() -> usize {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0 {
            winsize.ws_col as usize
        } else {
            80
        }
    }
}

pub fn render_startup(cfg: &Config, env: &Env) -> String {
    if !has_content(cfg) {
        return String::new();
    }

    if cfg.ascii.animate && !cfg.ascii.file.is_empty() {
        let expanded = expand_tilde(&cfg.ascii.file);
        if let Ok(content) = std::fs::read_to_string(&expanded) {
            let frames = split_frames(&content);
            if frames.len() > 1 {
                let delay = std::time::Duration::from_millis(cfg.ascii.animate_delay_ms as u64);
                let total_height = art_height(cfg, &frames[0]);
                for frame in &frames {
                    let rendered = render_frame(cfg, frame);
                    print!("{}", rendered);
                    let _ = std::io::stdout().flush();
                    std::thread::sleep(delay);
                    if frame != frames.last().unwrap() {
                        print!("\x1b[{}A", total_height);
                        let _ = std::io::stdout().flush();
                    }
                }
                return String::new();
            }
        }
    }

    render_welcome(cfg, env)
}

fn split_frames(content: &str) -> Vec<String> {
    let mut frames = Vec::new();
    let mut current = String::new();
    let mut last_was_blank = false;

    for line in content.lines() {
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                frames.push(std::mem::take(&mut current));
            }
            last_was_blank = true;
        } else {
            if last_was_blank && !current.is_empty() {
                current.push('\n');
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
            last_was_blank = false;
        }
    }
    if !current.trim().is_empty() {
        frames.push(current);
    }
    frames
}

fn art_height(cfg: &Config, content: &str) -> usize {
    content.lines().count() + cfg.ascii.margin_top as usize + cfg.ascii.margin_bottom as usize
}

fn render_frame(cfg: &Config, content: &str) -> String {
    let color_override = if cfg.ascii.color.is_empty() {
        None
    } else {
        Some(hex_to_ansi(&cfg.ascii.color))
    };
    let has_block_colors = !cfg.ascii.blocks.is_empty();
    let mut result = String::new();

    for _ in 0..cfg.ascii.margin_top {
        result.push('\n');
    }

    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let color = if let Some(ref c) = color_override {
            c.clone()
        } else if has_block_colors {
            cfg.ascii.blocks.get(i)
                .map(|b| hex_to_ansi(&b.color))
                .unwrap_or_else(|| hex_to_ansi(&cfg.ascii.box_color))
        } else if cfg.ascii.box_color.is_empty() {
            hex_to_ansi(&cfg.colors.primary)
        } else {
            hex_to_ansi(&cfg.ascii.box_color)
        };

        if cfg.ascii.center {
            let width = get_terminal_width();
            let line_len = visible_len(line);
            let padding = if width > line_len { (width - line_len) / 2 } else { 0 };
            result.push_str(&format!("{}{}{}{}", " ".repeat(padding), color, line, reset()));
        } else {
            let margin_left = " ".repeat(cfg.ascii.margin_left as usize);
            result.push_str(&format!("{}{}{}{}", margin_left, color, line, reset()));
        }
        if i < lines.len() - 1 {
            result.push('\n');
        }
    }

    for _ in 0..cfg.ascii.margin_bottom {
        result.push('\n');
    }

    result
}
