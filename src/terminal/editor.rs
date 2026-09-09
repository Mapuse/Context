use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::io::{self, Write};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use super::color::hex_to_ansi;
use super::prompt::PromptDisplay;
use crate::shell::builtin::HISTORY_CB;

fn ps2_display() -> (String, bool) {
    match std::env::var("PS2") {
        Ok(v) => (v, true),
        Err(_) => ("> ".to_string(), false),
    }
}

#[derive(Clone)]
enum ViChange {
    Insert(String),
    Delete(usize, usize),
    Change(usize, usize, String),
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

fn find_word_start(input: &str, cursor: usize) -> usize {
    let chars: Vec<char> = input.chars().collect();
    if cursor == 0 {
        return 0;
    }
    let mut i = cursor;
    while i > 0 {
        let ch = chars[i - 1];
        if ch == ' '
            || ch == '\t'
            || ch == '\n'
            || ch == '|'
            || ch == '&'
            || ch == ';'
            || ch == '('
            || ch == '{'
        {
            return i;
        }
        i -= 1;
    }
    0
}

fn tab_common_prefix(completions: &[String]) -> String {
    if completions.is_empty() {
        return String::new();
    }
    let first: Vec<char> = completions[0].chars().collect();
    let mut len = first.len();
    for comp in &completions[1..] {
        let other: Vec<char> = comp.chars().collect();
        let mut i = 0;
        while i < len && i < other.len() && first[i] == other[i] {
            i += 1;
        }
        len = i;
    }
    first[..len].iter().collect()
}

static KILL_RING: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static UNDO_STACK: LazyLock<Mutex<Vec<(String, usize)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static REDO_STACK: LazyLock<Mutex<Vec<(String, usize)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static LAST_UNDO_PUSH: LazyLock<Mutex<Option<std::time::Instant>>> =
    LazyLock::new(|| Mutex::new(None));

use std::borrow::Cow;

struct EditorRenderCtx<'a> {
    prompt: Cow<'a, PromptDisplay>,
    autosuggest_cfg: &'a crate::config::schema::AutosuggestConfig,
    colorize: bool,
    colors: &'a crate::config::schema::ColorsConfig,
}

fn push_undo(input: &str, cursor: usize) {
    let now = std::time::Instant::now();
    let coalesce = matches!(
        LAST_UNDO_PUSH.lock().expect("LAST_UNDO_PUSH lock").as_ref(),
        Some(last) if now.duration_since(*last) < std::time::Duration::from_millis(500)
    );
    let mut stack = UNDO_STACK.lock().expect("UNDO_STACK lock");
    if coalesce && !stack.is_empty() {
        stack.pop();
    }
    stack.push((input.to_string(), cursor));
    drop(stack);
    if !coalesce {
        *LAST_UNDO_PUSH.lock().expect("LAST_UNDO_PUSH lock") = Some(now);
    }
    REDO_STACK.lock().expect("REDO_STACK lock").clear();
}

fn undo(input: &str, cursor: usize) -> Option<(String, usize)> {
    *LAST_UNDO_PUSH.lock().expect("LAST_UNDO_PUSH lock") = None;
    UNDO_STACK
        .lock()
        .expect("UNDO_STACK lock")
        .pop()
        .inspect(|_prev| {
            REDO_STACK
                .lock()
                .expect("REDO_STACK lock")
                .push((input.to_string(), cursor));
        })
}

