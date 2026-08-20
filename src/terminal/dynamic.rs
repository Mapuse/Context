use crate::config::schema::Config;
use crate::terminal::color::parse_hex_rgb;
use std::path::Path;
use std::process::Command;

fn expand_tilde(path: &str) -> String {
    if path.starts_with('~')
        && let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    path.to_string()
}

fn myrgb_to_hex(c: &wallust::colors::Myrgb) -> String {
    format!("{}", c)
}

fn detect_wallpaper_path() -> Option<String> {
    if let Ok(o) = Command::new("swww").args(["query"]).output() {
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            if let Some(path) = line.strip_prefix("image: ") {
                let path = expand_tilde(path.trim());
                if std::path::Path::new(&path).exists() {
                    return Some(path);
                }
            }
        }
    }

    if let Ok(o) = Command::new("swaymsg").args(["-t", "get_outputs"]).output()
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout)
            && let Some(path) = v.get(0).and_then(|o| o.get("current_wallpaper")).and_then(|v| v.as_str()) {
                let path = expand_tilde(path);
                if std::path::Path::new(&path).exists() {
                    return Some(path);
                }
            }

    for key in &["picture-uri-dark", "picture-uri"] {
        if let Ok(o) = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.background", key])
            .output()
        {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let path = s
                .trim()
                .trim_start_matches("file://")
                .trim_matches('\'');
            let path = expand_tilde(path);
            if std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let candidates = [
            ".wallpaper",
            ".wallpaper.png",
            ".wallpaper.jpg",
            ".config/wallpaper",
            ".config/wallpaper.png",
            ".config/wallpaper.jpg",
            "Pictures/wallpaper.png",
            "Pictures/wallpaper.jpg",
        ];
        for c in &candidates {
            let p = home.join(c);
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn extract_colors_via_wallust(path: &str) -> Option<Vec<String>> {
    let file = Path::new(path);
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("context");
    let cache_path = cache_dir.join("wallust.json");

    let wallust_cfg = wallust::config::Config::default();

    let colors = wallust::gen_colors(file, &wallust_cfg, false, &cache_path, true, true, false)
        .ok()?;

    let bg = myrgb_to_hex(&colors.background);
    let fg = myrgb_to_hex(&colors.foreground);
    let accent = myrgb_to_hex(&colors.color6);
    let success = myrgb_to_hex(&colors.color2);
    let err = myrgb_to_hex(&colors.color1);
    let warning = myrgb_to_hex(&colors.color3);
    let info = myrgb_to_hex(&colors.color4);
    let dim = darken(&fg, 0.5);

    Some(vec![bg, fg.clone(), accent, fg, success, err, warning, info, dim])
}

fn query_terminal_bg() -> Vec<String> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let _ = std::io::stdout().write_all(b"\x1b]11;?\x07");
    let _ = std::io::stdout().flush();

    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    let mut buf = [0u8; 1024];

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(100) {
        if let Ok(n) = handle.read(&mut buf)
            && n > 0 {
                let s = String::from_utf8_lossy(&buf[..n]);
                if let Some(idx) = s.find("\x1b]11;") {
                    let rest = &s[idx + 5..];
                    if let Some(end) = rest.find('\x07') {
                        let color_str = &rest[..end];
                        return parse_osc_color(color_str);
                    }
                }
            }
    }

    fallback_palette()
}

fn parse_osc_color(s: &str) -> Vec<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("rgb:") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() == 3 {
            let r = u8::from_str_radix(&parts[0][..2.min(parts[0].len())], 16).unwrap_or(0);
            let g = u8::from_str_radix(&parts[1][..2.min(parts[1].len())], 16).unwrap_or(0);
            let b = u8::from_str_radix(&parts[2][..2.min(parts[2].len())], 16).unwrap_or(0);
            let hex = format!("{:02x}{:02x}{:02x}", r, g, b);
            let dark = darken(&hex, 0.7);
            let light = brighten(&hex, 0.3);
            let accent = brighten(&hex, 0.5);
            let dim = darken(&light, 0.5);
            return vec![
                dark,
                light,
                accent,
                hex,
                "#9ece6a".into(),
                "#f7768e".into(),
                "#e0af68".into(),
                "#7dcfff".into(),
                dim,
            ];
        }
    }
    fallback_palette()
}

fn fallback_palette() -> Vec<String> {
    vec![
        "#1a1b26".into(),
        "#c0caf5".into(),
        "#7aa2f7".into(),
        "#7aa2f7".into(),
        "#9ece6a".into(),
        "#f7768e".into(),
        "#e0af68".into(),
        "#7dcfff".into(),
        "#565f89".into(),
    ]
}

fn brighten(hex: &str, amount: f64) -> String {
    let (r, g, b) = parse_hex_rgb(hex);
    let r = (r as f64 + (255.0 - r as f64) * amount).min(255.0) as u8;
    let g = (g as f64 + (255.0 - g as f64) * amount).min(255.0) as u8;
    let b = (b as f64 + (255.0 - b as f64) * amount).min(255.0) as u8;
    format!("{:02x}{:02x}{:02x}", r, g, b)
}

fn darken(hex: &str, amount: f64) -> String {
    let (r, g, b) = parse_hex_rgb(hex);
    let r = (r as f64 * (1.0 - amount)).max(0.0) as u8;
    let g = (g as f64 * (1.0 - amount)).max(0.0) as u8;
    let b = (b as f64 * (1.0 - amount)).max(0.0) as u8;
    format!("{:02x}{:02x}{:02x}", r, g, b)
}

