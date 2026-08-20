use std::sync::atomic::{AtomicU8, Ordering};

pub static COLOR_MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_color_mode(mode: &str) {
    let m = match mode {
        "true_color" | "24bit" => 0u8,
        "256" => 1,
        "16" => 2,
        "none" => 3,
        "auto" | "" => {
            let cap = detect_color_capability();
            match cap {
                ColorCapability::TrueColor => 0,
                ColorCapability::Color256 => 1,
                ColorCapability::Color16 => 2,
                ColorCapability::NoColor => 3,
            }
        }
        _ => 0,
    };
    COLOR_MODE.store(m, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorCapability {
    TrueColor,
    Color256,
    Color16,
    NoColor,
}

pub fn detect_color_capability() -> ColorCapability {
    if let Ok(ct) = std::env::var("COLORTERM")
        && (ct == "truecolor" || ct == "24bit") {
            return ColorCapability::TrueColor;
        }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("256color") { return ColorCapability::Color256; }
        if term == "dumb" || term.is_empty() { return ColorCapability::NoColor; }
        return ColorCapability::Color16;
    }
    ColorCapability::Color16
}

fn hex_to_ansi_256(hex: &str) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return String::new();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);

    let ri = ((r as u32 * 5 + 127) / 255) as u8;
    let gi = ((g as u32 * 5 + 127) / 255) as u8;
    let bi = ((b as u32 * 5 + 127) / 255) as u8;
    let idx = 16 + 36 * ri + 6 * gi + bi;
    format!("\x1b[38;5;{}m", idx)
}

fn hex_to_ansi_16(hex: &str) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return String::new();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);

    let mut idx = 30;
    if r > 128 { idx += 1; }
    if g > 128 { idx += 2; }
    if b > 128 { idx += 4; }
    if idx == 30 && (r > 30 || g > 30 || b > 30) {
        idx = 37;
    }
    format!("\x1b[{}m", idx)
}

pub fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return (0, 0, 0);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
}

pub fn hex_to_ansi(hex: &str) -> String {
    match COLOR_MODE.load(Ordering::Relaxed) {
        0 => {
            let hex = hex.trim_start_matches('#');
            if hex.len() != 6 {
                return String::new();
            }
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            format!("\x1b[38;2;{};{};{}m", r, g, b)
        }
        1 => hex_to_ansi_256(hex),
        2 => hex_to_ansi_16(hex),
        _ => String::new(),
    }
}

pub fn hex_to_ansi_bg(hex: &str) -> String {
    match COLOR_MODE.load(Ordering::Relaxed) {
        0 => {
            let (r, g, b) = parse_hex_rgb(hex);
            format!("\x1b[48;2;{};{};{}m", r, g, b)
        }
        1 => {
            let hex = hex.trim_start_matches('#');
            if hex.len() != 6 {
                return String::new();
            }
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let ri = ((r as u32 * 5 + 127) / 255) as u8;
            let gi = ((g as u32 * 5 + 127) / 255) as u8;
            let bi = ((b as u32 * 5 + 127) / 255) as u8;
            let idx = 16 + 36 * ri + 6 * gi + bi;
            format!("\x1b[48;5;{}m", idx)
        }
        2 => {
            let (r, g, b) = parse_hex_rgb(hex);
            let mut idx = 40;
            if r > 128 { idx += 1; }
            if g > 128 { idx += 2; }
            if b > 128 { idx += 4; }
            if idx == 40 && (r > 30 || g > 30 || b > 30) {
                idx = 47;
            }
            format!("\x1b[{}m", idx)
        }
        _ => String::new(),
    }
}

pub fn gradient_color(start_hex: &str, end_hex: &str, t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    let (sr, sg, sb) = parse_hex_rgb(start_hex);
    let (er, eg, eb) = parse_hex_rgb(end_hex);
    let r = (sr as f64 + (er as f64 - sr as f64) * t) as u8;
    let g = (sg as f64 + (eg as f64 - sg as f64) * t) as u8;
    let b = (sb as f64 + (eb as f64 - sb as f64) * t) as u8;
    match COLOR_MODE.load(Ordering::Relaxed) {
        0 => format!("\x1b[38;2;{};{};{}m", r, g, b),
        1 => {
            let ri = ((r as u32 * 5 + 127) / 255) as u8;
            let gi = ((g as u32 * 5 + 127) / 255) as u8;
            let bi = ((b as u32 * 5 + 127) / 255) as u8;
            let idx = 16 + 36 * ri + 6 * gi + bi;
            format!("\x1b[38;5;{}m", idx)
        }
        2 => {
            let mut idx = 30;
            if r > 128 { idx += 1; }
            if g > 128 { idx += 2; }
            if b > 128 { idx += 4; }
            if idx == 30 && (r > 30 || g > 30 || b > 30) {
                idx = 37;
            }
            format!("\x1b[{}m", idx)
        }
        _ => String::new(),
    }
}

