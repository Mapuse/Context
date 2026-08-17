use std::cell::{Cell, RefCell};
use crate::shell::env::Env;

pub struct Expander<'a> {
    env: &'a mut Env,
    last_status: i32,
    positional: Vec<String>,
    background_pid: i32,
    pending_sets: RefCell<Vec<(String, String)>>,
    nounset: bool,
    had_nounset_error: Cell<bool>,
}

impl<'a> Expander<'a> {
    pub fn new(env: &'a mut Env, last_status: i32, positional: Vec<String>, background_pid: i32) -> Self {
        let positional = if positional.is_empty() {
            env.positional().to_vec()
        } else {
            positional
        };
        Self {
            env,
            last_status,
            positional,
            background_pid,
            pending_sets: RefCell::new(Vec::new()),
            nounset: false,
            had_nounset_error: Cell::new(false),
        }
    }

    pub fn expand_word(&mut self, word: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = word.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let ch = chars[i];
            match ch {
                '\x02' => {
                    i += 1;
                    let start = i;
                    while i < len && chars[i] != '\x02' { i += 1; }
                    result.push('\x02');
                    result.push_str(&word[start..i]);
                    result.push('\x02');
                    if i < len { i += 1; }
                }
                '\x01' => {
                    i += 1;
                    result.push('\x01');
                    while i < len && chars[i] != '\x01' {
                        if chars[i] == '$' && i + 1 < len && chars[i + 1] == '@' {
                            i += 2;
                            for (j, param) in self.positional.iter().enumerate() {
                                if j > 0 { result.push(' '); }
                                result.push('\x01');
                                result.push_str(param);
                                result.push('\x01');
                            }
                        } else if chars[i] == '$' && i + 1 < len && chars[i + 1] == '*' {
                            i += 2;
                            let ifs = self.env.get("IFS").unwrap_or(" \t\n");
                            let sep = ifs.chars().next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string());
                            result.push_str(&self.positional.join(&sep));
                        } else if chars[i] == '$' && i + 1 < len {
                            i += 1;
                            result.push_str(&self.expand_dollar(&chars, &mut i, len));
                        } else if chars[i] == '\\' && i + 1 < len
                            && matches!(chars[i + 1], '$' | '`' | '"' | '\\') {
                                i += 1;
                                result.push(chars[i]);
                                i += 1;
                        } else if chars[i] == '`' {
                            i += 1;
                            let start = i;
                            while i < len && chars[i] != '`' { i += 1; }
                            let inner = &word[start..i];
                            if i < len { i += 1; }
                            let output = self.run_cmd_sub(inner);
                            result.push_str(&output);
                        } else {
                            result.push(chars[i]);
                            i += 1;
                        }
                    }
                    result.push('\x01');
                    if i < len { i += 1; }
                }
                '$' if i + 1 < len => {
                    i += 1;
                    result.push_str(&self.expand_dollar(&chars, &mut i, len));
                }
                '`' => {
                    i += 1;
                    let start = i;
                    while i < len && chars[i] != '`' { i += 1; }
                    let inner = &word[start..i];
                    if i < len { i += 1; }
                    let output = self.run_cmd_sub(inner);
                    result.push_str(&output);
                }
                '~' if i == 0 || matches!(chars[i - 1], ' ' | ':' | '=') => {
                    i += 1;
                    let start = i;
                    while i < len && !matches!(chars[i], '/' | ':' | ' ') { i += 1; }
                    let user_part = &word[start..i];
                    if user_part.is_empty() {
                        result.push_str(&self.env.home());
                    } else if let Some(named) = self.env.get_named_dir(user_part) {
                        result.push_str(named);
                    } else {
                        result.push_str(&resolve_user_home(user_part));
                    }
                }
                '\\' if i + 1 < len => {
                    i += 1;
                    result.push(chars[i]);
                    i += 1;
                }
                _ => {
                    result.push(ch);
                    i += 1;
                }
            }
        }
        result
    }

    fn expand_dollar(&mut self, chars: &[char], i: &mut usize, len: usize) -> String {
        if *i >= len {
            return String::new();
        }
        match chars[*i] {
            '(' => {
                if *i + 1 < len && chars[*i + 1] == '(' {
                    *i += 1;
                    let start = *i + 1;
                    let mut depth = 0u32;
                    let mut j = start;
                    while j < len {
                        match chars[j] {
                            '(' => depth += 1,
                            ')' if depth == 0 => break,
                            ')' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let inner: String = chars[start..j].iter().collect();
                    *i = j + 2;
                    let expanded_inner = self.expand_word(&inner);
                    crate::shell::builtin::eval_arith_assign(&expanded_inner, self.env).to_string()
                } else {
                    let start = *i + 1;
                    let mut depth = 1u32;
                    let mut j = start;
                    while j < len && depth > 0 {
                        match chars[j] {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 { break; }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    let inner: String = chars[start..j].iter().collect();
                    *i = j + 1;
                    self.run_cmd_sub(&inner)
                }
            }
            '{' => {
                *i += 1;
                let start = *i;
                while *i < len && chars[*i] != '}' { *i += 1; }
                let var: String = chars[start..*i].iter().collect();
                if *i < len { *i += 1; }
                self.expand_var(&var)
            }
            '?' => { *i += 1; self.last_status.to_string() }
            '$' => { *i += 1; std::process::id().to_string() }
            '!' => { *i += 1; self.background_pid.to_string() }
            '0'..='9' => {
                let mut n: usize = 0;
                while *i < len && chars[*i].is_ascii_digit() {
                    n = n * 10 + chars[*i].to_digit(10).unwrap() as usize;
                    *i += 1;
                }
                if n == 0 {
                    self.positional.first().cloned().unwrap_or_default()
                } else if n - 1 < self.positional.len() {
                    self.positional[n - 1].clone()
                } else {
                    String::new()
                }
            }
            '@' => {
                *i += 1;
                self.positional.join(" ")
            }
            '*' => {
                *i += 1;
                let ifs = self.env.get("IFS").unwrap_or(" \t\n");
                let sep = ifs.chars().next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string());
                self.positional.join(&sep)
            }
            '#' => { *i += 1; self.positional.len().to_string() }
            '-' => { *i += 1; "-".to_string() }
            '_' => { *i += 1; self.env.get("_").unwrap_or("").to_string() }
            _ => {
                let start = *i;
                while *i < len && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
                    *i += 1;
                }
                if *i > start {
                    let var: String = chars[start..*i].iter().collect();
                    self.expand_var(&var)
                } else {
                    *i += 1;
                    "$".to_string()
                }
            }
        }
    }

    fn run_cmd_sub(&mut self, cmd: &str) -> String {
        use std::os::fd::FromRawFd;
        use std::io::Read;

        let mut fds = [0i32; 2];
        unsafe {
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return String::new();
            }
        }
        let (read_fd, write_fd) = (fds[0], fds[1]);

        match unsafe { libc::fork() } {
            -1 => {
                unsafe { libc::close(read_fd); libc::close(write_fd); }
                String::new()
            }
            0 => {
                unsafe {
                    libc::dup2(write_fd, libc::STDOUT_FILENO);
                    libc::dup2(write_fd, libc::STDERR_FILENO);
                    libc::close(write_fd);
                    libc::close(read_fd);
                }
                crate::shell::signals::setup_child_handlers();
                let tokens = crate::shell::lexer::tokenize(cmd);
                let ast = crate::shell::parser::parse(tokens);
                let env = self.env.clone();
                let cfg = crate::config::loader::load();
                let mut sub_exec = crate::shell::executor::Executor::new(env, cfg);
                let status = sub_exec.execute(&ast);
                std::process::exit(status);
            }
            pid => {
                let output = unsafe {
                    libc::close(write_fd);
                    let mut output = String::new();
                    let mut file = std::fs::File::from_raw_fd(read_fd);
                    let mut buf = [0u8; 4096];
                    const MAX_OUTPUT: usize = 1024 * 1024;
                    loop {
                        match file.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                if output.len() + n > MAX_OUTPUT {
                                    let _ = libc::kill(pid, libc::SIGTERM);
                                    break;
                                }
                                output.push_str(std::str::from_utf8(&buf[..n]).unwrap_or(""));
                            }
                            Err(_) => break,
                        }
                    }
                    libc::waitpid(pid, std::ptr::null_mut(), 0);
                    output
                };
                output.trim_end_matches('\n').to_string()
            }
        }
    }

    pub fn take_pending_sets(&self) -> Vec<(String, String)> {
        self.pending_sets.borrow_mut().drain(..).collect()
    }

    pub fn set_nounset(&mut self, val: bool) { self.nounset = val; }
    pub fn had_nounset_error(&self) -> bool { self.had_nounset_error.get() }

    fn expand_var(&mut self, var: &str) -> String {
        if var.is_empty() {
            return String::new();
        }
        if let Some(name) = var.strip_prefix('!')
            && !name.is_empty() {
                return self.env.get(name).unwrap_or("").to_string();
            }
        if let Some(name) = var.strip_prefix('#') {
            if name.is_empty() {
                return self.positional.len().to_string();
            }
            if name.as_bytes()[0] == b'#' {
                let name = &name[1..];
                let val = self.env.get(name).unwrap_or("").to_string();
                val.len().to_string()
            } else if name.as_bytes()[0] == b'%' {
                let name = &name[1..];
                let val = self.env.get(name).unwrap_or("").to_string();
                val.matches(char::is_alphanumeric).count().to_string()
            } else if self.env.is_assoc_array(name) {
                self.env.assoc_len(name).to_string()
            } else {
                self.env.get(name).unwrap_or("").chars().count().to_string()
            }
        } else if let Some(stripped) = var.strip_suffix("^^") {
            let val = self.env.get(stripped).unwrap_or("").to_string();
            val.to_uppercase()
        } else if let Some(stripped) = var.strip_suffix(",,") {
            let val = self.env.get(stripped).unwrap_or("").to_string();
            val.to_lowercase()
        } else if let Some(stripped) = var.strip_suffix('^') {
            let val = self.env.get(stripped).unwrap_or("").to_string();
            if let Some(first) = val.chars().next() {
                format!("{}{}", first.to_uppercase(), &val[first.len_utf8()..])
            } else {
                val
            }
        } else if let Some(stripped) = var.strip_suffix(',') {
            let val = self.env.get(stripped).unwrap_or("").to_string();
            if let Some(first) = val.chars().next() {
                format!("{}{}", first.to_lowercase(), &val[first.len_utf8()..])
            } else {
                val
            }
        } else if let Some(name) = var.strip_suffix(":u") {
            let val = self.env.get(name).unwrap_or("");
            val.to_uppercase()
        } else if let Some(name) = var.strip_suffix(":l") {
            let val = self.env.get(name).unwrap_or("");
            val.to_lowercase()
        } else if let Some(name) = var.strip_suffix(":r") {
            let val = self.env.get(name).unwrap_or("");
            match val.rfind('.') {
                Some(pos) => val[..pos].to_string(),
                None => val.to_string(),
            }
        } else if let Some(name) = var.strip_suffix(":e") {
            let val = self.env.get(name).unwrap_or("");
            match val.rfind('.') {
                Some(pos) => val[pos + 1..].to_string(),
                None => String::new(),
            }
        } else if let Some(name) = var.strip_suffix(":t") {
            let val = self.env.get(name).unwrap_or("");
            match val.rfind('/') {
                Some(pos) => val[pos + 1..].to_string(),
                None => val.to_string(),
            }
        } else if let Some(name) = var.strip_suffix(":h") {
            let val = self.env.get(name).unwrap_or("");
            match val.rfind('/') {
                Some(pos) if pos > 0 => val[..pos].to_string(),
                _ => "/".to_string(),
            }
        } else if let Some(colon_pos) = var.find(":-") {
            let name = &var[..colon_pos];
            let default = &var[colon_pos + 2..];
            let val = self.env.get(name).unwrap_or("");
            if val.is_empty() { default.to_string() } else { val.to_string() }
        } else if let Some(colon_pos) = var.find(":+") {
            let name = &var[..colon_pos];
            let alt = &var[colon_pos + 2..];
            if self.env.get(name).unwrap_or("").is_empty() { String::new() } else { alt.to_string() }
        } else if let Some(colon_pos) = var.find(":=") {
            let name = &var[..colon_pos];
            let default = &var[colon_pos + 2..];
            match self.env.get(name) {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => {
                    self.pending_sets.borrow_mut().push((name.to_string(), default.to_string()));
                    default.to_string()
                }
            }
        } else if let Some(colon_pos) = var.find(":?") {
            let name = &var[..colon_pos];
            let msg = &var[colon_pos + 2..];
            match self.env.get(name) {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => {
                    let m = if msg.is_empty() { format!("{}: parameter null or not set", name) } else { msg.to_string() };
                    eprintln!("context: {}", m);
                    self.had_nounset_error.set(true);
                    String::new()
                }
            }
        } else if let Some(pos) = var.find('-')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let default = &var[pos + 1..];
                if self.env.get(name).is_none() { default.to_string() } else { self.env.get(name).unwrap_or("").to_string() }
        } else if let Some(pos) = var.find('+')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let alt = &var[pos + 1..];
                if self.env.get(name).is_none() { String::new() } else { alt.to_string() }
        } else if let Some(pos) = var.find('=')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let default = &var[pos + 1..];
                match self.env.get(name) {
                    Some(_) => self.env.get(name).unwrap_or("").to_string(),
                    None => {
                        self.pending_sets.borrow_mut().push((name.to_string(), default.to_string()));
                        default.to_string()
                    }
                }
        } else if let Some(pos) = var.find('?')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let msg = &var[pos + 1..];
                match self.env.get(name) {
                    Some(_) => self.env.get(name).unwrap_or("").to_string(),
                    None => {
                        let m = if msg.is_empty() { format!("{}: parameter null or not set", name) } else { msg.to_string() };
                        eprintln!("context: {}", m);
                        self.had_nounset_error.set(true);
                        String::new()
                    }
                }
        } else if let Some(inner) = var.strip_prefix('@') {
            if let Some(name) = inner.strip_suffix('Q') {
                let val = self.env.get(name).unwrap_or("");
                format!("'{}'", val.replace('\'', "'\\''"))
            } else {
                self.env.expand_special(inner)
            }
        } else if let Some(colon_pos) = var.find(':') {
            let name = &var[..colon_pos];
            let spec = &var[colon_pos + 1..];
            if let Some(semi_pos) = spec.find(':') {
                let offset_str = &spec[..semi_pos];
                let len_str = &spec[semi_pos + 1..];
                if let (Ok(offset), Ok(length)) = (offset_str.parse::<isize>(), len_str.parse::<usize>()) {
                    let val = self.env.get(name).unwrap_or("").to_string();
                    let chars: Vec<char> = val.chars().collect();
                    let start = if offset < 0 {
                        (chars.len() as isize + offset).max(0) as usize
                    } else {
                        offset as usize
                    };
                    if start >= chars.len() {
                        String::new()
                    } else {
                        chars[start..].iter().take(length).collect()
                    }
                } else {
                    self.env.expand_special(var)
                }
            } else if let Ok(offset) = spec.parse::<isize>() {
                let val = self.env.get(name).unwrap_or("").to_string();
                let chars: Vec<char> = val.chars().collect();
                let start = if offset < 0 {
                    (chars.len() as isize + offset).max(0) as usize
                } else {
                    offset as usize
                };
                if start >= chars.len() {
                    String::new()
                } else {
                    chars[start..].iter().collect()
                }
            } else {
                self.env.expand_special(var)
            }
        } else if let Some(pos) = var.find('=') {
            let name = &var[..pos];
            let default = &var[pos + 1..];
            self.pending_sets.borrow_mut().push((name.to_string(), default.to_string()));
            default.to_string()
        } else if let Some(pos) = var.find('#') {
            let name = &var[..pos];
            let is_double = pos + 1 < var.len() && var.as_bytes()[pos + 1] == b'#';
            let pattern_str = if is_double { &var[pos + 2..] } else { &var[pos + 1..] };
            let val = self.env.get(name).unwrap_or("").to_string();
            let val_chars: Vec<char> = val.chars().collect();
            let pattern_chars: Vec<char> = pattern_str.chars().collect();
            if is_double {
                let mut remove_at = 0;
                for i in 0..=val_chars.len() {
                    if glob_match_simple(&pattern_chars, &val_chars[..i]) {
                        remove_at = i;
                    }
                }
                val_chars[remove_at..].iter().collect()
            } else {
                let mut remove_at = 0;
                for i in 0..=val_chars.len() {
                    if glob_match_simple(&pattern_chars, &val_chars[..i]) {
                        remove_at = i;
                        break;
                    }
                }
                val_chars[remove_at..].iter().collect()
            }
        } else if let Some(pos) = var.find('%') {
            let name = &var[..pos];
            let is_double = pos + 1 < var.len() && var.as_bytes()[pos + 1] == b'%';
            let pattern_str = if is_double { &var[pos + 2..] } else { &var[pos + 1..] };
            let val = self.env.get(name).unwrap_or("").to_string();
            let val_chars: Vec<char> = val.chars().collect();
            let pattern_chars: Vec<char> = pattern_str.chars().collect();
            if is_double {
                let mut remove_at = val_chars.len();
                for i in 0..=val_chars.len() {
                    if glob_match_simple(&pattern_chars, &val_chars[i..]) {
                        remove_at = i;
                        break;
                    }
                }
                val_chars[..remove_at].iter().collect()
            } else {
                let mut remove_at = val_chars.len();
                for i in (0..=val_chars.len()).rev() {
                    if glob_match_simple(&pattern_chars, &val_chars[i..]) {
                        remove_at = i;
                        break;
                    }
                }
                val_chars[..remove_at].iter().collect()
            }
        } else if let Some(pos) = var.find('/') {
            let name = &var[..pos];
            let is_double = pos + 1 < var.len() && var.as_bytes()[pos + 1] == b'/';
            let rest = if is_double { &var[pos + 2..] } else { &var[pos + 1..] };
            let split_at = rest
                .as_bytes()
                .iter()
                .enumerate()
                .filter(|(i, c)| **c == b'/' && !(*i > 0 && rest.as_bytes()[*i - 1] == b'\\'))
                .map(|(i, _)| i)
                .next();
            let (old, new) = if let Some(pos2) = split_at {
                (&rest[..pos2], &rest[pos2 + 1..])
            } else {
                (rest, "")
            };
            let unescape = |s: &str| -> String {
                let mut out = String::with_capacity(s.len());
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(n) = chars.next() {
                            out.push(n);
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            };
            let old = unescape(old);
            let new = unescape(new);
            let val = self.env.get(name).unwrap_or("").to_string();
            let val_chars: Vec<char> = val.chars().collect();
            let pattern_chars: Vec<char> = old.chars().collect();
            let new_chars: Vec<char> = new.chars().collect();
            let mut result = String::new();
            let mut i = 0;
            while i < val_chars.len() {
                let mut match_len = 0;
                for j in (i + 1..=val_chars.len()).rev() {
                    if glob_match_simple(&pattern_chars, &val_chars[i..j]) {
                        match_len = j - i;
                        break;
                    }
                }
                if match_len > 0 {
                    for c in &new_chars { result.push(*c); }
                    i += match_len;
                    if !is_double {
                        break;
                    }
                } else {
                    result.push(val_chars[i]);
                    i += 1;
                }
            }
            for c in &val_chars[i..] { result.push(*c); }
            result
        } else if let Some(name) = var.strip_prefix('?') {
            match self.env.get(name) {
                Some(v) => v.to_string(),
                None => { eprintln!("context: {}: unset variable", name); String::new() }
            }
        } else if var.contains('[') && var.ends_with(']') {
            if let Some(bracket_pos) = var.find('[') {
                let name = &var[..bracket_pos];
                let key = &var[bracket_pos + 1..].trim_end_matches(']');
                match self.env.assoc_get(name, key) {
                    Some(val) => val.to_string(),
                    None => String::new(),
                }
            } else {
                self.env.expand_special(var)
            }
        } else if let Some(name) = var.strip_prefix("(k)") {
            self.env.assoc_keys(name).join(" ")
        } else if let Some(name) = var.strip_prefix("(v)") {
            self.env.assoc_values(name).join(" ")
        } else if let Some(name) = var.strip_prefix("(kv)") {
            self.env.assoc_pairs(name).iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>().join(" ")
        } else if !var.is_empty() && var.chars().all(|c| c.is_ascii_digit()) {
            let n: usize = var.parse().unwrap_or(0);
            if n == 0 {
                self.positional.first().cloned().unwrap_or_default()
            } else if n - 1 < self.positional.len() {
                self.positional[n - 1].clone()
            } else {
                String::new()
            }
        } else {
            if self.nounset && self.env.get(var).is_none() {
                eprintln!("context: {}: unset variable", var);
                self.had_nounset_error.set(true);
            }
            self.env.expand_special(var)
        }
    }
}

