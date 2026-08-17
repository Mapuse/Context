use std::io::{self, Write};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
};

use super::prompt::PromptDisplay;
use super::color::hex_to_ansi;

fn ps2_display() -> (String, bool) {
    match std::env::var("PS2") {
        Ok(v) => (v, true),
        Err(_) => ("> ".to_string(), false),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum EditorMode {
    Emacs,
    ViInsert,
    ViNormal,
    ViVisual,
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(i, _)| i)
}

fn remove_char_at(s: &mut String, char_idx: usize) {
    let byte = char_to_byte(s, char_idx);
    s.remove(byte);
}

fn insert_char_at(s: &mut String, char_idx: usize, c: char) {
    let byte = char_to_byte(s, char_idx);
    s.insert(byte, c);
}

static KILL_RING: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static UNDO_STACK: LazyLock<Mutex<Vec<(String, usize)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static REDO_STACK: LazyLock<Mutex<Vec<(String, usize)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

use std::borrow::Cow;

struct EditorRenderCtx<'a> {
    prompt: Cow<'a, PromptDisplay>,
    autosuggest_cfg: &'a crate::config::schema::AutosuggestConfig,
    colorize: bool,
    colors: &'a crate::config::schema::ColorsConfig,
}

fn push_undo(input: &str, cursor: usize) {
    UNDO_STACK.lock().expect("UNDO_STACK lock").push((input.to_string(), cursor));
    REDO_STACK.lock().expect("REDO_STACK lock").clear();
}

fn undo(input: &str, cursor: usize) -> Option<(String, usize)> {
    UNDO_STACK.lock().expect("UNDO_STACK lock").pop().inspect(|_prev| {
        REDO_STACK.lock().expect("REDO_STACK lock").push((input.to_string(), cursor));
    })
}

fn redo(input: &str, cursor: usize) -> Option<(String, usize)> {
    REDO_STACK.lock().expect("REDO_STACK lock").pop().inspect(|_prev| {
        UNDO_STACK.lock().expect("UNDO_STACK lock").push((input.to_string(), cursor));
    })
}

fn kill_ring_push(text: &str) {
    if !text.is_empty() {
        let mut ring = KILL_RING.lock().expect("KILL_RING lock");
        if ring.last().map(|s| s.as_str()) != Some(text) {
            ring.push(text.to_string());
        }
    }
}

fn kill_ring_yank() -> Option<String> {
    KILL_RING.lock().expect("KILL_RING lock").last().cloned()
}

fn format_key(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut parts: Vec<String> = Vec::new();
    if modifiers.contains(KeyModifiers::CONTROL) { parts.push("Ctrl".to_string()); }
    if modifiers.contains(KeyModifiers::ALT) { parts.push("Alt".to_string()); }
    if modifiers.contains(KeyModifiers::SHIFT) { parts.push("Shift".to_string()); }
    match code {
        KeyCode::Char(c) => parts.push(c.to_string()),
        KeyCode::Enter => parts.push("Enter".to_string()),
        KeyCode::Tab => parts.push("Tab".to_string()),
        KeyCode::Backspace => parts.push("Backspace".to_string()),
        KeyCode::Delete => parts.push("Delete".to_string()),
        KeyCode::Up => parts.push("Up".to_string()),
        KeyCode::Down => parts.push("Down".to_string()),
        KeyCode::Left => parts.push("Left".to_string()),
        KeyCode::Right => parts.push("Right".to_string()),
        KeyCode::Home => parts.push("Home".to_string()),
        KeyCode::End => parts.push("End".to_string()),
        KeyCode::PageUp => parts.push("PageUp".to_string()),
        KeyCode::PageDown => parts.push("PageDown".to_string()),
        KeyCode::Esc => parts.push("Esc".to_string()),
        _ => parts.push("?".to_string()),
    }
    parts.join("+")
}

fn terminal_bell(bell: &str) {
    match bell {
        "audible" => { print!("\x07"); }
        "visible" => { print!("\x1b[?5h"); print!("\x1b[?5l"); }
        _ => {}
    }
}

fn parse_key_spec(spec: &str) -> (KeyModifiers, KeyCode) {
    let mut mods = KeyModifiers::empty();
    let mut parts: Vec<&str> = spec.split('+').collect();
    for part in &parts {
        match *part {
            "Ctrl" | "Control" => mods |= KeyModifiers::CONTROL,
            "Alt" | "Meta" => mods |= KeyModifiers::ALT,
            "Shift" => mods |= KeyModifiers::SHIFT,
            _ => {}
        }
    }
    parts.retain(|p| !matches!(*p, "Ctrl" | "Control" | "Alt" | "Meta" | "Shift"));
    let key_name = parts.first().copied().unwrap_or("");
    let code = match key_name {
        "Right" => KeyCode::Right,
        "Left" => KeyCode::Left,
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Enter" | "Return" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Esc" | "Escape" => KeyCode::Esc,
        c if c.len() == 1 => KeyCode::Char(c.chars().next().expect("len==1 char")),
        _ => KeyCode::Esc,
    };
    (mods, code)
}

fn matches_key_event(code: KeyCode, modifiers: KeyModifiers, spec: &str) -> bool {
    if spec.is_empty() { return false; }
    let (exp_mods, exp_code) = parse_key_spec(spec);
    let ctrl_ok = !exp_mods.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::CONTROL);
    let alt_ok = !exp_mods.contains(KeyModifiers::ALT) || modifiers.contains(KeyModifiers::ALT);
    let shift_ok = !exp_mods.contains(KeyModifiers::SHIFT) || modifiers.contains(KeyModifiers::SHIFT);
    code == exp_code && ctrl_ok && alt_ok && shift_ok
}

fn vi_cursor_escape(name: &str) -> &str {
    match name {
        "block" => "\x1b[2 q",
        "underline" => "\x1b[4 q",
        "beam" => "\x1b[5 q",
        _ => "\x1b[2 q",
    }
}