pub fn gradient_color_3(start_hex: &str, mid_hex: &str, end_hex: &str, t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        gradient_color(start_hex, mid_hex, t * 2.0)
    } else {
        gradient_color(mid_hex, end_hex, (t - 0.5) * 2.0)
    }
}

pub fn reset() -> &'static str {
    "\x1b[0m"
}

pub fn set_terminal_bg(hex: &str) {
    if hex.is_empty() { return; }
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return; }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    print!("\x1b]11;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x07", r, r, g, g, b, b);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

pub fn set_terminal_fg(hex: &str) {
    if hex.is_empty() { return; }
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return; }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    print!("\x1b]10;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x07", r, r, g, g, b, b);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

pub fn set_terminal_cursor_color(hex: &str) {
    if hex.is_empty() { return; }
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return; }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    print!("\x1b]12;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x07", r, r, g, g, b, b);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

pub fn bold() -> &'static str {
    "\x1b[1m"
}

pub fn italic() -> &'static str {
    "\x1b[3m"
}

pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(next) = chars.next() {
                if next == '[' {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() { break; }
                    }
                } else if next == ']' {
                    let mut prev = ']';
                    for c in chars.by_ref() {
                        if c == '\x07' {
                            break;
                        }
                        if prev == '\x1b' && c == '\\' {
                            break;
                        }
                        prev = c;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_wide_char(cp: u32) -> bool {
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0xFF00..=0xFFEF).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
}

pub fn visible_len(text: &str) -> usize {
    let mut len = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(next) = chars.next() {
                if next == '[' {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() { break; }
                    }
                } else if next == ']' {
                    let mut prev = ']';
                    for c in chars.by_ref() {
                        if c == '\x07' { break; }
                        if prev == '\x1b' && c == '\\' { break; }
                        prev = c;
                    }
                }
            }
        } else {
            let cp = c as u32;
            len += if is_wide_char(cp) { 2 } else { 1 };
        }
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_ansi() {
        COLOR_MODE.store(0, Ordering::Relaxed);
        assert_eq!(hex_to_ansi("#ff0000"), "\x1b[38;2;255;0;0m");
        assert_eq!(hex_to_ansi("00ff00"), "\x1b[38;2;0;255;0m");
    }

    #[test]
    fn test_hex_to_ansi_256() {
        let result = hex_to_ansi_256("#ff0000");
        assert!(result.starts_with("\x1b[38;5;"));
        assert!(result.ends_with('m'));
    }

    #[test]
    fn test_hex_to_ansi_16() {
        let result = hex_to_ansi_16("#ff0000");
        assert!(result.starts_with("\x1b["));
        assert!(result.ends_with('m'));
    }

    #[test]
    fn test_hex_to_ansi_nocolor() {
        COLOR_MODE.store(3, Ordering::Relaxed);
        let result = hex_to_ansi("#ff0000");
        assert_eq!(result, "");
        COLOR_MODE.store(0, Ordering::Relaxed);
    }

    #[test]
    fn test_strip_ansi() {
        let result = strip_ansi("\x1b[38;2;255;0;0mhello\x1b[0m");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_strip_ansi_cursor() {
        let result = strip_ansi("\x1b[2Ahello\x1b[3C");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_strip_ansi_erase() {
        let result = strip_ansi("\x1b[2Khello\x1b[2J");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_visible_len() {
        COLOR_MODE.store(0, Ordering::Relaxed);
        let colored = format!("{}hello{}", hex_to_ansi("#ff0000"), reset());
        assert_eq!(visible_len(&colored), 5);
    }

    #[test]
    fn test_detect_color_capability() {
        let cap = detect_color_capability();
        assert!(matches!(cap, ColorCapability::TrueColor | ColorCapability::Color256 | ColorCapability::Color16 | ColorCapability::NoColor));
    }

    #[test]
    fn test_gradient_color() {
        COLOR_MODE.store(0, Ordering::Relaxed);
        let start = gradient_color("#000000", "#ffffff", 0.0);
        assert_eq!(start, "\x1b[38;2;0;0;0m");
        let mid = gradient_color("#000000", "#ffffff", 0.5);
        assert_eq!(mid, "\x1b[38;2;127;127;127m");
        let end = gradient_color("#000000", "#ffffff", 1.0);
        assert_eq!(end, "\x1b[38;2;255;255;255m");
    }

    #[test]
    fn test_hex_to_ansi_bg() {
        COLOR_MODE.store(0, Ordering::Relaxed);
        let result = hex_to_ansi_bg("#ff0000");
        assert_eq!(result, "\x1b[48;2;255;0;0m");
        COLOR_MODE.store(3, Ordering::Relaxed);
        assert_eq!(hex_to_ansi_bg("#ff0000"), "");
        COLOR_MODE.store(0, Ordering::Relaxed);
    }
}