fn redo(input: &str, cursor: usize) -> Option<(String, usize)> {
    *LAST_UNDO_PUSH.lock().expect("LAST_UNDO_PUSH lock") = None;
    REDO_STACK
        .lock()
        .expect("REDO_STACK lock")
        .pop()
        .inspect(|_prev| {
            UNDO_STACK
                .lock()
                .expect("UNDO_STACK lock")
                .push((input.to_string(), cursor));
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
    if modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
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
        "audible" => {
            print!("\x07");
        }
        "visible" => {
            print!("\x1b[?5h");
            print!("\x1b[?5l");
        }
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
    if spec.is_empty() {
        return false;
    }
    let (exp_mods, exp_code) = parse_key_spec(spec);
    let ctrl_ok =
        !exp_mods.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::CONTROL);
    let alt_ok = !exp_mods.contains(KeyModifiers::ALT) || modifiers.contains(KeyModifiers::ALT);
    let shift_ok =
        !exp_mods.contains(KeyModifiers::SHIFT) || modifiers.contains(KeyModifiers::SHIFT);
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
    if !cfg.enabled || text.is_empty() {
        return;
    }
    let tool = match cfg.method.as_str() {
        "xclip" => Some("xclip"),
        "xsel" => Some("xsel"),
        "pbcopy" => Some("pbcopy"),
        "wl-copy" => Some("wl-copy"),
        _ => ["pbcopy", "xclip", "xsel", "wl-copy"]
            .iter()
            .find(|t| {
                Command::new(t).arg("--version").output().is_ok()
                    || Command::new(t)
                        .stdin(std::process::Stdio::null())
                        .spawn()
                        .is_ok()
            })
            .copied(),
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

fn highlight_line(
    input: &str,
    colorize: bool,
    colors: &crate::config::schema::ColorsConfig,
) -> String {
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
            '#' if i == 0
                || (i > 0 && matches!(chars[i - 1], ' ' | '|' | '&' | ';' | '(' | '\n')) =>
            {
                out.push_str(&comment_color);
                while i < len {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '"' => {
                out.push_str(&string_color);
                out.push(chars[i]);
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        out.push(chars[i]);
                        i += 1;
                    }
                    out.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '\'' => {
                out.push_str(&string_color);
                out.push(chars[i]);
                i += 1;
                while i < len && chars[i] != '\'' {
                    out.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '`' => {
                out.push_str(&string_color);
                out.push(chars[i]);
                i += 1;
                while i < len && chars[i] != '`' {
                    out.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '$' => {
                out.push_str(&variable_color);
                out.push(chars[i]);
                i += 1;
                if i < len {
                    match chars[i] {
                        '{' => {
                            out.push(chars[i]);
                            i += 1;
                            while i < len && chars[i] != '}' {
                                out.push(chars[i]);
                                i += 1;
                            }
                            if i < len {
                                out.push(chars[i]);
                                i += 1;
                            }
                        }
                        '(' => {
                            out.push(chars[i]);
                            i += 1;
                            let mut depth = 1u32;
                            while i < len && depth > 0 {
                                match chars[i] {
                                    '(' => depth += 1,
                                    ')' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                                out.push(chars[i]);
                                i += 1;
                            }
                            if i < len {
                                out.push(chars[i]);
                                i += 1;
                            }
                        }
                        _ => {
                            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                                out.push(chars[i]);
                                i += 1;
                            }
                        }
                    }
                }
                out.push_str(&effective_reset);
            }
            '|' if i + 1 < len && chars[i + 1] == '&' => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                out.push(chars[i + 1]);
                i += 2;
                out.push_str(&effective_reset);
            }
            '|' | '&' if i + 1 < len && chars[i + 1] == chars[i] => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                out.push(chars[i + 1]);
                i += 2;
                out.push_str(&effective_reset);
            }
            '|' | '&' | ';' | '>' | '<' => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                i += 1;
                if i < len && (chars[i] == '>' || chars[i] == '&' || chars[i] == '<') {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '=' | '+' | '-' if i + 1 < len && chars[i + 1] == '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                i += 1;
                out.push(chars[i]);
                i += 1;
                out.push_str(&effective_reset);
            }
            '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                i += 1;
                if i < len && chars[i] == '=' {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push_str(&effective_reset);
            }
            '!' if i + 1 < len && chars[i + 1] == '=' => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                out.push(chars[i + 1]);
                i += 2;
                out.push_str(&effective_reset);
            }
            '\\' if i + 1 < len => {
                out.push_str(&operator_color);
                out.push(chars[i]);
                i += 1;
                out.push(chars[i]);
                i += 1;
                out.push_str(&effective_reset);
            }
            ' ' | '\t' => {
                out.push(chars[i]);
                i += 1;
            }
            _ => {
                let start = i;
                while i < len {
                    match chars[i] {
                        ' ' | '\t' | '"' | '\'' | '$' | '|' | '&' | ';' | '>' | '<' | '#' | '='
                        | '!' | '+' | '-' | '\\' | '(' | ')' | '{' | '}' | '`' => break,
                        _ => {}
                    }
                    i += 1;
                }
                if i == start {
                    out.push(chars[i]);
                    i += 1;
                } else if i > start {
                    let word: String = chars[start..i].iter().collect();
                    let mut prev = start;
                    while prev > 0 && chars[prev - 1] == ' ' {
                        prev -= 1;
                    }
                    let is_cmd_pos = prev == 0
                        || (prev > 0
                            && (chars[prev - 1] == '|'
                                || chars[prev - 1] == '&'
                                || chars[prev - 1] == ';'
                                || chars[prev - 1] == '('
                                || chars[prev - 1] == '\n'
                                || chars[prev - 1] == '!'));
                    if word.starts_with('-') && word.len() > 1 && word != "--" {
                        out.push_str(&flag_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else if !is_cmd_pos
                        && word.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && word.chars().all(|c| {
                            c.is_ascii_hexdigit()
                                || c == '.'
                                || c == 'x'
                                || c == 'X'
                                || c == 'o'
                                || c == 'O'
                                || c == 'b'
                                || c == 'B'
                        })
                    {
                        out.push_str(&number_color);
                        out.push_str(&word);
                        out.push_str(&effective_reset);
                    } else if word.starts_with('~') || word.chars().any(|c| c == '/') {
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
    let mut filtered = String::with_capacity(out.len());
    let out_chars: Vec<char> = out.chars().collect();
    let mut j = 0;
    while j < out_chars.len() {
        if out_chars[j] == '\\'
            && j + 1 < out_chars.len()
            && (out_chars[j + 1] == '[' || out_chars[j + 1] == ']')
        {
            j += 2;
        } else {
            filtered.push(out_chars[j]);
            j += 1;
        }
    }
    filtered
}

#[allow(clippy::too_many_arguments)]
fn exec_widget(
    widget: &str,
    input: &mut String,
    cursor_pos: &mut usize,
    history: &[String],
    history_offset: &mut Option<usize>,
    temp_buf: &mut String,
    word_delimiters: &str,
    clipboard_cfg: &crate::config::schema::ClipboardConfig,
) {
    let delims: Vec<char> = word_delimiters.chars().collect();
    let is_delim = |c: char| delims.contains(&c);
    match widget {
        "backward-char" => {
            if *cursor_pos > 0 {
                *cursor_pos -= 1;
            }
        }
        "forward-char" => {
            if *cursor_pos < input.chars().count() {
                *cursor_pos += 1;
            }
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
            while pos > 0 && is_delim(chars[pos - 1]) {
                pos -= 1;
            }
            while pos > 0 && !is_delim(chars[pos - 1]) {
                pos -= 1;
            }
            *cursor_pos = pos;
        }
        "forward-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut pos = *cursor_pos;
            while pos < chars.len() && is_delim(chars[pos]) {
                pos += 1;
            }
            while pos < chars.len() && !is_delim(chars[pos]) {
                pos += 1;
            }
            *cursor_pos = pos;
        }
        "beginning-of-line" => {
            *cursor_pos = 0;
        }
        "end-of-line" => {
            *cursor_pos = input.chars().count();
        }
        "kill-line" => {
            let killed: String = input.chars().skip(*cursor_pos).collect();
            kill_ring_push(&killed);
            input.truncate(char_to_byte(input, *cursor_pos));
        }
        "backward-kill-line" => {
            let killed: String = input.chars().take(*cursor_pos).collect();
            kill_ring_push(&killed);
            let tail: String = input.chars().skip(*cursor_pos).collect();
            input.clear();
            input.push_str(&tail);
            *cursor_pos = 0;
        }
        "kill-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut end = *cursor_pos;
            while end < chars.len() && is_delim(chars[end]) {
                end += 1;
            }
            while end < chars.len() && !is_delim(chars[end]) {
                end += 1;
            }
            let killed: String = input
                .chars()
                .skip(*cursor_pos)
                .take(end - *cursor_pos)
                .collect();
            kill_ring_push(&killed);
            let byte_start = char_to_byte(input, *cursor_pos);
            let byte_end = char_to_byte(input, end);
            input.drain(byte_start..byte_end);
        }
        "backward-kill-word" => {
            let chars: Vec<char> = input.chars().collect();
            let mut start = *cursor_pos;
            while start > 0 && is_delim(chars[start - 1]) {
                start -= 1;
            }
            while start > 0 && !is_delim(chars[start - 1]) {
                start -= 1;
            }
            let killed: String = input
                .chars()
                .skip(start)
                .take(*cursor_pos - start)
                .collect();
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
                && *idx < history.len().saturating_sub(1)
            {
                *idx += 1;
            }
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
                    let hi = history
                        .len()
                        .saturating_sub(1 + history_offset.expect("history_offset Some"));
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
                let pos = if *cursor_pos >= input.chars().count() {
                    *cursor_pos - 1
                } else {
                    *cursor_pos
                };
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
        "transpose-words" => {
            let chars: Vec<char> = input.chars().collect();
            let len = chars.len();
            if len < 2 {
                return;
            }
            let mut w1_start = *cursor_pos;
            while w1_start < len && is_delim(chars[w1_start]) {
                w1_start += 1;
            }
            let mut w1_end = w1_start;
            while w1_end < len && !is_delim(chars[w1_end]) {
                w1_end += 1;
            }
            let mut w2_end = *cursor_pos;
            while w2_end > 0 && is_delim(chars[w2_end - 1]) {
                w2_end -= 1;
            }
            let mut w2_start = w2_end;
            while w2_start > 0 && !is_delim(chars[w2_start - 1]) {
                w2_start -= 1;
            }
            if w2_start < w2_end && w1_start < w1_end && w2_end <= w1_start {
                let word_before: String = chars[w2_start..w2_end].iter().collect();
                let word_after: String = chars[w1_start..w1_end].iter().collect();
                let before: String = chars[..w2_start].iter().collect();
                let between: String = chars[w2_end..w1_start].iter().collect();
                let after: String = chars[w1_end..].iter().collect();
                *input = format!(
                    "{}{}{}{}{}",
                    before, word_after, between, word_before, after
                );
                *cursor_pos = w2_start + word_after.chars().count();
            }
        }
        "capitalize-word" => {
            let chars: Vec<char> = input.chars().collect();
            let len = chars.len();
            let mut start = *cursor_pos;
            while start < len && is_delim(chars[start]) {
                start += 1;
            }
            let mut end = start;
            while end < len && !is_delim(chars[end]) {
                end += 1;
            }
            if start < end {
                let word: String = chars[start..end].iter().collect();
                let mut result = String::with_capacity(word.len());
                let mut first = true;
                for ch in word.chars() {
                    if first {
                        result.extend(ch.to_uppercase());
                        first = false;
                    } else {
                        result.extend(ch.to_lowercase());
                    }
                }
                let byte_start = char_to_byte(input, start);
                let byte_end = char_to_byte(input, end);
                input.replace_range(byte_start..byte_end, &result);
                *cursor_pos = end;
            }
        }
        "upcase-word" => {
            let chars: Vec<char> = input.chars().collect();
            let len = chars.len();
            let mut start = *cursor_pos;
            while start < len && is_delim(chars[start]) {
                start += 1;
            }
            let mut end = start;
            while end < len && !is_delim(chars[end]) {
                end += 1;
            }
            if start < end {
                let word: String = chars[start..end].iter().collect();
                let upper: String = word.chars().flat_map(|c| c.to_uppercase()).collect();
                let byte_start = char_to_byte(input, start);
                let byte_end = char_to_byte(input, end);
                input.replace_range(byte_start..byte_end, &upper);
                *cursor_pos = end;
            }
        }
        "downcase-word" => {
            let chars: Vec<char> = input.chars().collect();
            let len = chars.len();
            let mut start = *cursor_pos;
            while start < len && is_delim(chars[start]) {
                start += 1;
            }
            let mut end = start;
            while end < len && !is_delim(chars[end]) {
                end += 1;
            }
            if start < end {
                let word: String = chars[start..end].iter().collect();
                let lower: String = word.chars().flat_map(|c| c.to_lowercase()).collect();
                let byte_start = char_to_byte(input, start);
                let byte_end = char_to_byte(input, end);
                input.replace_range(byte_start..byte_end, &lower);
                *cursor_pos = end;
            }
        }
        _ => {}
    }
}

pub fn is_input_incomplete(input: &str) -> bool {
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

    // Unterminated heredoc: `<<[-]DELIM` whose delimiter line never came.
    if heredoc_unterminated(input) {
        return true;
    }

    // Unterminated block constructs (if/fi, for|while|until/do/done,
    // case/esac, brace groups).
    if block_construct_unterminated(input) {
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
                '\\' if i + 1 < len => {
                    i += 1;
                }
                '"' => {
                    in_double = false;
                }
                _ => {}
            }
        } else {
            match ch {
                '\'' => {
                    in_single = true;
                }
                '"' => {
                    in_double = true;
                }
                '$' if i + 1 < len && chars[i + 1] == '(' => {
                    cmd_sub_depth += 1;
                    i += 1;
                }
                ')' if cmd_sub_depth > 0 => {
                    cmd_sub_depth -= 1;
                }
                '\\' if i + 1 < len => {
                    i += 1;
                }
                '#' => {
                    while i < len && chars[i] != '\n' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    in_single || in_double || cmd_sub_depth > 0
}

/// True when the input contains an unclosed `if`, loop, `case` or brace
/// group. Quote-aware and comment-aware word scan; best-effort.
fn block_construct_unterminated(input: &str) -> bool {
    let mut if_depth = 0i32;
    let mut loop_depth = 0i32;
    let mut case_depth = 0i32;
    let mut brace_depth = 0i32;
    for line in input.lines() {
        for tok in shell_words(line) {
            match tok.trim_end_matches(';') {
                "if" => if_depth += 1,
                "fi" => if_depth -= 1,
                "for" | "while" | "until" | "select" => loop_depth += 1,
                "done" => loop_depth -= 1,
                "case" => case_depth += 1,
                "esac" => case_depth -= 1,
                "{" => brace_depth += 1,
                "}" => brace_depth -= 1,
                _ => {}
            }
        }
    }
    if_depth > 0 || loop_depth > 0 || case_depth > 0 || brace_depth > 0
}

/// Split a line into unquoted words (quotes stripped, contents marked as
/// non-keywords via a `\u{1}` guard), stopping at an unquoted `#` comment.
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut in_single = false;
    let mut in_double = false;
    for c in line.chars() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                quoted = true;
            }
            continue;
        }
        if in_double {
            match c {
                '"' => in_double = false,
                '\\' => {
                    quoted = true;
                }
                _ => quoted = true,
            }
            continue;
        }
        match c {
            '\'' => {
                in_single = true;
                quoted = true;
            }
            '"' => {
                in_double = true;
                quoted = true;
            }
            '\\' => {
                quoted = true;
            }
            '#' if cur.is_empty() && !quoted => break,
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                    quoted = false;
                }
            }
            c => cur.push(c),
        }
    }
    // A trailing backslash-continuation or quote leaves `quoted` set; the
    // caller already treats those as incomplete.
    if !cur.is_empty() && !quoted {
        words.push(cur);
    }
    words
}

/// True when the input contains a `<<` redirection whose delimiter line has
/// not appeared yet. Best-effort: quotes around the delimiter are honored.
fn heredoc_unterminated(input: &str) -> bool {
    let mut pending: Vec<(String, bool)> = Vec::new();
    for line in input.lines() {
        // Delimiter check first: this line may close earlier heredocs.
        if !pending.is_empty() {
            let stripped = line.trim_start_matches('\t');
            let mut consumed = false;
            pending.retain(|(delim, _)| {
                if !consumed && stripped == delim {
                    consumed = true;
                    false
                } else {
                    true
                }
            });
            if !pending.is_empty() && !line.contains("<<") {
                continue;
            }
        }
        // Scan the line for `<<` operators (outside single/double quotes).
        let bytes: Vec<char> = line.chars().collect();
        let n = bytes.len();
        let mut i = 0;
        let mut in_single = false;
        let mut in_double = false;
        while i < n {
            let c = bytes[i];
            if in_single {
                if c == '\'' {
                    in_single = false;
                }
                i += 1;
                continue;
            }
            if in_double {
                if c == '"' {
                    in_double = false;
                }
                if c == '\\' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            match c {
                '\'' => in_single = true,
                '"' => in_double = true,
                '\\' => {
                    i += 1;
                }
                '<' if i + 1 < n && bytes[i + 1] == '<' && !(i + 2 < n && bytes[i + 2] == '<') => {
                    // Found a heredoc operator; read the delimiter word.
                    let mut j = i + 2;
                    let strip_tabs = j < n && bytes[j] == '-';
                    if strip_tabs {
                        j += 1;
                    }
                    while j < n && bytes[j].is_whitespace() {
                        j += 1;
                    }
                    let (delim, next) = if j < n && (bytes[j] == '\'' || bytes[j] == '"') {
                        let quote = bytes[j];
                        let start = j + 1;
                        let mut k = start;
                        while k < n && bytes[k] != quote {
                            k += 1;
                        }
                        (bytes[start..k.min(n)].iter().collect::<String>(), k + 1)
                    } else {
                        let start = j;
                        let mut k = j;
                        while k < n && !bytes[k].is_whitespace() && !";&|<>()".contains(bytes[k]) {
                            k += 1;
                        }
                        (bytes[start..k].iter().collect::<String>(), k)
                    };
                    if !delim.is_empty() {
                        pending.push((delim, strip_tabs));
                    }
                    i = next.max(i + 2);
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
    }
    !pending.is_empty()
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
    let mut last_vi_action: Option<String> = None;
    let mut vi_last_change: Option<ViChange> = None;
    let mut vi_search_pattern: Option<String> = None;
    let mut vi_search_forward: bool = true;
    let mut vi_visual_start: usize = 0;
    let mut overwrite_mode = false;
    let mut prev_mode_for_cursor = EditorMode::Emacs;
    let mut last_suggest_input = String::new();
    let mut cached_suggestion: Option<String> = None;

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
        let mut suggestion = if autosuggest_cfg.enabled
            && autosuggest_cfg.strategy != "none"
            && input.chars().count() >= autosuggest_cfg.min_chars as usize
        {
            if cached_suggestion.is_none() || input != last_suggest_input {
                cached_suggestion = Some(find_suggestion(
                    &input,
                    history,
                    autosuggest_cfg.case_sensitive,
                    history_cfg.substring_search,
                ));
                last_suggest_input.clone_from(&input);
            }
            cached_suggestion.clone().unwrap_or_default()
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
                        insert_char_at(&mut input, cursor_pos, '\n');
                    } else {
                        insert_char_at(&mut input, cursor_pos, ch);
                    }
                    cursor_pos += 1;
                }
                history_offset = None;
                redraw(&rctx, &input, cursor_pos, "")?;
                continue;
            }
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                match mode {
                    EditorMode::ViNormal | EditorMode::ViVisual => {
                        let mut switched_to_insert = false;
                        if mode == EditorMode::ViVisual {
                            match code {
                                KeyCode::Char('d')
                                    if !modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    let start = vi_visual_start.min(cursor_pos);
                                    let end = vi_visual_start.max(cursor_pos);
                                    let deleted: String =
                                        input.chars().skip(start).take(end - start).collect();
                                    push_undo(&input, cursor_pos);
                                    vi_last_change = Some(ViChange::Delete(start, end));
                                    kill_ring_push(&deleted);
                                    let byte_start = char_to_byte(&input, start);
                                    let byte_end = char_to_byte(&input, end);
                                    input.drain(byte_start..byte_end);
                                    cursor_pos = start.min(input.chars().count());
                                    mode = EditorMode::ViNormal;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                    continue;
                                }
                                KeyCode::Char('y')
                                    if !modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    let start = vi_visual_start.min(cursor_pos);
                                    let end = vi_visual_start.max(cursor_pos);
                                    let yanked: String =
                                        input.chars().skip(start).take(end - start).collect();
                                    kill_ring_push(&yanked);
                                    mode = EditorMode::ViNormal;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                    continue;
                                }
                                KeyCode::Char('u')
                                    if !modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    let start = vi_visual_start.min(cursor_pos);
                                    let end = vi_visual_start.max(cursor_pos);
                                    let byte_start = char_to_byte(&input, start);
                                    let byte_end = char_to_byte(&input, end);
                                    let selected: String =
                                        input[byte_start..byte_end].to_lowercase();
                                    push_undo(&input, cursor_pos);
                                    input.replace_range(byte_start..byte_end, &selected);
                                    mode = EditorMode::ViNormal;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                    continue;
                                }
                                KeyCode::Char('U')
                                    if !modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    let start = vi_visual_start.min(cursor_pos);
                                    let end = vi_visual_start.max(cursor_pos);
                                    let byte_start = char_to_byte(&input, start);
                                    let byte_end = char_to_byte(&input, end);
                                    let selected: String =
                                        input[byte_start..byte_end].to_uppercase();
                                    push_undo(&input, cursor_pos);
                                    input.replace_range(byte_start..byte_end, &selected);
                                    mode = EditorMode::ViNormal;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                    continue;
                                }
                                _ => {}
                            }
                        }
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
                                if cursor_pos < input.chars().count() {
                                    cursor_pos += 1;
                                }
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
                                let delims: Vec<char> =
                                    editor_cfg.word_delimiters.chars().collect();
                                let mut pos = cursor_pos;
                                while pos < chars.len() && delims.contains(&chars[pos]) {
                                    pos += 1;
                                }
                                while pos < chars.len() && !delims.contains(&chars[pos]) {
                                    pos += 1;
                                }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('W') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                let mut pos = cursor_pos;
                                while pos < chars.len() && chars[pos].is_whitespace() {
                                    pos += 1;
                                }
                                while pos < chars.len() && !chars[pos].is_whitespace() {
                                    pos += 1;
                                }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('B') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                // vi B — move back to start of previous WORD (whitespace-delimited)
                                let chars: Vec<char> = input.chars().collect();
                                let mut pos = cursor_pos;
                                while pos > 0 && chars[pos - 1].is_whitespace() {
                                    pos -= 1;
                                }
                                while pos > 0 && !chars[pos - 1].is_whitespace() {
                                    pos -= 1;
                                }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('b') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                let delims: Vec<char> =
                                    editor_cfg.word_delimiters.chars().collect();
                                let mut pos = cursor_pos;
                                while pos > 0 && delims.contains(&chars[pos - 1]) {
                                    pos -= 1;
                                }
                                while pos > 0 && !delims.contains(&chars[pos - 1]) {
                                    pos -= 1;
                                }
                                cursor_pos = pos;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('e') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                let delims: Vec<char> =
                                    editor_cfg.word_delimiters.chars().collect();
                                let mut pos = cursor_pos;
                                if pos < chars.len() {
                                    while pos < chars.len() && delims.contains(&chars[pos]) {
                                        pos += 1;
                                    }
                                    if pos < chars.len() {
                                        while pos + 1 < chars.len()
                                            && !delims.contains(&chars[pos + 1])
                                        {
                                            pos += 1;
                                        }
                                        cursor_pos = pos;
                                    }
                                }
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
                            KeyCode::Char('~') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let chars: Vec<char> = input.chars().collect();
                                if cursor_pos < chars.len() {
                                    let old = chars[cursor_pos];
                                    let new = if old.is_uppercase() {
                                        old.to_lowercase().next().unwrap_or(old)
                                    } else if old.is_lowercase() {
                                        old.to_uppercase().next().unwrap_or(old)
                                    } else {
                                        old
                                    };
                                    if old != new {
                                        push_undo(&input, cursor_pos);
                                        remove_char_at(&mut input, cursor_pos);
                                        insert_char_at(&mut input, cursor_pos, new);
                                    }
                                    cursor_pos += 1;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('d')
                                if !modifiers.contains(KeyModifiers::CONTROL)
                                    && last_vi_action.as_deref() != Some("d") =>
                            {
                                last_vi_action = Some("d".to_string());
                            }
                            KeyCode::Char('d') if last_vi_action.as_deref() == Some("d") => {
                                let byte_cursor = char_to_byte(&input, cursor_pos);
                                let line_end_char = input
                                    .char_indices()
                                    .skip(byte_cursor)
                                    .find(|&(_, c)| c == '\n')
                                    .map(|(byte, _)| input[..byte].chars().count())
                                    .unwrap_or(input.chars().count());
                                let killed: String = input
                                    .chars()
                                    .skip(cursor_pos)
                                    .take(line_end_char - cursor_pos)
                                    .collect();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                let byte_end = char_to_byte(&input, line_end_char);
                                input.drain(byte_cursor..byte_end);
                                last_vi_action = None;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('D') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let killed: String =
                                    input[char_to_byte(&input, cursor_pos)..].to_string();
                                push_undo(&input, cursor_pos);
                                kill_ring_push(&killed);
                                input.truncate(char_to_byte(&input, cursor_pos));
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('y')
                                if !modifiers.contains(KeyModifiers::CONTROL)
                                    && last_vi_action.as_deref() != Some("y") =>
                            {
                                last_vi_action = Some("y".to_string());
                            }
                            KeyCode::Char('y') if last_vi_action.as_deref() == Some("y") => {
                                // yy — yank the current line (whole buffer in shell editing)
                                kill_ring_push(&input);
                                last_vi_action = None;
                            }
                            KeyCode::Char('p') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(yanked) = kill_ring_yank() {
                                    push_undo(&input, cursor_pos);
                                    vi_last_change = Some(ViChange::Insert(yanked.clone()));
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
                                let old_text = input.clone();
                                vi_last_change =
                                    Some(ViChange::Change(0, input.chars().count(), old_text));
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                input.clear();
                                cursor_pos = 0;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('v') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if mode == EditorMode::ViVisual {
                                    mode = EditorMode::ViNormal;
                                } else {
                                    vi_visual_start = cursor_pos;
                                    mode = EditorMode::ViVisual;
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('/')
                                if mode == EditorMode::ViNormal
                                    && !modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                let original = input.clone();
                                let mut search = String::new();
                                loop {
                                    {
                                        let ctx = EditorRenderCtx {
                                            prompt: Cow::Owned(PromptDisplay {
                                                lines_above: vec![],
                                                input_prefix: "/".to_string(),
                                                lines_below: vec![],
                                                right_prompt: String::new(),
                                                right_prompt_color: String::new(),
                                                right_prompt_hide_threshold: 0.0,
                                            }),
                                            autosuggest_cfg,
                                            colorize: editor_cfg.colorize_output,
                                            colors: colors_cfg,
                                        };
                                        redraw(&ctx, &search, search.len(), "")?;
                                    }
                                    if let Ok(Event::Key(ev)) = event::read() {
                                        match ev.code {
                                            KeyCode::Char('c')
                                                if ev.modifiers.contains(KeyModifiers::CONTROL) =>
                                            {
                                                search.clear();
                                                break;
                                            }
                                            KeyCode::Char(c) => {
                                                search.push(c);
                                            }
                                            KeyCode::Esc => {
                                                search.clear();
                                                break;
                                            }
                                            KeyCode::Enter => break,
                                            KeyCode::Backspace => {
                                                search.pop();
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                if !search.is_empty() {
                                    let hist = HISTORY_CB.get().map(|cb| cb()).unwrap_or_default();
                                    let found = if vi_search_forward {
                                        hist.iter().rev().find(|l| l.contains(&search)).cloned()
                                    } else {
                                        hist.iter().find(|l| l.contains(&search)).cloned()
                                    };
                                    if let Some(matched) = found {
                                        input = matched;
                                        cursor_pos = input.chars().count();
                                        vi_search_pattern = Some(search);
                                    } else {
                                        input = original;
                                        cursor_pos = input.chars().count();
                                    }
                                } else {
                                    input = original;
                                    cursor_pos = input.chars().count();
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('?')
                                if mode == EditorMode::ViNormal
                                    && !modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                let original = input.clone();
                                let mut search = String::new();
                                loop {
                                    {
                                        let ctx = EditorRenderCtx {
                                            prompt: Cow::Owned(PromptDisplay {
                                                lines_above: vec![],
                                                input_prefix: "?".to_string(),
                                                lines_below: vec![],
                                                right_prompt: String::new(),
                                                right_prompt_color: String::new(),
                                                right_prompt_hide_threshold: 0.0,
                                            }),
                                            autosuggest_cfg,
                                            colorize: editor_cfg.colorize_output,
                                            colors: colors_cfg,
                                        };
                                        redraw(&ctx, &search, search.len(), "")?;
                                    }
                                    if let Ok(Event::Key(ev)) = event::read() {
                                        match ev.code {
                                            KeyCode::Char('c')
                                                if ev.modifiers.contains(KeyModifiers::CONTROL) =>
                                            {
                                                search.clear();
                                                break;
                                            }
                                            KeyCode::Char(c) => {
                                                search.push(c);
                                            }
                                            KeyCode::Esc => {
                                                search.clear();
                                                break;
                                            }
                                            KeyCode::Enter => break,
                                            KeyCode::Backspace => {
                                                search.pop();
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                if !search.is_empty() {
                                    let hist = HISTORY_CB.get().map(|cb| cb()).unwrap_or_default();
                                    let found = hist.iter().find(|l| l.contains(&search)).cloned();
                                    if let Some(matched) = found {
                                        input = matched;
                                        cursor_pos = input.chars().count();
                                        vi_search_pattern = Some(search);
                                        vi_search_forward = false;
                                    } else {
                                        input = original;
                                        cursor_pos = input.chars().count();
                                    }
                                } else {
                                    input = original;
                                    cursor_pos = input.chars().count();
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('n')
                                if mode == EditorMode::ViNormal
                                    && !modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                if let Some(ref pat) = vi_search_pattern.clone() {
                                    let hist = HISTORY_CB.get().map(|cb| cb()).unwrap_or_default();
                                    let found = if vi_search_forward {
                                        hist.iter()
                                            .rev()
                                            .find(|l| l.contains(pat.as_str()))
                                            .cloned()
                                    } else {
                                        hist.iter().find(|l| l.contains(pat.as_str())).cloned()
                                    };
                                    if let Some(matched) = found {
                                        input = matched;
                                        cursor_pos = input.chars().count();
                                    }
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('N')
                                if mode == EditorMode::ViNormal
                                    && !modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                if let Some(ref pat) = vi_search_pattern.clone() {
                                    let hist = HISTORY_CB.get().map(|cb| cb()).unwrap_or_default();
                                    let found = if !vi_search_forward {
                                        hist.iter()
                                            .rev()
                                            .find(|l| l.contains(pat.as_str()))
                                            .cloned()
                                    } else {
                                        hist.iter().find(|l| l.contains(pat.as_str())).cloned()
                                    };
                                    if let Some(matched) = found {
                                        input = matched;
                                        cursor_pos = input.chars().count();
                                    }
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('J')
                                if mode == EditorMode::ViNormal
                                    && !modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                if let Some(nl_pos) =
                                    input[char_to_byte(&input, cursor_pos)..].find('\n')
                                {
                                    let byte_pos = char_to_byte(&input, cursor_pos);
                                    let end = byte_pos + nl_pos;
                                    let next_nl = input[end + 1..]
                                        .find('\n')
                                        .map(|p| end + 1 + p)
                                        .unwrap_or(input.len());
                                    let removed: String = input[end..next_nl].chars().collect();
                                    let stripped = removed.trim_start();
                                    let replace = if stripped.starts_with('\\') { "" } else { " " };
                                    push_undo(&input, cursor_pos);
                                    input.replace_range(
                                        char_to_byte(&input, cursor_pos)..next_nl,
                                        replace,
                                    );
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Enter if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if is_input_incomplete(&input) {
                                    if input.trim_end().ends_with('\\') {
                                        let last_nl = input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                        let last_line = &input[last_nl..];
                                        let trimmed = last_line.trim_end();
                                        let mut bs_count = 0usize;
                                        for ch in trimmed.chars().rev() {
                                            if ch == '\\' {
                                                bs_count += 1;
                                            } else {
                                                break;
                                            }
                                        }
                                        if bs_count % 2 == 1 {
                                            let strip = trimmed.len() - bs_count;
                                            input = format!(
                                                "{}{}",
                                                &input[..last_nl + strip],
                                                &input[last_nl + trimmed.len()..]
                                            );
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
                            KeyCode::Char(c)
                                if !modifiers.contains(KeyModifiers::CONTROL)
                                    && c.is_ascii_digit()
                                    && last_vi_action.as_deref() == Some("d") =>
                            {
                                if c == 'd' {
                                    let byte_cursor = char_to_byte(&input, cursor_pos);
                                    let line_start = input[..byte_cursor]
                                        .rfind('\n')
                                        .map(|p| p + 1)
                                        .unwrap_or(0);
                                    let byte_end = input[byte_cursor..]
                                        .find('\n')
                                        .map(|p| byte_cursor + p)
                                        .unwrap_or(input.len());
                                    let killed: String = input[line_start..byte_end].to_string();
                                    push_undo(&input, cursor_pos);
                                    kill_ring_push(&killed);
                                    let new_cursor_char = input[..line_start].chars().count();
                                    input.drain(line_start..byte_end);
                                    cursor_pos = new_cursor_char;
                                    last_vi_action = None;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                } else {
                                    last_vi_action = None;
                                }
                            }
                            KeyCode::Char('G') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                cursor_pos = input.chars().count();
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('g')
                                if !modifiers.contains(KeyModifiers::CONTROL)
                                    && last_vi_action.as_deref() == Some("g") =>
                            {
                                cursor_pos = 0;
                                last_vi_action = None;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('g') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                last_vi_action = Some("g".to_string());
                            }
                            KeyCode::Char('f') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Ok(next_ev) = event::read()
                                    && let Event::Key(KeyEvent {
                                        code: KeyCode::Char(c),
                                        ..
                                    }) = next_ev
                                {
                                    let chars: Vec<char> = input.chars().collect();
                                    if let Some(rel) =
                                        chars[cursor_pos + 1..].iter().position(|&ch| ch == c)
                                    {
                                        cursor_pos += 1 + rel;
                                    }
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('F') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Ok(next_ev) = event::read()
                                    && let Event::Key(KeyEvent {
                                        code: KeyCode::Char(c),
                                        ..
                                    }) = next_ev
                                {
                                    let chars: Vec<char> = input.chars().collect();
                                    if cursor_pos > 0
                                        && let Some(rel) =
                                            chars[..cursor_pos].iter().rev().position(|&ch| ch == c)
                                    {
                                        cursor_pos -= 1 + rel;
                                    }
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('t') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Ok(next_ev) = event::read()
                                    && let Event::Key(KeyEvent {
                                        code: KeyCode::Char(c),
                                        ..
                                    }) = next_ev
                                {
                                    let chars: Vec<char> = input.chars().collect();
                                    if let Some(rel) =
                                        chars[cursor_pos + 1..].iter().position(|&ch| ch == c)
                                    {
                                        cursor_pos += rel;
                                    }
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('T') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Ok(next_ev) = event::read()
                                    && let Event::Key(KeyEvent {
                                        code: KeyCode::Char(c),
                                        ..
                                    }) = next_ev
                                {
                                    let chars: Vec<char> = input.chars().collect();
                                    if cursor_pos > 0
                                        && let Some(rel) =
                                            chars[..cursor_pos].iter().rev().position(|&ch| ch == c)
                                    {
                                        cursor_pos = cursor_pos.saturating_sub(rel);
                                    }
                                }
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('C') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                push_undo(&input, cursor_pos);
                                vi_last_change =
                                    Some(ViChange::Delete(cursor_pos, input.chars().count()));
                                let killed: String = input.chars().skip(cursor_pos).collect();
                                kill_ring_push(&killed);
                                input.truncate(char_to_byte(&input, cursor_pos));
                                mode = EditorMode::ViInsert;
                                switched_to_insert = true;
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('Y') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                let line_start_byte = input[..char_to_byte(&input, cursor_pos)]
                                    .rfind('\n')
                                    .map(|p| p + 1)
                                    .unwrap_or(0);
                                let line_end_byte = input[char_to_byte(&input, cursor_pos)..]
                                    .find('\n')
                                    .map(|p| char_to_byte(&input, cursor_pos) + p)
                                    .unwrap_or(input.len());
                                let killed: String =
                                    input[line_start_byte..line_end_byte].to_string();
                                kill_ring_push(&killed);
                                redraw(&rctx, &input, cursor_pos, &suggestion)?;
                            }
                            KeyCode::Char('P') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(yanked) = kill_ring_yank() {
                                    push_undo(&input, cursor_pos);
                                    let before: String = input.chars().take(cursor_pos).collect();
                                    let after: String = input.chars().skip(cursor_pos).collect();
                                    input = format!("{}{}{}", before, yanked, after);
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                            }
                            KeyCode::Char('.') if !modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Some(ref change) = vi_last_change.clone() {
                                    match change {
                                        ViChange::Insert(s) => {
                                            push_undo(&input, cursor_pos);
                                            for ch in s.chars() {
                                                insert_char_at(&mut input, cursor_pos, ch);
                                                cursor_pos += 1;
                                            }
                                        }
                                        ViChange::Delete(from, to) => {
                                            let from_byte = char_to_byte(&input, *from);
                                            let to_byte = char_to_byte(&input, *to);
                                            if from_byte < to_byte && to_byte <= input.len() {
                                                push_undo(&input, cursor_pos);
                                                input.drain(from_byte..to_byte);
                                                if cursor_pos > input.chars().count() {
                                                    cursor_pos = input.chars().count();
                                                }
                                            }
                                        }
                                        ViChange::Change(from, to, s) => {
                                            let from_byte = char_to_byte(&input, *from);
                                            let to_byte = char_to_byte(&input, *to);
                                            push_undo(&input, cursor_pos);
                                            if from_byte < to_byte && to_byte <= input.len() {
                                                input.drain(from_byte..to_byte);
                                            }
                                            let insert_pos = from;
                                            let mut pos = *insert_pos;
                                            for ch in s.chars() {
                                                insert_char_at(&mut input, pos, ch);
                                                pos += 1;
                                            }
                                            cursor_pos = pos;
                                            mode = EditorMode::ViInsert;
                                            switched_to_insert = true;
                                        }
                                    }
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
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
                                    EditorMode::ViInsert => {
                                        hex_to_ansi(&prompt_cfg.vi_cmd_color_success)
                                    }
                                    EditorMode::ViNormal => hex_to_ansi(&prompt_cfg.vi_cmd_color),
                                    EditorMode::ViVisual => {
                                        hex_to_ansi(&prompt_cfg.vi_cmd_color_error)
                                    }
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
                            exec_widget(
                                widget,
                                &mut input,
                                &mut cursor_pos,
                                history,
                                &mut history_offset,
                                &mut temp_buf,
                                &editor_cfg.word_delimiters,
                                clipboard_cfg,
                            );
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
                                } else if matches_key_event(
                                    code,
                                    modifiers,
                                    &autosuggest_cfg.accept_word_key,
                                ) {
                                    let suffix = suggestion;
                                    let chars: Vec<char> = input.chars().collect();
                                    let after: String = chars[cursor_pos..].iter().collect();
                                    let delimiters: Vec<char> =
                                        editor_cfg.word_delimiters.chars().collect();
                                    let mut word_end = 0;
                                    let suffix_chars: Vec<char> = suffix.chars().collect();
                                    while word_end < suffix_chars.len() {
                                        if delimiters.contains(&suffix_chars[word_end])
                                            && word_end > 0
                                        {
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
                                            let last_nl =
                                                input.rfind('\n').map(|p| p + 1).unwrap_or(0);
                                            let last_line = &input[last_nl..];
                                            let trimmed = last_line.trim_end();
                                            let mut bs_count = 0usize;
                                            for ch in trimmed.chars().rev() {
                                                if ch == '\\' {
                                                    bs_count += 1;
                                                } else {
                                                    break;
                                                }
                                            }
                                            if bs_count % 2 == 1 {
                                                let strip = trimmed.len() - bs_count;
                                                input = format!(
                                                    "{}{}",
                                                    &input[..last_nl + strip],
                                                    &input[last_nl + trimmed.len()..]
                                                );
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
                                            input_prefix: if from_env {
                                                String::new()
                                            } else {
                                                ps2
                                            },
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
                                KeyCode::Char('t')
                                    if modifiers.contains(KeyModifiers::CONTROL)
                                        && editor_cfg.emacs_overwrite_mode =>
                                {
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
                                        crate::shell::signals::SHOULD_EXIT
                                            .store(true, std::sync::atomic::Ordering::SeqCst);
                                        crate::shell::signals::EXIT_CODE
                                            .store(0, std::sync::atomic::Ordering::SeqCst);
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
                                    let killed: String =
                                        input[char_to_byte(&input, cursor_pos)..].to_string();
                                    push_undo(&input, cursor_pos);
                                    kill_ring_push(&killed);
                                    input.truncate(char_to_byte(&input, cursor_pos));
                                    history_offset = None;
                                    suggestion.clear();
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                                KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                                    let killed: String =
                                        input[..char_to_byte(&input, cursor_pos)].to_string();
                                    push_undo(&input, cursor_pos);
                                    kill_ring_push(&killed);
                                    let tail: String =
                                        input[char_to_byte(&input, cursor_pos)..].to_string();
                                    input.clear();
                                    input.push_str(&tail);
                                    cursor_pos = 0;
                                    history_offset = None;
                                    suggestion.clear();
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                                KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                                    let chars: Vec<char> = input.chars().collect();
                                    let mut new_pos = cursor_pos;
                                    while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
                                        new_pos -= 1;
                                    }
                                    while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
                                        new_pos -= 1;
                                    }
                                    if new_pos < cursor_pos {
                                        let byte_start = char_to_byte(&input, new_pos);
                                        let byte_end = char_to_byte(&input, cursor_pos);
                                        let killed: String =
                                            input[byte_start..byte_end].to_string();
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
                                        let before: String =
                                            input.chars().take(cursor_pos).collect();
                                        let after: String =
                                            input.chars().skip(cursor_pos).collect();
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
                                KeyCode::Right
                                    if !modifiers.contains(KeyModifiers::CONTROL)
                                        && cursor_pos < input.chars().count() =>
                                {
                                    cursor_pos += 1;
                                    redraw(&rctx, &input, cursor_pos, &suggestion)?;
                                }
                                KeyCode::Char('f') if modifiers.contains(KeyModifiers::ALT) => {
                                    if cursor_pos < input.chars().count() {
                                        let delimiters: Vec<char> =
                                            editor_cfg.word_delimiters.chars().collect();
                                        let chars: Vec<char> = input.chars().collect();
                                        let mut pos = cursor_pos;
                                        while pos < chars.len() && delimiters.contains(&chars[pos])
                                        {
                                            pos += 1;
                                        }
                                        while pos < chars.len() && !delimiters.contains(&chars[pos])
                                        {
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
                                    let prompt_prefix = rctx.prompt.input_prefix.clone();
                                    print!("\x1b[2K\r{}(reverse-i-search)`': ", prompt_prefix);
                                    io::stdout().flush()?;
                                    let mut search_buf = String::new();
                                    let mut cur_idx: Option<usize> = None;
                                    let find_from = |needle: &str, end: usize| -> Option<usize> {
                                        if needle.is_empty() {
                                            return None;
                                        }
                                        (0..end.min(history.len())).rev().find(|&i| {
                                            let h = &history[i];
                                            if history_cfg.search_case_sensitive {
                                                h.contains(needle)
                                            } else {
                                                h.to_lowercase().contains(&needle.to_lowercase())
                                            }
                                        })
                                    };
                                    let render_search =
                                        |prefix: &str,
                                         query: &str,
                                         matched: Option<&str>,
                                         failed: bool|
                                         -> io::Result<()> {
                                            let label = if failed { "failed " } else { "" };
                                            print!(
                                                "\x1b[2K\r{}({}reverse-i-search)`{}': ",
                                                prefix, label, query
                                            );
                                            if let Some(line) = matched {
                                                print!(
                                                    "{}",
                                                    highlight_line(
                                                        line,
                                                        editor_cfg.colorize_output,
                                                        colors_cfg
                                                    )
                                                );
                                            }
                                            io::stdout().flush()
                                        };
                                    loop {
                                        let ev = match event::read() {
                                            Ok(e) => e,
                                            Err(_) => {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(10),
                                                );
                                                continue;
                                            }
                                        };
                                        if let Event::Key(KeyEvent {
                                            code: sc,
                                            modifiers: sm,
                                            ..
                                        }) = ev
                                        {
                                            match sc {
                                                KeyCode::Char(c)
                                                    if !sm.contains(KeyModifiers::CONTROL) =>
                                                {
                                                    search_buf.push(c);
                                                    cur_idx = find_from(&search_buf, history.len());
                                                    render_search(
                                                        &prompt_prefix,
                                                        &search_buf,
                                                        cur_idx.map(|i| history[i].as_str()),
                                                        false,
                                                    )?;
                                                }
                                                KeyCode::Char('r')
                                                    if sm.contains(KeyModifiers::CONTROL) =>
                                                {
                                                    let start = cur_idx.unwrap_or(history.len());
                                                    if start > 0
                                                        && let Some(idx) =
                                                            find_from(&search_buf, start)
                                                    {
                                                        cur_idx = Some(idx);
                                                        render_search(
                                                            &prompt_prefix,
                                                            &search_buf,
                                                            Some(history[idx].as_str()),
                                                            false,
                                                        )?;
                                                    } else {
                                                        render_search(
                                                            &prompt_prefix,
                                                            &search_buf,
                                                            None,
                                                            true,
                                                        )?;
                                                    }
                                                }
                                                KeyCode::Backspace => {
                                                    search_buf.pop();
                                                    cur_idx = find_from(&search_buf, history.len());
                                                    render_search(
                                                        &prompt_prefix,
                                                        &search_buf,
                                                        cur_idx.map(|i| history[i].as_str()),
                                                        false,
                                                    )?;
                                                }
                                                KeyCode::Enter => {
                                                    if is_input_incomplete(&input) {
                                                        if input.trim_end().ends_with('\\') {
                                                            let last_nl = input
                                                                .rfind('\n')
                                                                .map(|p| p + 1)
                                                                .unwrap_or(0);
                                                            let last_line = &input[last_nl..];
                                                            let trimmed = last_line.trim_end();
                                                            let mut bs_count = 0usize;
                                                            for ch in trimmed.chars().rev() {
                                                                if ch == '\\' {
                                                                    bs_count += 1;
                                                                } else {
                                                                    break;
                                                                }
                                                            }
                                                            if bs_count % 2 == 1 {
                                                                let strip =
                                                                    trimmed.len() - bs_count;
                                                                input = format!(
                                                                    "{}{}",
                                                                    &input[..last_nl + strip],
                                                                    &input
                                                                        [last_nl + trimmed.len()..]
                                                                );
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
                                                            input_prefix: if from_env {
                                                                String::new()
                                                            } else {
                                                                ps2
                                                            },
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
                                                    break;
                                                }
                                                KeyCode::Char('g')
                                                    if sm.contains(KeyModifiers::CONTROL) =>
                                                {
                                                    input = saved_input;
                                                    cursor_pos = saved_cursor;
                                                    rctx.prompt = Cow::Borrowed(prompt);
                                                    break;
                                                }
                                                KeyCode::Char('c')
                                                    if sm.contains(KeyModifiers::CONTROL) =>
                                                {
                                                    input = saved_input;
                                                    cursor_pos = saved_cursor;
                                                    rctx.prompt = Cow::Borrowed(prompt);
                                                    print!("^C\r\n");
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    redraw(&rctx, &input, cursor_pos, "")?;
                                }
                                KeyCode::Tab => {
                                    let completions =
                                        crate::shell::builtin::get_completions(&input, cursor_pos);
                                    if completions.len() == 1 {
                                        let ws = find_word_start(&input, cursor_pos);
                                        let byte_start = char_to_byte(&input, ws);
                                        let byte_end = char_to_byte(&input, cursor_pos);
                                        input.replace_range(byte_start..byte_end, &completions[0]);
                                        cursor_pos = ws + completions[0].chars().count();
                                        history_offset = None;
                                        redraw(&rctx, &input, cursor_pos, "")?;
                                    } else if completions.len() > 1 {
                                        let common = tab_common_prefix(&completions);
                                        if !common.is_empty() {
                                            let ws = find_word_start(&input, cursor_pos);
                                            let byte_start = char_to_byte(&input, ws);
                                            let byte_end = char_to_byte(&input, cursor_pos);
                                            input.replace_range(byte_start..byte_end, &common);
                                            cursor_pos = ws + common.chars().count();
                                        }
                                        terminal_bell(&editor_cfg.bell);
                                        history_offset = None;
                                        redraw(&rctx, &input, cursor_pos, "")?;
                                    } else {
                                        terminal_bell(&editor_cfg.bell);
                                    }
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
                                        && *idx < history.len().saturating_sub(1)
                                    {
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
                                            let hi = history.len().saturating_sub(
                                                1 + history_offset.expect("history_offset Some"),
                                            );
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
                                    if !suggestion.is_empty() && cursor_pos == input.chars().count()
                                    {
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
                                        let next_ch = if at_end {
                                            Some(chars[cursor_pos])
                                        } else {
                                            None
                                        };
                                        if editor_cfg.auto_match_quotes
                                            && ((ch == '"' && next_ch == Some('"'))
                                                || (ch == '\'' && next_ch == Some('\'')))
                                        {
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
                                    if input.chars().count() >= editor_cfg.max_line_length as usize
                                    {
                                        terminal_bell(&editor_cfg.bell);
                                        continue;
                                    }
                                    if overwrite_mode && cursor_pos < input.chars().count() {
                                        push_undo(&input, cursor_pos);
                                        let byte = char_to_byte(&input, cursor_pos);
                                        input.remove(byte);
                                        input.insert(byte, c);
                                        cursor_pos += 1;
                                    } else if editor_cfg.auto_match_quotes
                                        && (c == '"' || c == '\'')
                                    {
                                        push_undo(&input, cursor_pos);
                                        if input.chars().nth(cursor_pos) == Some(c) {
                                            // Typing the closing quote skips over
                                            // the auto-inserted matching quote.
                                            cursor_pos += 1;
                                        } else {
                                            insert_char_at(&mut input, cursor_pos, c);
                                            cursor_pos += 1;
                                            insert_char_at(&mut input, cursor_pos, c);
                                        }
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

/// Move the physical cursor to `cursor_pos` characters into `input`,
/// computed in display columns (wide chars count double) and honoring
/// terminal line wrapping.
fn position_cursor(prefix: &str, input: &str, cursor_pos: usize, suggestion: &str) {
    let w = crate::terminal::prompt::get_terminal_width().max(1);
    let prefix_vis = crate::terminal::color::visible_len(prefix);
    let before: String = input.chars().take(cursor_pos).collect();
    let cursor_cols = prefix_vis + crate::terminal::color::visible_len(&before);
    let total_cols = prefix_vis
        + crate::terminal::color::visible_len(input)
        + crate::terminal::color::visible_len(suggestion);
    let end_row = total_cols / w;
    let target_row = cursor_cols / w;
    if end_row > target_row {
        print!("\x1b[{}A", end_row - target_row);
    }
    print!("\r");
    let col = cursor_cols % w;
    if col > 0 {
        print!("\x1b[{}C", col);
    }
}

fn render_display(
    context: &EditorRenderCtx,
    input: &str,
    cursor_pos: usize,
    suggestion: &str,
) -> io::Result<()> {
    print!("\x1b[?25l");
    if context.prompt.lines_above.is_empty() {
        print!("\r\x1b[2K");
    }
    for line in &context.prompt.lines_above {
        print!("{}\r\n", line);
    }
    print!("{}", context.prompt.input_prefix);
    print!(
        "{}",
        highlight_line(input, context.colorize, context.colors)
    );
    if !suggestion.is_empty() {
        let color = color_to_ansi(&context.autosuggest_cfg.highlight_color);
        print!("{}{}\x1b[0m", color, suggestion);
    }

    if !context.prompt.right_prompt.is_empty() {
        let term_width = crate::terminal::prompt::get_terminal_width();
        let input_vis = crate::terminal::color::visible_len(input)
            + crate::terminal::color::visible_len(suggestion);
        let rprompt_vis = crate::terminal::color::visible_len(&context.prompt.right_prompt);
        let prefix_vis = crate::terminal::color::visible_len(&context.prompt.input_prefix);
        let threshold = (term_width as f64 * context.prompt.right_prompt_hide_threshold) as usize;
        let occupied = prefix_vis + input_vis;

        if occupied < threshold {
            let padding = if term_width > prefix_vis + input_vis + rprompt_vis {
                term_width - prefix_vis - input_vis - rprompt_vis
            } else {
                0
            };
            print!(
                "{}{}{}\x1b[0m",
                " ".repeat(padding),
                color_to_ansi(&context.prompt.right_prompt_color),
                context.prompt.right_prompt
            );
        }
    }

    position_cursor(&context.prompt.input_prefix, input, cursor_pos, suggestion);
    if !context.prompt.lines_below.is_empty() {
        print!("\r\n");
        for line in &context.prompt.lines_below {
            print!("{}\r\n", line);
        }
    }
    io::stdout().flush()
}

fn redraw(
    context: &EditorRenderCtx,
    input: &str,
    cursor_pos: usize,
    suggestion: &str,
) -> io::Result<()> {
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
    print!(
        "{}",
        highlight_line(input, context.colorize, context.colors)
    );
    if !suggestion.is_empty() {
        let color = color_to_ansi(&context.autosuggest_cfg.highlight_color);
        print!("{}{}\x1b[0m", color, suggestion);
    }

    if !context.prompt.right_prompt.is_empty() {
        let term_width = crate::terminal::prompt::get_terminal_width();
        let input_vis = crate::terminal::color::visible_len(input)
            + crate::terminal::color::visible_len(suggestion);
        let rprompt_vis = crate::terminal::color::visible_len(&context.prompt.right_prompt);
        let prefix_vis = crate::terminal::color::visible_len(&context.prompt.input_prefix);
        let threshold = (term_width as f64 * context.prompt.right_prompt_hide_threshold) as usize;
        let occupied = prefix_vis + input_vis;

        if occupied < threshold {
            let padding = if term_width > prefix_vis + input_vis + rprompt_vis {
                term_width - prefix_vis - input_vis - rprompt_vis
            } else {
                0
            };
            print!(
                "{}{}{}\x1b[0m",
                " ".repeat(padding),
                color_to_ansi(&context.prompt.right_prompt_color),
                context.prompt.right_prompt
            );
        }
    }

    if below_count > 0 {
        for line in &context.prompt.lines_below {
            print!("\x1b[2K{}\r\n", line);
        }
        print!("\x1b[{}A", below_count);
    }

    position_cursor(&context.prompt.input_prefix, input, cursor_pos, suggestion);
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

fn starts_with_ci(haystack: &str, needle: &[char]) -> bool {
    let mut h = haystack.chars();
    for &nc in needle {
        match h.next() {
            Some(c) if c.to_ascii_lowercase() == nc => {}
            _ => return false,
        }
    }
    true
}

fn find_ci_char_pos(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle.len() {
        return None;
    }
    'outer: for i in 0..=(haystack.len() - needle.len()) {
        for (j, &nc) in needle.iter().enumerate() {
            if haystack[i + j].to_ascii_lowercase() != nc {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

fn find_suggestion(
    input: &str,
    history: &[String],
    case_sensitive: bool,
    substring: bool,
) -> String {
    if input.is_empty() {
        return String::new();
    }
    let input_lower: Vec<char> = input.chars().map(|c| c.to_ascii_lowercase()).collect();
    for entry in history.iter().rev() {
        let matches = if case_sensitive {
            entry.starts_with(input)
        } else {
            starts_with_ci(entry, &input_lower)
        };
        if matches && entry != input {
            let char_count = input_lower.len();
            return entry
                .char_indices()
                .nth(char_count)
                .map_or(String::new(), |(i, _)| entry[i..].to_string());
        }
    }
    if substring {
        for entry in history.iter().rev() {
            if entry == input {
                continue;
            }
            if case_sensitive {
                if let Some(byte_pos) = entry.find(input) {
                    return entry[byte_pos..].to_string();
                }
            } else {
                let entry_chars: Vec<char> = entry.chars().collect();
                if let Some(char_pos) = find_ci_char_pos(&entry_chars, &input_lower) {
                    let byte_pos = entry
                        .char_indices()
                        .nth(char_pos)
                        .map(|(i, _)| i)
                        .unwrap_or(entry.len());
                    return entry[byte_pos..].to_string();
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