fn glob_match_simple(pattern: &[char], text: &[char]) -> bool {
    glob_match_inner(pattern, text)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() { return text.is_empty(); }
    if pattern[0] == '*' {
        for i in 0..=text.len() {
            if glob_match_inner(&pattern[1..], &text[i..]) { return true; }
        }
        return false;
    }
    if text.is_empty() { return false; }
    if pattern[0] == '?' || pattern[0] == text[0] {
        return glob_match_inner(&pattern[1..], &text[1..]);
    }
    if pattern[0] == '['
        && let Some(close) = pattern[1..].iter().position(|&c| c == ']') {
            let class = &pattern[2..close + 1];
            let negate = !class.is_empty() && (class[0] == '^' || class[0] == '!');
            let class_chars = if negate { &class[1..] } else { class };
            let mut matches = false;
            let mut i = 0;
            while i < class_chars.len() {
                if i + 2 < class_chars.len() && class_chars[i + 1] == '-' {
                    let lo = class_chars[i];
                    let hi = class_chars[i + 2];
                    if text[0] >= lo && text[0] <= hi {
                        matches = true;
                    }
                    i += 3;
                } else {
                    if class_chars[i] == text[0] {
                        matches = true;
                    }
                    i += 1;
                }
            }
            return if negate { !matches } else { matches }
                && glob_match_inner(&pattern[close + 2..], &text[1..]);
        }
    false
}