fn clipboard_copy(text: &str, cfg: &crate::config::schema::ClipboardConfig) {
    if !cfg.enabled || text.is_empty() { return; }
    let tool = match cfg.method.as_str() {
        "xclip" => Some("xclip"),
        "xsel" => Some("xsel"),
        "pbcopy" => Some("pbcopy"),
        "wl-copy" => Some("wl-copy"),
        _ => {
            ["pbcopy", "xclip", "xsel", "wl-copy"].iter().find(|t| {
                Command::new(t).arg("--version").output().is_ok()
                    || Command::new(t).stdin(std::process::Stdio::null()).spawn().is_ok()
            }).copied()
        }
    };
    if let Some(tool) = tool {
        let mut cmd = match tool {
            "xclip" => {
                let mut c = Command::new("xclip");
                c.args(["-selection", "clipboard"]);
                c
            }
            "xsel" => {
                let mut c = Command::new("xsel");
                c.args(["--clipboard", "--input"]);
                c
            }
            _ => Command::new(tool),
        };
        if let Ok(mut child) = cmd.stdin(std::process::Stdio::piped()).spawn() {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
}

fn highlight_line(input: &str, colorize: bool, colors: &crate::config::schema::ColorsConfig) -> String {
    if !colorize {
        if colors.input_color.is_empty() {
            return input.to_string();
        }
        let input_fg = crate::terminal::color::hex_to_ansi(&colors.input_color);
        let reset = crate::terminal::color::reset();
        return format!("{}{}{}", input_fg, input, reset);
    }
    let comment_color = crate::terminal::color::hex_to_ansi(&colors.syntax_comment);
    let string_color = crate::terminal::color::hex_to_ansi(&colors.syntax_string);
    let variable_color = crate::terminal::color::hex_to_ansi(&colors.syntax_variable);
    let operator_color = crate::terminal::color::hex_to_ansi(&colors.syntax_operator);
    let command_color = crate::terminal::color::hex_to_ansi(&colors.syntax_command);
    let flag_color = crate::terminal::color::hex_to_ansi(&colors.syntax_flag);
    let path_color = crate::terminal::color::hex_to_ansi(&colors.syntax_path);
    let number_color = crate::terminal::color::hex_to_ansi(&colors.syntax_number);
    let input_fg = crate::terminal::color::hex_to_ansi(&colors.input_color);
    let reset = crate::terminal::color::reset();
    let effective_reset = if colors.input_color.is_empty() {
        reset.to_string()
    } else {
        format!("{}{}", reset, input_fg)
    };
    let mut out = String::with_capacity(input.len() * 2);
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    if !colors.input_color.is_empty() {
        out.push_str(&input_fg);
    }

    while i < len {
        match chars[i] {
            '#' if i == 0 || (i > 0 && chars[i-1] == ' ') => {
                out.push_str(&comment_color);
                while i < len { out.push(chars[i]); i += 1; }
                out.push_str(&effective_reset);
            }
            '"' => {
                out.push_str(&string_color);
                out.push(chars[i]); i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len { out.push(chars[i]); i += 1; }
                    out.push(chars[i]); i += 1;
                }
                if i < len { out.push(chars[i]); i += 1; }
                out.push_str(&effective_reset);
            }
            '\'' => {
                out.push_str(&string_color);
                out.push(chars[i]); i += 1;
                while i < len && chars[i] != '\'' { out.push(chars[i]); i += 1; }
                if i < len { out.push(chars[i]); i += 1; }
                out.push_str(&effective_reset);
            }
            '$' => {
                out.push_str(&variable_color);
                out.push(chars[i]); i += 1;
                if i < len {
                    match chars[i] {
                        '{' => {
                            out.push(chars[i]); i += 1;
                            while i < len && chars[i] != '}' { out.push(chars[i]); i += 1; }
                            if i < len { out.push(chars[i]); i += 1; }
                        }
                        '(' => {
                            out.push(chars[i]); i += 1;
                            let mut depth = 1u32;
                            while i < len && depth > 0 {
                                match chars[i] {
                                    '(' => depth += 1,
                                    ')' => { depth -= 1; if depth == 0 { break; } }
                                    _ => {}
                                }
                                out.push(chars[i]); i += 1;
                            }
                            if i < len { out.push(chars[i]); i += 1; }
                        }
                        _ => {
                            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                                out.push(chars[i]); i += 1;
                            }
                        }
                    }
                }
                out.push_str(&effective_reset);
            }
            '|' | '&' if i + 1 < len && chars[i + 1] == chars[i] => {
                out.push_str(&operator_color);
                out.push(chars[i]); out.push(chars[i+1]); i += 2;
                out.push_str(&effective_reset);
            }
            '|' | '&' | ';' | '>' | '<' => {
                out.push_str(&operator_color);
                out.push(chars[i]); i += 1;
                if i < len && (chars[i] == '>' || chars[i] == '&' || chars[i] == '<') {
                    out.push(chars[i]); i += 1;
                }
                out.push_str(&effective_reset);
            }
            '=' | '+' | '-' if i + 1 < len && chars[i + 1] == '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]); i += 1;
                out.push(chars[i]); i += 1;
                out.push_str(&effective_reset);
            }
            '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]); i += 1;
                if i < len && chars[i] == '=' {
                    out.push(chars[i]); i += 1;
                }
                out.push_str(&effective_reset);
            }
            '!' if i + 1 < len && chars[i + 1] == '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]); out.push(chars[i+1]); i += 2;
                out.push_str(&effective_reset);
            }
            '\\' if i + 1 < len => {
                out.push_str(&operator_color);
                out.push(chars[i]); i += 1;
                out.push(chars[i]); i += 1;
                out.push_str(&effective_reset);
            }
            _ => {
                let start = i;
                while i < len {
                    match chars[i] {
                        '"' | '\'' | '$' | '|' | '&' | ';' | '>' | '<' | '#' | '=' | '!' | '+' | '-' | '\\' => break,
                        _ => {}
                    }
                    i += 1;
                }
                if i == start {
                    out.push(chars[i]); i += 1;
                } else if i > start {
                    let word: String = chars[start..i].iter().collect();
                    let is_cmd_pos = start == 0 || (start > 0 && (chars[start-1] == '|' || chars[start-1] == '&' || chars[start-1] == ';' || chars[start-1] == '('));
                    if word.starts_with('-') && word.len() > 1 && word != "--" {
                        out.push_str(&flag_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else if !is_cmd_pos && word.chars().next().is_some_and(|c| c.is_ascii_digit()) && word.chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c == 'x' || c == 'X' || c == 'o' || c == 'O' || c == 'b' || c == 'B') {
                        out.push_str(&number_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else if !is_cmd_pos && word.chars().any(|c| c == '/') && (word.starts_with('/') || word.starts_with("./") || word.starts_with("../") || word.starts_with("~/") || word.contains('/')) {
                        out.push_str(&path_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else if is_cmd_pos {
                        out.push_str(&command_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else {
                        out.push_str(&word);
                    }
                }
            }
        }
    }
    out.push_str(reset);
    out
}

#[allow(clippy::too_many_arguments)]
fn exec_widget(widget: &str, input: &mut String, cursor_pos: &mut usize, history: &[String], history_offset: &mut Option<usize>, temp_buf: &mut String, word_delimiters: &str, clipboard_cfg: &crate::config::schema::ClipboardConfig) {
    let delims: Vec<char> = word_delimiters.chars().collect();
    let is_delim = |c: char| delims.contains(&c);
    match widget {
        "backward-char" => {
            if *cursor_pos > 0 { *cursor_pos -= 1; }
        }
        "forward-char" => {
            if *cursor_pos < input.chars().count() { *cursor_pos += 1; }
        }
        "backward-delete-char" => {
            if *cursor_pos > 0 {
                *cursor_pos -= 1;
                remove_char_at(input, *cursor_pos);
            }
        }
        "delete-char" => {
            if *cursor_pos < input.chars().count() {
                remove_char_at(input, *cursor_pos);
            }
        }
        "backward-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut pos = *cursor_pos;
            while pos > 0 && is_delim(chars[pos - 1]) { pos -= 1; }
            while pos > 0 && !is_delim(chars[pos - 1]) { pos -= 1; }
            *cursor_pos = pos;
        }
        "forward-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut pos = *cursor_pos;
            while pos < chars.len() && is_delim(chars[pos]) { pos += 1; }
            while pos < chars.len() && !is_delim(chars[pos]) { pos += 1; }
            *cursor_pos = pos;
        }
        "beginning-of-line" => { *cursor_pos = 0; }
        "end-of-line" => { *cursor_pos = input.chars().count(); }
        "kill-line" => {
            input.truncate(char_to_byte(input, *cursor_pos));
        }
        "backward-kill-line" => {
            let tail: String = input.chars().skip(*cursor_pos).collect();
            input.clear();
            input.push_str(&tail);
            *cursor_pos = 0;
        }
        "kill-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut end = *cursor_pos;
            while end < chars.len() && is_delim(chars[end]) { end += 1; }
            while end < chars.len() && !is_delim(chars[end]) { end += 1; }
            let killed: String = input.chars().skip(*cursor_pos).take(end - *cursor_pos).collect();
            kill_ring_push(&killed);
            let byte_start = char_to_byte(input, *cursor_pos);
            let byte_end = char_to_byte(input, end);
            input.drain(byte_start..byte_end);
        }
        "backward-kill-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut start = *cursor_pos;
            while start > 0 && is_delim(chars[start - 1]) { start -= 1; }
            while start > 0 && !is_delim(chars[start - 1]) { start -= 1; }
            let killed: String = input.chars().skip(start).take(*cursor_pos - start).collect();
            kill_ring_push(&killed);
            let byte_start = char_to_byte(input, start);
            let byte_end = char_to_byte(input, *cursor_pos);
            input.drain(byte_start..byte_end);
            *cursor_pos = start;
        }
        "yank" => {
            if let Some(yanked) = kill_ring_yank() {
                if clipboard_cfg.yank_to_clipboard {
                    clipboard_copy(&yanked, clipboard_cfg);
                }
                let before: String = input.chars().take(*cursor_pos).collect();
                let after: String = input.chars().skip(*cursor_pos).collect();
                *input = format!("{}{}{}", before, yanked, after);
                *cursor_pos += yanked.chars().count();
            }
        }
        "history-search-backward" => {
            if history_offset.is_none() {
                *temp_buf = input.clone();
                *history_offset = Some(0);
            } else if let Some(ref mut idx) = *history_offset
                && *idx < history.len().saturating_sub(1) { *idx += 1; }
            if let Some(idx) = *history_offset {
                let hi = history.len().saturating_sub(1 + idx);
                if hi < history.len() {
                    *input = history[hi].clone();
                    *cursor_pos = input.chars().count();
                }
            }
        }
        "history-search-forward" => {
            if let Some(idx) = *history_offset {
                if idx > 0 {
                    *history_offset = Some(idx - 1);
                    let hi = history.len().saturating_sub(1 + history_offset.expect("history_offset Some"));
                    *input = history[hi].clone();
                    *cursor_pos = input.chars().count();
                } else {
                    *history_offset = None;
                    *input = temp_buf.clone();
                    *cursor_pos = input.chars().count();
                }
            }
        }
        "undo" => {
            if let Some((prev, prev_cursor)) = undo(input, *cursor_pos) {
                *input = prev;
                *cursor_pos = prev_cursor;
            }
        }
        "redo" => {
            if let Some((next, next_cursor)) = redo(input, *cursor_pos) {
                *input = next;
                *cursor_pos = next_cursor;
            }
        }
        "transpose-chars" => {
            if *cursor_pos > 0 && input.chars().count() > 1 {
                let pos = if *cursor_pos >= input.chars().count() { *cursor_pos - 1 } else { *cursor_pos };
                if pos > 0 {
                    let chars: Vec<char> = input.chars().collect();
                    let a = chars[pos - 1];
                    let b = chars[pos];
                    let before: String = chars[..pos - 1].iter().collect();
                    let after: String = chars[pos + 1..].iter().collect();
                    *input = format!("{}{}{}{}", before, b, a, after);
                    *cursor_pos = pos;
                }
            }
        }
        "clear-screen" => {
            print!("\x1b[2J\x1b[H");
        }
        _ => {}
    }
}