pub fn apply(cfg: &mut Config) {
    if !cfg.dynamic.enabled {
        return;
    }

    let wallpaper_path = cfg
        .dynamic
        .wallpaper_path
        .as_deref()
        .map(|p| {
            let expanded = expand_tilde(p);
            if std::path::Path::new(&expanded).exists() {
                Some(expanded)
            } else {
                None
            }
        })
        .unwrap_or_else(detect_wallpaper_path);

    let colors = match wallpaper_path {
        Some(path) => extract_colors_via_wallust(&path),
        None => None,
    };

    let colors = match colors {
        Some(c) => c,
        None => {
            if cfg.dynamic.fallback_to_terminal_bg {
                query_terminal_bg()
            } else {
                return;
            }
        }
    };

    if colors.len() < 8 {
        return;
    }

    let mapping = &cfg.dynamic.color_mapping;
    let apply = |field: &str, idx: usize| -> String {
        mapping.get(field).cloned().unwrap_or_else(|| colors[idx].clone())
    };

    cfg.colors.bg_primary = apply("bg_primary", 0);
    cfg.colors.primary = apply("primary", 1);
    cfg.colors.accent = apply("accent", 2);
    cfg.colors.text = apply("text", 1);
    cfg.colors.success = apply("success", 4);
    cfg.colors.err = apply("err", 5);
    cfg.colors.warning = apply("warning", 6);
    cfg.colors.info = apply("info", 7);
    cfg.colors.dim = apply("dim", 8);

    let err_base = if cfg.colors.err.len() >= 7 { cfg.colors.err[1..5].to_string() } else { "aaff".into() };
    let success_base = if cfg.colors.success.len() >= 7 { cfg.colors.success[1..5].to_string() } else { "aaff".into() };
    let warning_base = if cfg.colors.warning.len() >= 7 { cfg.colors.warning[1..5].to_string() } else { "aaff".into() };
    let info_base = if cfg.colors.info.len() >= 7 { cfg.colors.info[1..5].to_string() } else { "aaff".into() };

    cfg.colors.bg_err = format!("#{}00", err_base);
    cfg.colors.bg_success = format!("#{}00", success_base);
    cfg.colors.bg_warning = format!("#{}00", warning_base);
    cfg.colors.bg_info = format!("#{}00", info_base);

    cfg.prompt.color_cwd = cfg.colors.accent.clone();
    cfg.prompt.color_prompt = cfg.colors.success.clone();
    cfg.prompt.color_top = cfg.colors.primary.clone();
    cfg.prompt.color_symbol = cfg.colors.success.clone();
    cfg.prompt.color_user = cfg.colors.accent.clone();
    cfg.prompt.color_host = cfg.colors.info.clone();
    cfg.prompt.git_branch_color = cfg.colors.warning.clone();
    cfg.prompt.error_prompt_color = cfg.colors.err.clone();
    cfg.prompt.prompt_color_success = cfg.colors.success.clone();
    cfg.prompt.vi_cmd_color = cfg.colors.success.clone();
    cfg.prompt.vi_cmd_color_error = cfg.colors.err.clone();
    cfg.prompt.vi_cmd_color_success = cfg.colors.success.clone();

    if cfg.colors.input_use_accent_color {
        cfg.colors.input_color = cfg.colors.accent.clone();
    }

    if cfg.colors.syntax_use_dynamic_colors {
        cfg.colors.syntax_comment = cfg.colors.dim.clone();
        cfg.colors.syntax_string = cfg.colors.warning.clone();
        cfg.colors.syntax_variable = cfg.colors.accent.clone();
        cfg.colors.syntax_operator = cfg.colors.success.clone();
        cfg.colors.syntax_command = cfg.colors.info.clone();
        cfg.colors.syntax_flag = cfg.colors.accent.clone();
        cfg.colors.syntax_path = cfg.colors.warning.clone();
        cfg.colors.syntax_number = cfg.colors.accent.clone();
    }

    cfg.display.duration_color = cfg.colors.dim.clone();
    cfg.display.timestamp_color = cfg.colors.dim.clone();
    cfg.display.title_bar_color = cfg.colors.dim.clone();

    cfg.autosuggest.highlight_color = cfg.colors.dim.clone();

    cfg.box_config.border_color = cfg.colors.primary.clone();
    cfg.box_config.exit_code_color = cfg.colors.err.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brighten_darken() {
        let bright = brighten("333333", 0.5);
        let dark = darken("cccccc", 0.5);
        assert_ne!(bright, "333333");
        assert_ne!(dark, "cccccc");
    }

    #[test]
    fn test_parse_osc_color() {
        let result = parse_osc_color("rgb:ffff/0000/8080");
        assert_eq!(result.len(), 9);
        assert_eq!(result[3], "ff0080");
    }

    #[test]
    fn test_expand_tilde() {
        let result = expand_tilde("~/foo");
        assert!(result.starts_with('/') || !result.starts_with('~'));
    }

    #[test]
    fn test_fallback_palette() {
        let p = fallback_palette();
        assert_eq!(p.len(), 9);
        assert!(p[0].starts_with('#'));
    }
}