fn resolve_user_home(user: &str) -> String {
    if let Ok(contents) = std::fs::read_to_string("/etc/passwd") {
        for line in contents.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 6 && fields[0] == user {
                return fields[5].to_string();
            }
        }
    }
    format!("/home/{}", user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_simple_star() {
        assert!(glob_match_simple(&['*'], &['a', 'b', 'c']));
        assert!(glob_match_simple(&['*'], &[]));
        assert!(glob_match_simple(&['a', '*'], &['a']));
        assert!(glob_match_simple(&['*', 'c'], &['a', 'b', 'c']));
        assert!(glob_match_simple(&['*'], &['a', 'b', 'c', 'd']));
        assert!(!glob_match_simple(&['a', '*'], &['b']));
    }

    #[test]
    fn test_glob_match_simple_question() {
        assert!(glob_match_simple(&['?'], &['a']));
        assert!(!glob_match_simple(&['?'], &[]));
        assert!(!glob_match_simple(&['?'], &['a', 'b']));
        assert!(glob_match_simple(&['?', 'c'], &['a', 'c']));
        assert!(!glob_match_simple(&['?', 'c'], &['a', 'b']));
    }

    #[test]
    fn test_resolve_user_home() {
        let result = resolve_user_home("root");
        assert_eq!(result, "/root");
        let result = resolve_user_home("nonexistent_user_xyz_999");
        assert_eq!(result, "/home/nonexistent_user_xyz_999");
    }

    #[test]
    fn test_glob_match_simple_star_empty_pattern() {
        assert!(glob_match_simple(&[], &[]));
    }

    #[test]
    fn test_glob_match_simple_question_empty() {
        assert!(!glob_match_simple(&['?'], &[]));
    }

    #[test]
    fn test_glob_match_simple_mixed() {
        assert!(glob_match_simple(&['a', '*', 'z'], &['a', 'b', 'c', 'z']));
        assert!(!glob_match_simple(&['a', '*', 'z'], &['a', 'b', 'c']));
    }
}