fn is_input_incomplete(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    let last_nl = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let last_line = &input[last_nl..];
    let trimmed = last_line.trim_end();
    let trimmed_chars: Vec<char> = trimmed.chars().collect();
    let mut bs_count = 0usize;
    for ch in trimmed_chars.iter().rev() {
        if *ch == '\\' {
            bs_count += 1;
        } else {
            break;
        }
    }
    if bs_count % 2 == 1 {
        return true;
    }

    let mut in_single = false;
    let mut in_double = false;
    let mut cmd_sub_depth: i32 = 0;
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
        } else if in_double {
            match ch {
                '\\' if i + 1 < len => { i += 1; }
                '"' => { in_double = false; }
                _ => {}
            }
        } else {
            match ch {
                '\'' => { in_single = true; }
                '"' => { in_double = true; }
                '$' if i + 1 < len && chars[i + 1] == '(' => {
                    cmd_sub_depth += 1;
                    i += 1;
                }
                ')' if cmd_sub_depth > 0 => { cmd_sub_depth -= 1; }
                '\\' if i + 1 < len => { i += 1; }
                '#' => {
                    while i < len && chars[i] != '\n' { i += 1; }
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    in_single || in_double || cmd_sub_depth > 0
}

#[allow(clippy::too_many_arguments)]
pub fn read_line_editor(
    prompt: &PromptDisplay,
    history: &[String],
    editor_cfg: &crate::config::schema::EditorConfig,
    autosuggest_cfg: &crate::config::schema::AutosuggestConfig,
    history_cfg: &crate::config::schema::HistoryConfig,
    clipboard_cfg: &crate::config::schema::ClipboardConfig,
    cursor_cfg: &crate::config::schema::CursorConfig,
    prompt_cfg: &crate::config::schema::PromptConfig,
    colors_cfg: &crate::config::schema::ColorsConfig,
    symbols_cfg: &crate::config::schema::SymbolsConfig,
    custom_bindings: &std::collections::HashMap<String, String>,
) -> io::Result<String> {
    let mut input = String::new();
    let mut cursor_pos: usize = 0;
    let mut temp_buf = String::new();
    let mut history_offset: Option<usize> = None;
    let mode = if editor_cfg.mode == "vi" {
        EditorMode::ViInsert
    } else {
        EditorMode::Emacs
    };
    let mut mode = mode;
    let _vi_register: char = '"';
    let mut last_vi_action: Option<String> = None;
    let mut overwrite_mode = false;
    let mut prev_mode_for_cursor = EditorMode::Emacs;

    let mut rctx = EditorRenderCtx {
        prompt: Cow::Borrowed(prompt),
        autosuggest_cfg,
        colorize: editor_cfg.colorize_output,
        colors: colors_cfg,
    };

    if editor_cfg.bracketed_paste {
        print!("\x1b[?2004h");
    }
    if editor_cfg.mode == "vi" {
        print!("{}", vi_cursor_escape(&editor_cfg.vi_cursor_insert));
    } else {
        let style_esc = match cursor_cfg.style.as_str() {
            "block" => "\x1b[2 q",
            "beam" => "\x1b[5 q",
            "underline" => "\x1b[4 q",
            _ => "\x1b[2 q",
        };
        print!("{}", style_esc);
    }
    if cursor_cfg.blink {
        print!("\x1b[?12h");
    } else {
        print!("\x1b[?12l");
    }
    print!("\x1b[?25h");
    io::stdout().flush()?;

    render_display(&rctx, &input, cursor_pos, "")?;
    print!("\x1b[?25h");
    io::stdout().flush()?;

    loop {
        let mut suggestion = if autosuggest_cfg.enabled && autosuggest_cfg.strategy != "none" && input.chars().count() >= autosuggest_cfg.min_chars as usize {
            find_suggestion(&input, history, autosuggest_cfg.case_sensitive, history_cfg.substring_search)
        } else {
            String::new()
        };

        let event = match event::read() {
            Ok(e) => e,
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
        };
        match event {
            Event::Paste(text) => {
                let max_len = editor_cfg.max_line_length as usize;
                for ch in text.chars() {
                    if max_len > 0 && input.chars().count() >= max_len {
                        terminal_bell(&editor_cfg.bell);
                        break;
                    }
                    if ch == '\r' || ch == '\n' {
                        input.push('\n');
                    } else {
                        insert_char_at(&mut input, cursor_pos, ch);
                    }
                    cursor_pos += 1;
                }
                history_offset = None;
                redraw(&rctx, &input, cursor_pos, "")?;
                continue;
            }
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                match mode {
                    EditorMode::ViNormal | EditorMode::ViVisual => {
                        let mut switched_to_insert = false;
                        match code {
                            KeyCode::Char('i') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('a') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                if cursor_pos < input.chars().count() {
                                    cursor_pos += 1;
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('A') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                cursor_pos = input.chars().count();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('I') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('o') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                input.push('\n');
                                cursor_pos = input.chars().count();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('O') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                let newline_pos = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                input.insert(newline_pos, '\n');
                                cursor_pos = newline_pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('h') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = cursor_pos.saturating_sub(1);
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('l') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if cursor_pos < input.chars().count() { cursor_pos += 1; }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('0') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('$') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = input.chars().count();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('w') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                let delims: Vec<char> = editor_cfg.word_delimiters.chars().collect();
                                let mut pos = cursor_pos;
                                while pos < chars.len() && delims.contains(&chars[pos]) { pos += 1; }
                                while pos < chars.len() && !delims.contains(&chars[pos]) { pos += 1; }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('b') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                let delims: Vec<char> = editor_cfg.word_delimiters.chars().collect();
                                let mut pos = cursor_pos;
                                while pos > 0 && delims.contains(&chars[pos - 1]) { pos -= 1; }
                                while pos > 0 && !delims.contains(&chars[pos - 1]) { pos -= 1; }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('x') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if cursor_pos < input.chars().count() {
                                    let removed = input.chars().nth(cursor_pos).unwrap_or('\0');
                                    push_undo(&input, cursor_pos);
                                    remove_char_at(&mut input, cursor_pos);
                                    kill_ring_push(&removed.to_string());
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('d') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                last_vi_action = Some("d".to_string());
                            }
                            KeyCode::Char('d') if last_vi_action.as_deref() == Some("d") => {
                                let line_end = input[cursor_pos..].find('\n').map(|p| cursor_pos + p).unwrap_or(input.chars().count());
                                let killed: String = input.chars().skip(cursor_pos).take(line_end - cursor_pos).collect();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                let bytes_to_remove = char_to_byte(&input, line_end) - char_to_byte(&input, cursor_pos);
                                let byte_start = char_to_byte(&input, cursor_pos);
                                input.drain(byte_start..byte_start + bytes_to_remove);
                                last_vi_action = None;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('D') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let killed: String = input[cursor_pos..].to_string();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                input.truncate(char_to_byte(&input, cursor_pos));
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('p') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(yanked) = kill_ring_yank() {
                                    push_undo(&input, cursor_pos);
                                    let before: String = input.chars().take(cursor_pos).collect();
                                    let after: String = input.chars().skip(cursor_pos).collect();
                                    input = format!("{}{}{}", before, yanked, after);
                                    cursor_pos += yanked.chars().count();
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('u') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some((prev, prev_cursor)) = undo(&input, cursor_pos) {
                                    input = prev;
                                    cursor_pos = prev_cursor;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('r') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some((next, next_cursor)) = redo(&input, cursor_pos) {
                                    input = next;
                                    cursor_pos = next_cursor;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('c') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                input.clear();
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('v') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                mode = EditorMode::ViVisual;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Enter if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if is_input_incomplete(&input) {
                                    if input.trim_end().ends_with('\\') {
                                        let last_nl = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                        let last_line = &input[last_nl..];
                                        let trimmed = last_line.trim_end();
                                        let mut bs_count = 0usize;
                                        for ch in trimmed.chars().rev() {
                                            if ch == '\\' { bs_count += 1; } else { break; }
                                        }
                                        if bs_count % 2 == 1 {
                                            let strip = trimmed.len() - bs_count;
                                            input = format!("{}{}", &input[..last_nl + strip], &input[last_nl + trimmed.len()..]);
                                        } else {
                                            input.push('\n');
                                        }
                                    } else {
                                        input.push('\n');
                                    }
                                    cursor_pos = input.chars().count();
                                    let (ps2, from_env) = ps2_display();
                                    if from_env {
                                        eprint!("{}", ps2);
                                        let _ = io::stderr().flush();
                                    }
                                    rctx.prompt = Cow::Owned(PromptDisplay {
                                        lines_above: vec![],
                                        input_prefix: if from_env { String::new() } else { ps2 },
                                        lines_below: vec![],
                                        right_prompt: String::new(),
                                        right_prompt_color: String::new(),
                                        right_prompt_hide_threshold: 0.0,
                                    });
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                    continue;
                                }
                                if editor_cfg.bracketed_paste {
                                    print!("\x1b[?2004l");
                                }
                                print!("\r\n");
                                io::stdout().flush()?;
                                return Ok(input);
                            }
                            KeyCode::Char('q') if !modifiers.contains(KeyModifiers::CONTROL) => {}
                            KeyCode::Esc => {
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_digit() && last_vi_action.as_deref() == Some("d") => {
                                if c == 'd' {
                                    let line_start = input[..char_to_byte(&input, cursor_pos)].rfind('\n').map(|p| p + 1).unwrap_or(0);
                                    let line_end = input[cursor_pos..].find('\n').map(|p| cursor_pos + p).unwrap_or(input.chars().count());
                                    let killed: String = input[line_start..char_to_byte(&input, line_end)].to_string();
                                    push_undo(&input, cursor_pos);
                                    kill_ring_push(&killed);
                                    let byte_start = char_to_byte(&input, line_start);
                                    let byte_end = char_to_byte(&input, line_end);
                                    input.drain(byte_start..byte_end);
                                    cursor_pos = line_start;
                                    last_vi_action = None;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                } else {
                                    last_vi_action = None;
                                }
                            }
                            _ => {
                                terminal_bell(&editor_cfg.bell);
                            }
                        }
                        if editor_cfg.mode == "vi" && prev_mode_for_cursor != mode {
                            let cursor_esc = if switched_to_insert {
                                vi_cursor_escape(&editor_cfg.vi_cursor_insert)
                            } else {
                                vi_cursor_escape(&editor_cfg.vi_cursor_block)
                            };
                            print!("{}", cursor_esc);
                            let mode_text = match &mode {
                                EditorMode::ViInsert => prompt_cfg.vi_prompt_insert.single(),
                                EditorMode::ViNormal => prompt_cfg.vi_cmd_prompt.single(),
                                EditorMode::ViVisual => prompt_cfg.vi_prompt_visual.single(),
                                EditorMode::Emacs => String::new(),
                            };
                            let mode_text = if mode_text.is_empty() {
                                match &mode {
                                    EditorMode::ViNormal => prompt_cfg.vi_prompt_normal.single(),
                                    _ => mode_text,
                                }
                            } else {
                                mode_text
                            };
                            if !mode_text.is_empty() {
                                let color = match &mode {
                                    EditorMode::ViInsert => hex_to_ansi(&prompt_cfg.vi_cmd_color_success),
                                    EditorMode::ViNormal => hex_to_ansi(&prompt_cfg.vi_cmd_color),
                                    EditorMode::ViVisual => hex_to_ansi(&prompt_cfg.vi_cmd_color_error),
                                    EditorMode::Emacs => String::new(),
                                };
                                print!("\x1b[s");
                                print!("\x1b[1E");
                                print!("\x1b[2K");
                                print!("{}{}\x1b[0m", color, mode_text);
                                print!("\x1b[u");
                            }
                            io::stdout().flush()?;
                            prev_mode_for_cursor = mode.clone();
                        }
                    }
                    _ => {
                        // Emacs mode / Vi insert mode
                        let key_str = format_key(code, modifiers);
                        if let Some(widget) = custom_bindings.get(&key_str) {
                            exec_widget(widget, &mut input, &mut cursor_pos, history, &mut history_offset, &mut temp_buf, &editor_cfg.word_delimiters, clipboard_cfg);
                            redraw(&rctx, &input, cursor_pos, &suggestion)?;
                        } else {
                        // Check configurable autosuggestion accept keys before the main match
                        if !suggestion.is_empty() && cursor_pos == input.chars().count() {
                            if matches_key_event(code, modifiers, &autosuggest_cfg.accept_key) {
                                input = format!("{}{}", input, suggestion);
                                cursor_pos = input.chars().count();
                                history_offset = None;
                                redraw(&rctx, &input, cursor_pos, "")?;
                                continue;
                            } else if matches_key_event(code, modifiers, &autosuggest_cfg.accept_word_key) {
                                let suffix = suggestion;
                                let chars: Vec<char> = input.chars().collect();
                                let after: String = chars[cursor_pos..].iter().collect();
                                let delimiters: Vec<char> = editor_cfg.word_delimiters.chars().collect();
                                let mut word_end = 0;
                                let suffix_chars: Vec<char> = suffix.chars().collect();
                                while word_end < suffix_chars.len() {
                                    if delimiters.contains(&suffix_chars[word_end]) && word_end > 0 {
                                        break;
                                    }
                                    word_end += 1;
                                }
                                let word: String = suffix_chars[..word_end].iter().collect();
                                let before: String = chars[..cursor_pos].iter().collect();
                                input = format!("{}{}{}", before, word, after);
                                cursor_pos += word.chars().count();
                                history_offset = None;
                                redraw(&rctx, &input, cursor_pos, "")?;
                                continue;
                            }
                        }
                        match code {
                            KeyCode::Enter => {
                                if is_input_incomplete(&input) {
                                    if input.trim_end().ends_with('\\') {
                                        let last_nl = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                        let last_line = &input[last_nl..];
                                        let trimmed = last_line.trim_end();
                                        let mut bs_count = 0usize;
                                        for ch in trimmed.chars().rev() {
                                            if ch == '\\' { bs_count += 1; } else { break; }
                                        }
                                        if bs_count % 2 == 1 {
                                            let strip = trimmed.len() - bs_count;
                                            input = format!("{}{}", &input[..last_nl + strip], &input[last_nl + trimmed.len()..]);
                                        } else {
                                            input.push('\n');
                                        }
                                    } else {
                                        input.push('\n');
                                    }
                                    cursor_pos = input.chars().count();
                                    let (ps2, from_env) = ps2_display();
                                    if from_env {
                                        eprint!("{}", ps2);
                                        let _ = io::stderr().flush();
                                    }
                                    rctx.prompt = Cow::Owned(PromptDisplay {
                                        lines_above: vec![],
                                        input_prefix: if from_env { String::new() } else { ps2 },
                                        lines_below: vec![],
                                        right_prompt: String::new(),
                                        right_prompt_color: String::new(),
                                        right_prompt_hide_threshold: 0.0,
                                    });
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                    continue;
                                }
                                if editor_cfg.bracketed_paste {
                                    print!("\x1b[?2004l");
                                }
                                print!("\r\n");
                                io::stdout().flush()?;
                                return Ok(input);
                            }
                            KeyCode::Char('t') if modifiers.contains(KeyModifiers::CONTROL) && editor_cfg.emacs_overwrite_mode => {
                                overwrite_mode = !overwrite_mode;
                            }
                            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                                print!("^C\r\n");
                                input.clear();
                                cursor_pos = 0;
                                rctx.prompt = Cow::Borrowed(prompt);
                                redraw(&rctx, &input, cursor_pos, "")?;
                            }
                            KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                                if input.is_empty() {
                                    if editor_cfg.bracketed_paste {
                                        print!("\x1b[?2004l");
                                    }
                                    print!("{}\r\n", symbols_cfg.exit_prefix);
                                    crate::shell::signals::SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
                                    crate::shell::signals::EXIT_CODE.store(0, std::sync::atomic::Ordering::SeqCst);
                                    return Ok(String::new());
                                }
                            }
                            KeyCode::Char('l') if modifiers.contains(KeyModifiers::CONTROL) => {
                                print!("\x1b[2J\x1b[H");
                                render_display(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = input.chars().count();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                                let killed: String = input[char_to_byte(&input, cursor_pos)..].to_string();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                input.truncate(char_to_byte(&input, cursor_pos));
                                history_offset = None;
                                suggestion.clear();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                                let killed: String = input[..char_to_byte(&input, cursor_pos)].to_string();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                let tail: String = input[char_to_byte(&input, cursor_pos)..].to_string();
                                input.clear();
                                input.push_str(&tail);
                                cursor_pos = 0;
                                history_offset = None;
                                suggestion.clear();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                                let delimiters: Vec<char> = editor_cfg.word_delimiters.chars().collect();
                                let chars: Vec<char> = input.chars().collect();
                                let mut new_pos = cursor_pos;
                                while new_pos > 0 {
                                    let ch = chars[new_pos - 1];
                                    if delimiters.contains(&ch) { break; }
                                    new_pos -= 1;
                                }
                                if new_pos < cursor_pos {
                                    let byte_start = char_to_byte(&input, new_pos);
                                    let byte_end = char_to_byte(&input, cursor_pos);
                                    let killed: String = input[byte_start..byte_end].to_string();
                                    push_undo(&input, cursor_pos);
                                    kill_ring_push(&killed);
                                    input.drain(byte_start..byte_end);
                                    cursor_pos = new_pos;
                                    history_offset = None;
                                    suggestion.clear();
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('y') if modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(yanked) = kill_ring_yank() {
                                    if clipboard_cfg.yank_to_clipboard {
                                        clipboard_copy(&yanked, clipboard_cfg);
                                    }
                                    push_undo(&input, cursor_pos);
                                    let before: String = input.chars().take(cursor_pos).collect();
                                    let after: String = input.chars().skip(cursor_pos).collect();
                                    input = format!("{}{}{}", before, yanked, after);
                                    cursor_pos += yanked.chars().count();
                                    history_offset = None;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('z') if modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some((prev, prev_cursor)) = undo(&input, cursor_pos) {
                                    input = prev;
                                    cursor_pos = prev_cursor;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('_') if modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some((next, next_cursor)) = redo(&input, cursor_pos) {
                                    input = next;
                                    cursor_pos = next_cursor;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Right if !modifiers.contains(KeyModifiers::CONTROL) && cursor_pos < input.chars().count() => {
                                cursor_pos += 1;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('f') if modifiers.contains(KeyModifiers::ALT) => {
                                if cursor_pos < input.chars().count() {
                                    let delimiters: Vec<char> = editor_cfg.word_delimiters.chars().collect();
                                    let chars: Vec<char> = input.chars().collect();
                                    let mut pos = cursor_pos;
                                    while pos < chars.len() && delimiters.contains(&chars[pos]) {
                                        pos += 1;
                                    }
                                    while pos < chars.len() && !delimiters.contains(&chars[pos]) {
                                        pos += 1;
                                    }
                                    let moved = pos - cursor_pos;
                                    if moved > 0 {
                                        cursor_pos = pos;
                                        print!("\x1b[{}C", moved);
                                        io::stdout().flush()?;
                                    }
                                }
                            }
                            KeyCode::Char('r') if modifiers.contains(KeyModifiers::CONTROL) => {
                                let saved_input = input.clone();
                                let saved_cursor = cursor_pos;
                                let prompt_prefix = &rctx.prompt.input_prefix;
                                print!("\x1b[2K\r{}", prompt_prefix);
                                print!("(reverse-i-search)`': ");
                                io::stdout().flush()?;
                                let mut search_buf = String::new();
                                loop {
                                    let ev = match event::read() {
                                        Ok(e) => e,
                                        Err(_) => {
                                            std::thread::sleep(std::time::Duration::from_millis(10));
                                            continue;
                                        }
                                    };
                                    if let Event::Key(KeyEvent { code: sc, modifiers: sm, .. }) = ev {
                                        match sc {
                                            KeyCode::Char(c) if !sm.contains(KeyModifiers::CONTROL) => {
                                                search_buf.push(c);
                                                let found = if history_cfg.search_case_sensitive {
                                                    history.iter().rev().find(|h| h.contains(search_buf.as_str()))
                                                } else {
                                                    let needle = search_buf.to_lowercase();
                                                    history.iter().rev().find(|h| h.to_lowercase().contains(&needle))
                                                };
                                                if let Some(found) = found {
                                                    input = found.clone();
                                                     print!("\x1b[2K\r{}(reverse-i-search)`{}': {}", prompt_prefix, search_buf, highlight_line(&input, editor_cfg.colorize_output, colors_cfg));
                                                } else {
                                                    print!("\x1b[2K\r{}(reverse-i-search)`{}': ", prompt_prefix, search_buf);
                                                }
                                                io::stdout().flush()?;
                                            }
                                            KeyCode::Enter => {
                                                if is_input_incomplete(&input) {
                                                    if input.trim_end().ends_with('\\') {
                                                        let last_nl = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                                        let last_line = &input[last_nl..];
                                                        let trimmed = last_line.trim_end();
                                                        let mut bs_count = 0usize;
                                                        for ch in trimmed.chars().rev() {
                                                            if ch == '\\' { bs_count += 1; } else { break; }
                                                        }
                                                        if bs_count % 2 == 1 {
                                                            let strip = trimmed.len() - bs_count;
                                                            input = format!("{}{}", &input[..last_nl + strip], &input[last_nl + trimmed.len()..]);
                                                        } else {
                                                            input.push('\n');
                                                        }
                                                    } else {
                                                        input.push('\n');
                                                    }
                                                     cursor_pos = input.chars().count();
                                                    let (ps2, from_env) = ps2_display();
                                                    if from_env {
                                                        eprint!("{}", ps2);
                                                        let _ = io::stderr().flush();
                                                    }
                                                    rctx.prompt = Cow::Owned(PromptDisplay {
                                                        lines_above: vec![],
                                                        input_prefix: if from_env { String::new() } else { ps2 },
                                                        lines_below: vec![],
                                                        right_prompt: String::new(),
                                                        right_prompt_color: String::new(),
                                                        right_prompt_hide_threshold: 0.0,
                                                    });
                                                    print!("\r\n");
                                                    break;
                                                }
                                                print!("\r\n");
                                                return Ok(input);
                                            }
                                            KeyCode::Esc => {
                                                input = saved_input;
                                                cursor_pos = saved_cursor;
                                                rctx.prompt = Cow::Borrowed(prompt);
                                                print!("\r{}", rctx.prompt.input_prefix);
                                                io::stdout().flush()?;
                                                break;
                                            }
                                            KeyCode::Backspace => {
                                                search_buf.pop();
                                                print!("\x1b[2K\r{}(reverse-i-search)`{}': ", prompt_prefix, search_buf);
                                                io::stdout().flush()?;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                let max_len = editor_cfg.max_line_length as usize;
                                if max_len > 0 && input.chars().count() >= max_len {
                                    terminal_bell(&editor_cfg.bell);
                                    continue;
                                }
                                insert_char_at(&mut input, cursor_pos, '\t');
                                cursor_pos += 1;
                                history_offset = None;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Esc if mode == EditorMode::ViInsert => {
                                mode = EditorMode::ViNormal;
                                if cursor_pos > 0 && cursor_pos >= input.chars().count() {
                                    cursor_pos = cursor_pos.saturating_sub(1);
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Up => {
                                if history_offset.is_none() {
                                    temp_buf = input.clone();
                                    history_offset = Some(0);
                                } else if let Some(ref mut idx) = history_offset
                                    && *idx < history.len().saturating_sub(1) {
                                        *idx += 1;
                                    }
                                if let Some(idx) = history_offset {
                                    let hi = history.len().saturating_sub(1 + idx);
                                    if hi < history.len() {
                                        input = history[hi].clone();
                                        cursor_pos = input.chars().count();
                                        redraw(&rctx, &input, cursor_pos, "")?;
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if let Some(idx) = history_offset {
                                    if idx > 0 {
                                        history_offset = Some(idx - 1);
                                        let hi = history.len().saturating_sub(1 + history_offset.expect("history_offset Some"));
                                        input = history[hi].clone();
                                        cursor_pos = input.chars().count();
                                    } else {
                                        history_offset = None;
                                        input = temp_buf.clone();
                                        cursor_pos = input.chars().count();
                                    }
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                }
                            }
                            KeyCode::Left => {
                                if cursor_pos > 0 {
                                    cursor_pos = cursor_pos.saturating_sub(1);
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Home => {
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::End => {
                                if !suggestion.is_empty() && cursor_pos == input.chars().count() {
                                    input = format!("{}{}", input, suggestion);
                                    cursor_pos = input.chars().count();
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                } else {
                                    cursor_pos = input.chars().count();
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Backspace => {
                                if cursor_pos > 0 {
                                    let chars: Vec<char> = input.chars().collect();
                                    let ch = chars[cursor_pos - 1];
                                    let at_end = cursor_pos < chars.len();
                                    let next_ch = if at_end { Some(chars[cursor_pos]) } else { None };
                                    if editor_cfg.auto_match_quotes && ((ch == '"' && next_ch == Some('"')) || (ch == '\'' && next_ch == Some('\''))) {
                                        cursor_pos -= 1;
                                        remove_char_at(&mut input, cursor_pos);
                                        remove_char_at(&mut input, cursor_pos);
                                    } else {
                                        cursor_pos -= 1;
                                        remove_char_at(&mut input, cursor_pos);
                                    }
                                    history_offset = None;
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                }
                            }
                            KeyCode::Delete => {
                                if cursor_pos < input.chars().count() {
                                    remove_char_at(&mut input, cursor_pos);
                                    history_offset = None;
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                }
                            }
                            KeyCode::Char(c) => {
                                if input.chars().count() >= editor_cfg.max_line_length as usize {
                                    terminal_bell(&editor_cfg.bell);
                                    continue;
                                }
                                if overwrite_mode && cursor_pos < input.chars().count() {
                                    push_undo(&input, cursor_pos);
                                    let byte = char_to_byte(&input, cursor_pos);
                                    input.remove(byte);
                                    input.insert(byte, c);
                                    cursor_pos += 1;
                                } else if editor_cfg.auto_match_quotes && (c == '"' || c == '\'') {
                                    push_undo(&input, cursor_pos);
                                    insert_char_at(&mut input, cursor_pos, c);
                                    cursor_pos += 1;
                                    insert_char_at(&mut input, cursor_pos, c);
                                } else {
                                    push_undo(&input, cursor_pos);
                                    insert_char_at(&mut input, cursor_pos, c);
                                    cursor_pos += 1;
                                }
                                history_offset = None;
                                redraw(&rctx, &input, cursor_pos, "")?;
                            }
                            _ => {
                                terminal_bell(&editor_cfg.bell);
                            }
                        }
                        } // end custom binding else
                    }
                }
            }
            Event::Resize(_cols, _rows) => {
                redraw(&rctx, &input, cursor_pos, &suggestion)?;
            }
            _ => {}
        }
    }
}

fn render_display(context: &EditorRenderCtx, input: &str, cursor_pos: usize, suggestion: &str) -> io::Result<()> {
    print!("\x1b[?25l");
    if context.prompt.lines_above.is_empty() {
        print!("\r\x1b[2K");
    }
    for line in &context.prompt.lines_above {
        print!("{}\r\n", line);
    }
    print!("{}", context.prompt.input_prefix);
    print!("{}", highlight_line(input, context.colorize, context.colors));
    if !suggestion.is_empty() {
        let color = color_to_ansi(&context.autosuggest_cfg.highlight_color);
        print!("{}{}\x1b[0m", color, suggestion);
    }

    if !context.prompt.right_prompt.is_empty() {
        let term_width = crate::terminal::prompt::get_terminal_width();
        let input_vis = input.chars().count() + suggestion.chars().count();
        let rprompt_vis = visible_str_len(&context.prompt.right_prompt);
        let prefix_vis = visible_str_len(&context.prompt.input_prefix);
        let threshold = (term_width as f64 * context.prompt.right_prompt_hide_threshold) as usize;
        let occupied = prefix_vis + input_vis;

        if occupied < threshold {
            let padding = if term_width > prefix_vis + input_vis + rprompt_vis {
                term_width - prefix_vis - input_vis - rprompt_vis
            } else {
                0
            };
            print!("{}{}{}\x1b[0m", " ".repeat(padding), color_to_ansi(&context.prompt.right_prompt_color), context.prompt.right_prompt);
        }
    }

    let visible_after = input.chars().count() + suggestion.chars().count() - cursor_pos;
    if visible_after > 0 {
        print!("\x1b[{}D", visible_after);
    }
    if !context.prompt.lines_below.is_empty() {
        print!("\r\n");
        for line in &context.prompt.lines_below {
            print!("{}\r\n", line);
        }
    }
    io::stdout().flush()
}

fn redraw(context: &EditorRenderCtx, input: &str, cursor_pos: usize, suggestion: &str) -> io::Result<()> {
    let above_count = context.prompt.lines_above.len();
    let below_count = context.prompt.lines_below.len();

    if above_count > 0 {
        print!("\x1b[{}A", above_count + 1);
        for _ in 0..=above_count {
            print!("\x1b[2K\r\n");
        }
        print!("\x1b[{}A", above_count);
        for line in &context.prompt.lines_above {
            print!("{}\r\n", line);
        }
    } else {
        print!("\r\x1b[2K");
    }

    print!("{}", context.prompt.input_prefix);
    print!("{}", highlight_line(input, context.colorize, context.colors));
    if !suggestion.is_empty() {
        let color = color_to_ansi(&context.autosuggest_cfg.highlight_color);
        print!("{}{}\x1b[0m", color, suggestion);
    }

    if !context.prompt.right_prompt.is_empty() {
        let term_width = crate::terminal::prompt::get_terminal_width();
        let input_vis = input.chars().count() + suggestion.chars().count();
        let rprompt_vis = visible_str_len(&context.prompt.right_prompt);
        let prefix_vis = visible_str_len(&context.prompt.input_prefix);
        let threshold = (term_width as f64 * context.prompt.right_prompt_hide_threshold) as usize;
        let occupied = prefix_vis + input_vis;

        if occupied < threshold {
            let padding = if term_width > prefix_vis + input_vis + rprompt_vis {
                term_width - prefix_vis - input_vis - rprompt_vis
            } else {
                0
            };
            print!("{}{}{}\x1b[0m", " ".repeat(padding), color_to_ansi(&context.prompt.right_prompt_color), context.prompt.right_prompt);
        }
    }

    if below_count > 0 {
        for line in &context.prompt.lines_below {
            print!("\x1b[2K{}\r\n", line);
        }
        print!("\x1b[{}A", below_count);
    }

    let visible_after = input.chars().count() + suggestion.chars().count() - cursor_pos;
    if visible_after > 0 {
        print!("\x1b[{}D", visible_after);
    }
    print!("\x1b[?25h");
    io::stdout().flush()
}

fn color_to_ansi(hex: &str) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return "\x1b[90m".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

fn visible_str_len(s: &str) -> usize {
    let mut len = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() { break; }
            }
        } else if !c.is_control() {
            len += 1;
        }
    }
    len
}

fn find_suggestion(input: &str, history: &[String], case_sensitive: bool, substring: bool) -> String {
    if input.is_empty() {
        return String::new();
    }
    let input_lower: Vec<char> = input.chars().map(|c| c.to_ascii_lowercase()).collect();
    let input_lower_str: String = input_lower.iter().collect();
    for entry in history.iter().rev() {
        let matches = if case_sensitive {
            entry.starts_with(input)
        } else {
            entry.to_lowercase().starts_with(&input_lower_str)
        };
        if matches && entry != input {
            return entry[input.len()..].to_string();
        }
    }
    if substring {
        let input_lower_str = input_lower_str.clone();
        for entry in history.iter().rev() {
            if entry == input {
                continue;
            }
            let contains = if case_sensitive {
                entry.contains(input)
            } else {
                entry.to_lowercase().contains(&input_lower_str)
            };
            if contains {
                let entry_lower: Vec<char> = entry.chars().map(|c| c.to_ascii_lowercase()).collect();
                if let Some(pos) = entry_lower.windows(input_lower.len()).position(|w| w == input_lower.as_slice()) {
                    return entry[pos + input.len()..].to_string();
                }
            }
        }
    }
    let mut best: Option<(String, usize)> = None;
    for entry in history.iter().rev() {
        if entry == input {
            continue;
        }
        let entry_chars: Vec<char> = entry.chars().collect();
        if let Some(suffix_start) = fuzzy_suffix_start(&input_lower, &entry_chars) {
            let score = fuzzy_score(&input_lower, &entry_chars[..suffix_start]);
            let suffix: String = entry_chars[suffix_start..].iter().collect();
            if !suffix.is_empty() {
                match &best {
                    Some((_, best_score)) if score > *best_score => {
                        best = Some((suffix, score));
                    }
                    None => {
                        best = Some((suffix, score));
                    }
                    _ => {}
                }
            }
        }
    }
    best.map(|(s, _)| s).unwrap_or_default()
}

fn fuzzy_suffix_start(needle: &[char], haystack: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut ni = 0;
    for (hi, _) in haystack.iter().enumerate() {
        if ni < needle.len() && haystack[hi].to_ascii_lowercase() == needle[ni] {
            ni += 1;
            if ni == needle.len() {
                return Some(hi + 1);
            }
        }
    }
    None
}

fn fuzzy_score(needle: &[char], haystack: &[char]) -> usize {
    let mut score = 0usize;
    let mut ni = 0;
    let mut prev_was_separator = true;
    for (hi, &hc) in haystack.iter().enumerate() {
        if ni < needle.len() && hc.to_ascii_lowercase() == needle[ni] {
            score += 10;
            if prev_was_separator {
                score += 20;
            }
            if hi == 0 || !haystack[hi - 1].is_alphanumeric() {
                score += 15;
            }
            ni += 1;
        }
        prev_was_separator = !hc.is_alphanumeric();
    }
    if ni < needle.len() {
        return 0;
    }
    score
}
