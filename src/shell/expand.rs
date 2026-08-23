use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicUsize;
use crate::shell::env::Env;

pub static CURRENT_LINE: AtomicUsize = AtomicUsize::new(1);

pub struct Expander<'a> {
    env: &'a mut Env,
    last_status: i32,
    positional: Vec<String>,
    arg_zero: String,
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
        let arg_zero = env.get("0").unwrap_or("ctx").to_string();
        Self {
            env,
            last_status,
            positional,
            arg_zero,
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
                            let sep = match self.env.get("IFS") {
                                Some("") => String::new(),
                                Some(ifs) => ifs.chars().next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()),
                                None => " ".to_string(),
                            };
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
                    while i < len && !matches!(chars[i], '/' | ':' | ' ' | '\t' | '\n') { i += 1; }
                    let user_part = &word[start..i];
                    if user_part.is_empty() {
                        result.push_str(&self.env.home());
                    } else if user_part == "+" {
                        result.push_str(self.env.get("PWD").unwrap_or(""));
                    } else if user_part == "-" {
                        result.push_str(self.env.get("OLDPWD").unwrap_or(""));
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

    pub fn expand_words(&mut self, word: &str) -> Vec<String> {
        let brace_enabled = self.env.get("_OPT_B")
            .map(|s| !s.is_empty())
            .unwrap_or_else(|| {
                self.env.get("SHELLOPTS")
                    .map(|s| s.contains("braceexpand"))
                    .unwrap_or(false)
            });
        let expanded = if brace_enabled { brace_expand(word) } else { vec![word.to_string()] };
        expanded.into_iter().flat_map(|w| {
            let expanded_word = self.expand_word(&w);
            self.word_split(&expanded_word)
        }).collect()
    }

    fn word_split(&self, word: &str) -> Vec<String> {
        if word.contains('\x01') || word.contains('\x02') {
            return vec![word.to_string()];
        }
        if word.contains('\x03') {
            return word.split('\x03').filter(|s| !s.is_empty()).map(String::from).collect();
        }
        let ifs = self.env.get("IFS").map(|s| s.to_string()).unwrap_or_else(|| " \t\n".to_string());
        if ifs.is_empty() {
            return vec![word.to_string()];
        }
        let ifs_chars: Vec<char> = ifs.chars().collect();
        let ifs_ws: Vec<char> = ifs_chars.iter().copied()
            .filter(|c| *c == ' ' || *c == '\t' || *c == '\n')
            .collect();
        let ifs_nws: Vec<char> = ifs_chars.iter().copied()
            .filter(|c| *c != ' ' && *c != '\t' && *c != '\n')
            .collect();
        let mut result = Vec::new();
        let mut current = String::new();
        let mut chars = word.chars().peekable();
        while let Some(c) = chars.next() {
            if ifs_ws.contains(&c) {
                while chars.peek().is_some_and(|nc| ifs_ws.contains(nc)) {
                    chars.next();
                }
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            } else if ifs_nws.contains(&c) {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                result.push(String::new());
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            result.push(current);
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
            '[' => {
                // $[expr] — arithmetic expansion (legacy form of $((expr)))
                let start = *i + 1;
                let mut depth = 0u32;
                let mut j = start;
                while j < len {
                    match chars[j] {
                        '[' => depth += 1,
                        ']' if depth == 0 => break,
                        ']' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let inner: String = chars[start..j.min(len)].iter().collect();
                *i = j + 1;
                let expanded_inner = self.expand_word(&inner);
                crate::shell::builtin::eval_arith_assign(&expanded_inner, self.env).to_string()
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
                    // `$0` is the shell name, independent of the positionals.
                    self.arg_zero.clone()
                } else if n - 1 < self.positional.len() {
                    self.positional[n - 1].clone()
                } else {
                    String::new()
                }
            }
            '@' => {
                *i += 1;
                self.positional.join("\x03")
            }
            '*' => {
                *i += 1;
                let sep = match self.env.get("IFS") {
                    Some("") => String::new(),
                    Some(ifs) => ifs.chars().next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()),
                    None => " ".to_string(),
                };
                self.positional.join(&sep)
            }
            '#' => { *i += 1; self.positional.len().to_string() }
            '-' => { *i += 1; "himB".to_string() }
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
                    let mut wstatus: i32 = 0;
                    libc::waitpid(pid, &mut wstatus, 0);
                    self.last_status = if libc::WIFEXITED(wstatus) { libc::WEXITSTATUS(wstatus) }
                                       else if libc::WIFSIGNALED(wstatus) { 128 + libc::WTERMSIG(wstatus) }
                                       else { 1 };
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
                let inner = self.env.get(name).unwrap_or("").to_string();
                return self.env.get(&inner).unwrap_or("").to_string();
            }
        if let Some(name) = var.strip_prefix('#') {
            if name.is_empty() {
                return self.positional.len().to_string();
            }
            if name.as_bytes()[0] == b'#' {
                let name = &name[1..];
                let val = self.env.get(name).unwrap_or("").to_string();
                val.chars().count().to_string()
            } else if let Some(base) = name.strip_suffix("[@]").or_else(|| name.strip_suffix("[*]")) {
                // ${#arr[@]} — number of array elements.
                if self.env.is_indexed_array(base) {
                    self.env.indexed_array_len(base).to_string()
                } else if self.env.is_assoc_array(base) {
                    self.env.assoc_len(base).to_string()
                } else {
                    "0".to_string()
                }
            } else if name.as_bytes()[0] == b'%' {
                let name = &name[1..];
                let val = self.env.get(name).unwrap_or("").to_string();
                val.matches(char::is_alphanumeric).count().to_string()
            } else if self.env.is_assoc_array(name) {
                self.env.assoc_len(name).to_string()
            } else if self.env.is_indexed_array(name) {
                self.env.indexed_array_len(name).to_string()
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
            let default = var[colon_pos + 2..].to_string();
            let val = self.env.get(name).unwrap_or("");
            if val.is_empty() { self.expand_word(&default) } else { val.to_string() }
        } else if let Some(colon_pos) = var.find(":+") {
            let name = &var[..colon_pos];
            let alt = var[colon_pos + 2..].to_string();
            if self.env.get(name).unwrap_or("").is_empty() { String::new() } else { self.expand_word(&alt) }
        } else if let Some(colon_pos) = var.find(":=") {
            let name = &var[..colon_pos];
            let default = var[colon_pos + 2..].to_string();
            match self.env.get(name) {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => {
                    let expanded = self.expand_word(&default);
                    self.pending_sets.borrow_mut().push((name.to_string(), expanded.clone()));
                    expanded
                }
            }
        } else if let Some(colon_pos) = var.find(":?") {
            let name = &var[..colon_pos];
            let msg = var[colon_pos + 2..].to_string();
            match self.env.get(name) {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => {
                    let m = if msg.is_empty() { format!("{}: parameter null or not set", name) } else { self.expand_word(&msg) };
                    eprintln!("context: {}", m);
                    self.had_nounset_error.set(true);
                    String::new()
                }
            }
        } else if let Some(pos) = var.find('-')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let default = var[pos + 1..].to_string();
                if self.env.get(name).is_none() { self.expand_word(&default) } else { self.env.get(name).unwrap_or("").to_string() }
        } else if let Some(pos) = var.find('+')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let alt = var[pos + 1..].to_string();
                if self.env.get(name).is_none() { String::new() } else { self.expand_word(&alt) }
        } else if let Some(pos) = var.find('=')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let default = var[pos + 1..].to_string();
                match self.env.get(name) {
                    Some(_) => self.env.get(name).unwrap_or("").to_string(),
                    None => {
                        let expanded = self.expand_word(&default);
                        self.pending_sets.borrow_mut().push((name.to_string(), expanded.clone()));
                        expanded
                    }
                }
        } else if let Some(pos) = var.find('?')
            && !var[..pos].contains(':') {
                let name = &var[..pos];
                let msg = var[pos + 1..].to_string();
                match self.env.get(name) {
                    Some(v) => v.to_string(),
                    None => {
                        let m = if msg.is_empty() { format!("{}: parameter null or not set", name) } else { self.expand_word(&msg) };
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
                if let (Ok(offset), Ok(length)) = (offset_str.parse::<isize>(), len_str.parse::<isize>()) {
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
                        let effective_len = if length < 0 {
                            let end = (chars.len() as isize + length).max(start as isize) as usize;
                            end - start
                        } else {
                            length as usize
                        };
                        chars[start..].iter().take(effective_len).collect()
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
            // `${arr[#]}` is an indexed-array element count, not a
            // prefix-removal pattern.
            if let Some(arr_name) = var.strip_suffix("[#]")
                && self.env.is_indexed_array(arr_name) {
                    return self.env.indexed_array_len(arr_name).to_string();
                }
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
            let is_start = pos + 1 < var.len() && var.as_bytes()[pos + 1] == b'#';
            let is_end = pos + 1 < var.len() && var.as_bytes()[pos + 1] == b'%';
            let rest = if is_double || is_start || is_end { &var[pos + 2..] } else { &var[pos + 1..] };
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
            if is_start {
                for j in (1..=val_chars.len()).rev() {
                    if glob_match_simple(&pattern_chars, &val_chars[..j]) {
                        let matched: String = val_chars[..j].iter().collect();
                        for ci in 0..new_chars.len() {
                            if new_chars[ci] == '&' || (new_chars[ci] == '\\' && ci + 1 < new_chars.len() && new_chars[ci + 1].is_ascii_digit()) {
                                result.push_str(&matched);
                            } else {
                                result.push(new_chars[ci]);
                            }
                        }
                        result.push_str(&val_chars[j..].iter().collect::<String>());
                        return result;
                    }
                }
                val
            } else if is_end {
                for j in 0..=val_chars.len() {
                    if glob_match_simple(&pattern_chars, &val_chars[j..]) {
                        let matched: String = val_chars[j..].iter().collect();
                        for ci in 0..new_chars.len() {
                            if new_chars[ci] == '&' || (new_chars[ci] == '\\' && ci + 1 < new_chars.len() && new_chars[ci + 1].is_ascii_digit()) {
                                result.push_str(&matched);
                            } else {
                                result.push(new_chars[ci]);
                            }
                        }
                        result.push_str(&val_chars[..j].iter().collect::<String>());
                        return result;
                    }
                }
                val
            } else {
            while i < val_chars.len() {
                let mut match_len = 0;
                for j in (i + 1..=val_chars.len()).rev() {
                    if glob_match_simple(&pattern_chars, &val_chars[i..j]) {
                        match_len = j - i;
                        break;
                    }
                }
                if match_len > 0 {
                    let matched: String = val_chars[i..i+match_len].iter().collect();
                    for ci in 0..new_chars.len() {
                        if new_chars[ci] == '&' || (new_chars[ci] == '\\' && ci + 1 < new_chars.len() && new_chars[ci + 1].is_ascii_digit()) {
                            result.push_str(&matched);
                        } else {
                            result.push(new_chars[ci]);
                        }
                    }
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
            }
        } else if let Some(name) = var.strip_prefix('?') {
            match self.env.get(name) {
                Some(v) => v.to_string(),
                None => { eprintln!("context: {}: unset variable", name); String::new() }
            }
        } else if var.contains('[') && var.ends_with(']') {
            if let Some(bracket_pos) = var.find('[') {
                let name = &var[..bracket_pos];
                let key: &str = var[bracket_pos + 1..].trim_end_matches(']');
                if key == "@" || key == "*" {
                    // Whole-array expansion over consecutive arr_N keys.
                    if self.env.is_indexed_array(name) {
                        return self.env.indexed_array_elements(name).join(" ");
                    }
                    if self.env.is_assoc_array(name) {
                        return self.env.assoc_values(name).join(" ");
                    }
                    return self.env.get(name).unwrap_or("").to_string();
                }
                if key == "#" {
                    // `${arr[#]}` — element count of an indexed array.
                    if self.env.is_indexed_array(name) {
                        return self.env.indexed_array_len(name).to_string();
                    }
                }
                if let Some(val) = self.env.assoc_get(name, key) {
                    val.to_string()
                } else if let Some(val) = self.env.indexed_array_get(name, key) {
                    val.to_string()
                } else {
                    String::new()
                }
            } else {
                self.env.expand_special(var)
            }
        } else if let Some(name) = var.strip_prefix("(k)") {
            if self.env.is_indexed_array(name) {
                let len = self.env.indexed_array_len(name);
                (0..len).map(|i| i.to_string()).collect::<Vec<_>>().join(" ")
            } else {
                self.env.assoc_keys(name).join(" ")
            }
        } else if let Some(name) = var.strip_prefix("(v)") {
            if self.env.is_indexed_array(name) {
                let len = self.env.indexed_array_len(name);
                (0..len).filter_map(|i| self.env.indexed_array_get(name, &i.to_string()).map(|s| s.to_string())).collect::<Vec<_>>().join(" ")
            } else {
                self.env.assoc_values(name).join(" ")
            }
        } else if let Some(name) = var.strip_prefix("(kv)") {
            if self.env.is_indexed_array(name) {
                let len = self.env.indexed_array_len(name);
                (0..len).filter_map(|i| self.env.indexed_array_get(name, &i.to_string()).map(|v| format!("[{}]={}", i, v))).collect::<Vec<_>>().join(" ")
            } else {
                self.env.assoc_pairs(name).iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>().join(" ")
            }
        } else if let Some(name) = var.strip_suffix("@Q") {
            let val = self.env.get(name).unwrap_or("");
            format!("'{}'", val.replace('\'', "'\\''"))
        } else if let Some(name) = var.strip_suffix("@U") {
            let val = self.env.get(name).unwrap_or("").to_string();
            val.to_uppercase()
        } else if let Some(name) = var.strip_suffix("@u") {
            let val = self.env.get(name).unwrap_or("").to_string();
            val.to_lowercase()
        } else if let Some(name) = var.strip_suffix("@E") {
            let val = self.env.get(name).unwrap_or("").to_string();
            escape_expand(&val)
        } else if let Some(name) = var.strip_suffix("@a") {
            let mut attrs = String::new();
            if self.env.is_readonly(name) { attrs.push('r'); }
            if self.env.is_exported(name) { attrs.push('x'); }
            attrs
        } else if let Some(name) = var.strip_suffix("@P") {
            let val = self.env.get(name).unwrap_or("").to_string();
            prompt_expand_basic(&val, self.env)
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

/// Glob match `pattern` against `text` (supports `*`, `?`, bracket classes,
/// POSIX classes and extglob alternations).
pub fn glob_match_str(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_simple(pattern: &[char], text: &[char]) -> bool {
    glob_match_inner(pattern, text)
}

fn split_extglob_alts(inner: &[char]) -> Vec<&[char]> {
    let mut alts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, &c) in inner.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                alts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&inner[start..]);
    alts
}

fn extglob_match_repeat(alt: &[char], rest: &[char], text: &[char]) -> bool {
    if glob_match_inner(rest, text) {
        return true;
    }
    for i in 1..=text.len() {
        if glob_match_inner(alt, &text[..i]) && extglob_match_repeat(alt, rest, &text[i..]) {
            return true;
        }
    }
    false
}

fn extglob_match(op: char, inner: &[char], rest: &[char], text: &[char]) -> bool {
    let alts = split_extglob_alts(inner);
    match op {
        '@' => {
            for alt in &alts {
                for i in 1..=text.len() {
                    if glob_match_inner(alt, &text[..i]) && glob_match_inner(rest, &text[i..]) {
                        return true;
                    }
                }
            }
            false
        }
        '+' => {
            for alt in &alts {
                for i in 1..=text.len() {
                    if glob_match_inner(alt, &text[..i]) && extglob_match_repeat(alt, rest, &text[i..]) {
                        return true;
                    }
                }
            }
            false
        }
        '*' => {
            if glob_match_inner(rest, text) {
                return true;
            }
            for alt in &alts {
                for i in 1..=text.len() {
                    if glob_match_inner(alt, &text[..i]) && extglob_match_repeat(alt, rest, &text[i..]) {
                        return true;
                    }
                }
            }
            false
        }
        '?' => {
            if glob_match_inner(rest, text) {
                return true;
            }
            for alt in &alts {
                for i in 1..=text.len() {
                    if glob_match_inner(alt, &text[..i]) && glob_match_inner(rest, &text[i..]) {
                        return true;
                    }
                }
            }
            false
        }
        '!' => {
            for i in 0..=text.len() {
                let prefix = &text[..i];
                if !alts.iter().any(|alt| glob_match_inner(alt, prefix))
                    && glob_match_inner(rest, &text[i..]) {
                        return true;
                    }
            }
            false
        }
        _ => false,
    }
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() { return text.is_empty(); }
    if pattern.len() >= 2 && matches!(pattern[0], '+' | '*' | '?' | '!' | '@') && pattern[1] == '(' {
        let mut depth = 1u32;
        let mut j = 2;
        while j < pattern.len() && depth > 0 {
            if pattern[j] == '(' { depth += 1; }
            else if pattern[j] == ')' { depth -= 1; }
            j += 1;
        }
        if depth == 0 {
            let inner = &pattern[2..j - 1];
            let rest = &pattern[j..];
            return extglob_match(pattern[0], inner, rest, text);
        }
    }
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
    if pattern[0] == '[' {
        let mut pos = 1;
        let negate = pos < pattern.len() && (pattern[pos] == '^' || pattern[pos] == '!');
        if negate { pos += 1; }
        let mut char_matches = false;
        while pos < pattern.len() && pattern[pos] != ']' {
            if pattern[pos] == ':'
                && pos + 1 < pattern.len() && pattern[pos + 1] == ':'
                    && let Some(end) = pattern[pos + 2..].iter().position(|&c| c == ':')
                        && pos + 2 + end + 1 < pattern.len()
                            && pattern[pos + 2 + end + 1] == ']' {
                                let name: String = pattern[pos + 2..pos + 2 + end].iter().collect();
                                if posix_class_match(text[0], &name) {
                                    char_matches = true;
                                }
                                pos = pos + 2 + end + 2;
                                continue;
                            }
            if pos + 2 < pattern.len() && pattern[pos + 1] == '-' && pattern[pos + 2] != ']' {
                if text[0] >= pattern[pos] && text[0] <= pattern[pos + 2] {
                    char_matches = true;
                }
                pos += 3;
            } else {
                if pattern[pos] == text[0] {
                    char_matches = true;
                }
                pos += 1;
            }
        }
        if pos < pattern.len() && pattern[pos] == ']' {
            return (if negate { !char_matches } else { char_matches })
                && glob_match_inner(&pattern[pos + 1..], &text[1..]);
        }
    }
    false
}

fn posix_class_match(c: char, name: &str) -> bool {
    match name {
        "alpha" => c.is_ascii_alphabetic(),
        "digit" => c.is_ascii_digit(),
        "alnum" => c.is_ascii_alphanumeric(),
        "space" => c.is_ascii_whitespace(),
        "upper" => c.is_ascii_uppercase(),
        "lower" => c.is_ascii_lowercase(),
        "punct" => !c.is_ascii_alphanumeric() && !c.is_ascii_whitespace(),
        "blank" => c == ' ' || c == '\t',
        "cntrl" => (c as u8) < 32 || c == '\x7f',
        "print" => (c as u8) >= 0x20 && c != '\x7f',
        "graph" => (c as u8) >= 0x21 && c != '\x7f',
        "xdigit" => c.is_ascii_hexdigit(),
        _ => false,
    }
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

fn escape_expand(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '\\' && i + 1 < len {
            i += 1;
            match chars[i] {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                '\\' => result.push('\\'),
                'a' => result.push('\x07'),
                'b' => result.push('\x08'),
                'e' => result.push('\x1b'),
                'f' => result.push('\x0c'),
                'r' => result.push('\r'),
                'v' => result.push('\x0b'),
                'x' => {
                    i += 1;
                    let mut hex = String::new();
                    while i < len && hex.len() < 2 && chars[i].is_ascii_hexdigit() {
                        hex.push(chars[i]);
                        i += 1;
                    }
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    }
                    continue;
                }
                '0' => {
                    let mut oct = String::new();
                    i += 1;
                    while i < len && oct.len() < 3 && matches!(chars[i], '0'..='7') {
                        oct.push(chars[i]);
                        i += 1;
                    }
                    if !oct.is_empty()
                        && let Ok(byte) = u8::from_str_radix(&oct, 8) {
                            result.push(byte as char);
                        }
                    continue;
                }
                '1'..='7' => {
                    let mut oct = String::new();
                    oct.push(chars[i]);
                    i += 1;
                    while i < len && oct.len() < 3 && matches!(chars[i], '0'..='7') {
                        oct.push(chars[i]);
                        i += 1;
                    }
                    if let Ok(byte) = u8::from_str_radix(&oct, 8) {
                        result.push(byte as char);
                    }
                    continue;
                }
                _ => {
                    result.push('\\');
                    result.push(chars[i]);
                }
            }
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}

fn prompt_expand_basic(s: &str, env: &Env) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '\\' && i + 1 < len {
            i += 1;
            match chars[i] {
                'u' => result.push_str(env.get("USER").unwrap_or("user")),
                'h' => result.push_str(env.hostname().as_str()),
                'H' => result.push_str(env.hostname().as_str()),
                'w' => {
                    let cwd = env.get("PWD").unwrap_or("~");
                    let home = env.home();
                    if cwd == home {
                        result.push('~');
                    } else if let Some(rest) = cwd.strip_prefix(&home) {
                        result.push('~');
                        result.push_str(rest);
                    } else {
                        result.push_str(cwd);
                    }
                }
                'W' => {
                    let cwd = env.get("PWD").unwrap_or("~");
                    let home = env.home();
                    if cwd == home {
                        result.push('~');
                    } else {
                        match cwd.rfind('/') {
                            Some(0) => result.push_str(&cwd[1..]),
                            Some(pos) => result.push_str(&cwd[pos + 1..]),
                            None => result.push_str(cwd),
                        }
                    }
                }
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                '$' => {
                    if std::process::id() == 1 {
                        result.push('#');
                    } else {
                        result.push('$');
                    }
                }
                _ => {
                    result.push('\\');
                    result.push(chars[i]);
                }
            }
        } else if chars[i] == '$' && i + 1 < len {
            i += 1;
            match chars[i] {
                '0' => result.push_str(env.get("0").unwrap_or("context")),
                '!' => {
                    let h: String = crate::shell::expand::CURRENT_LINE.load(std::sync::atomic::Ordering::Relaxed).to_string();
                    result.push_str(&h);
                }
                '#' => {
                    let len = env.positional().len();
                    result.push_str(&len.to_string());
                }
                '?' => result.push_str(env.get("?").unwrap_or("0")),
                '$' => result.push_str(&std::process::id().to_string()),
                _ => {
                    let start = i;
                    while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    if let Some(val) = env.get(&name) {
                        result.push_str(val);
                    }
                    i -= 1;
                }
            }
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}

fn brace_expand(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut pos = 0;
    while pos < len {
        if chars[pos] == '{' && (pos == 0 || chars[pos - 1] != '$') {
            let before: String = chars[..pos].iter().collect();
            let mut depth = 1;
            let mut end = None;
            for (j, ch) in chars[pos + 1..].iter().enumerate() {
                let j = pos + 1 + j;
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end {
                let inner: String = chars[pos + 1..end_idx].iter().collect();
                let after: String = chars[end_idx + 1..].iter().collect();
                let alternatives = split_brace_items(&inner);
                let mut result = Vec::new();
                for alt in &alternatives {
                    if let Some(items) = expand_brace_sequence(alt) {
                        for item in items {
                            let expanded = format!("{}{}{}", before, item, after);
                            result.extend(brace_expand(&expanded));
                        }
                    } else {
                        let expanded = format!("{}{}{}", before, alt, after);
                        result.extend(brace_expand(&expanded));
                    }
                }
                return result;
            }
            return vec![input.to_string()];
        }
        pos += 1;
    }
    vec![input.to_string()]
}

fn split_brace_items(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => { depth += 1; current.push(c); }
            '}' => { depth -= 1; current.push(c); }
            ',' if depth == 0 => {
                result.push(current.clone());
                current.clear();
            }
            _ => { current.push(c); }
        }
    }
    result.push(current);
    result
}

fn expand_brace_sequence(s: &str) -> Option<Vec<String>> {
    if let Some(dot_pos) = s.find("..") {
        let start_str = &s[..dot_pos];
        let rest = &s[dot_pos + 2..];
        if let Some(step_pos) = rest.find("..") {
            let end_str = &rest[..step_pos];
            let step_str = &rest[step_pos + 2..];
            if let (Ok(start), Ok(end), Ok(step)) = (
                start_str.parse::<i64>(),
                end_str.parse::<i64>(),
                step_str.parse::<i64>(),
            ) {
                if step == 0 { return None; }
                let mut items = Vec::new();
                if start <= end {
                    let mut i = start;
                    while i <= end {
                        items.push(i.to_string());
                        i += step;
                    }
                } else {
                    let mut i = start;
                    while i >= end {
                        items.push(i.to_string());
                        i += step;
                    }
                }
                return Some(items);
            }
        } else if let (Ok(start), Ok(end)) = (start_str.parse::<i64>(), rest.parse::<i64>()) {
            if start_str.chars().all(|c| c.is_ascii_digit())
                && rest.chars().all(|c| c.is_ascii_digit()) {
                    let mut items = Vec::new();
                    if start <= end {
                        for i in start..=end {
                            items.push(i.to_string());
                        }
                    } else {
                        for i in (end..=start).rev() {
                            items.push(i.to_string());
                        }
                    }
                    return Some(items);
                }
            if start_str.len() == 1 && rest.len() == 1 {
                let s_char = start_str.chars().next().unwrap();
                let e_char = rest.chars().next().unwrap();
                if s_char.is_ascii_alphabetic() && e_char.is_ascii_alphabetic() {
                    let mut items = Vec::new();
                    if s_char <= e_char {
                        for c in s_char..=e_char {
                            items.push(c.to_string());
                        }
                    } else {
                        for c in (e_char..=s_char).rev() {
                            items.push(c.to_string());
                        }
                    }
                    return Some(items);
                }
            }
        }
    }
    None
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

    #[test]
    fn test_dollar_default_empty_unset() {
        let mut env = Env::new();
        env.unset("UNSET_VAR");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${UNSET_VAR:-}"), "");
    }

    #[test]
    fn test_dollar_plus_set() {
        let mut env = Env::new();
        env.set("MY_VAR", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${MY_VAR:+replacement}"), "replacement");
    }

    #[test]
    fn test_dollar_plus_unset() {
        let mut env = Env::new();
        env.unset("UNSET_VAR");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${UNSET_VAR:+replacement}"), "");
    }

    #[test]
    fn test_dollar_question_set() {
        let mut env = Env::new();
        env.set("MY_VAR", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${MY_VAR:?error msg}"), "hello");
    }

    #[test]
    fn test_dollar_question_unset() {
        let mut env = Env::new();
        env.unset("UNSET_VAR");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${UNSET_VAR:?error message}");
        assert_eq!(result, "");
        assert!(exp.had_nounset_error());
    }

    #[test]
    fn test_nested_dollar_default() {
        let mut env = Env::new();
        env.unset("A");
        env.set("B", "fromB");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${A:-$B}"), "fromB");
    }

    #[test]
    fn test_nested_dollar_default_outer_set() {
        let mut env = Env::new();
        env.set("A", "val");
        env.set("B", "fromB");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${A:-$B}"), "val");
    }

    #[test]
    fn test_word_splitting_simple() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_ifs_comma_splitting() {
        let mut env = Env::new();
        env.set("IFS", ",");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let words = exp.expand_words("a,b,c");
        assert_eq!(words, vec!["a", "", "b", "", "c"]);
    }

    #[test]
    fn test_ifs_empty_no_split() {
        let mut env = Env::new();
        env.set("IFS", "");
        let exp = Expander::new(&mut env, 0, vec![], 0);
        let words = exp.word_split("a b c");
        assert_eq!(words, vec!["a b c"]);
    }

    #[test]
    fn test_length_multibyte() {
        let mut env = Env::new();
        env.set("V", "cafe\u{0301}");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${#V}");
        assert_eq!(result, "5");
    }

    #[test]
    fn test_length_empty() {
        let mut env = Env::new();
        env.set("V", "");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${#V}");
        assert_eq!(result, "0");
    }

    #[test]
    fn test_substring_multibyte() {
        let mut env = Env::new();
        env.set("V", "abc\u{00e9}f");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${V:0:1}");
        assert_eq!(result, "a");
    }

    #[test]
    fn test_substring_negative_offset() {
        let mut env = Env::new();
        env.set("V", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${V:2}");
        assert_eq!(result, "llo");
    }

    #[test]
    fn test_substring_multibyte_offset() {
        let mut env = Env::new();
        env.set("V", "ab\u{00e9}cd");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${V:2:1}");
        assert_eq!(result, "\u{00e9}");
    }

    #[test]
    fn test_pattern_subst_literal_chars() {
        let mut env = Env::new();
        env.set("V", "a.b.c");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${V/./X}");
        assert_eq!(result, "aXb.c");
    }

    #[test]
    fn test_pattern_subst_special_glob_chars_literal() {
        let mut env = Env::new();
        env.set("V", "[test]");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${V/[test]/done}");
        assert_eq!(result, "done");
    }

    #[test]
    fn test_double_hash_length() {
        let mut env = Env::new();
        env.set("V", "cafe");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${##V}"), "4");
    }

    #[test]
    fn test_uppercase_operator() {
        let mut env = Env::new();
        env.set("V", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V^^}"), "HELLO");
    }

    #[test]
    fn test_lowercase_operator() {
        let mut env = Env::new();
        env.set("V", "HELLO");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V,,}"), "hello");
    }

    #[test]
    fn test_remove_shortest_suffix() {
        let mut env = Env::new();
        env.set("V", "file.tar.gz");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V%.*}"), "file.tar");
    }

    #[test]
    fn test_remove_longest_suffix() {
        let mut env = Env::new();
        env.set("V", "file.tar.gz");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V%%.*}"), "file");
    }

    #[test]
    fn test_remove_shortest_prefix() {
        let mut env = Env::new();
        env.set("V", "src/main.rs");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V#*/}"), "main.rs");
    }

    #[test]
    fn test_remove_longest_prefix() {
        let mut env = Env::new();
        env.set("V", "src/main.rs");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V##*/}"), "main.rs");
    }

    #[test]
    fn test_default_assign_sets_pending() {
        let mut env = Env::new();
        env.unset("X");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${X:=hello}");
        assert_eq!(result, "hello");
        let pending = exp.take_pending_sets();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "X");
        assert_eq!(pending[0].1, "hello");
    }

    #[test]
    fn test_empty_var_default() {
        let mut env = Env::new();
        env.set("V", "");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:-fallback}"), "fallback");
    }

    #[test]
    fn test_set_var_not_default() {
        let mut env = Env::new();
        env.set("V", "actual");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:-fallback}"), "actual");
    }

    #[test]
    fn test_positional_parameters() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec!["one".into(), "two".into(), "three".into()], 0);
        assert_eq!(exp.expand_word("$1"), "one");
        assert_eq!(exp.expand_word("$2"), "two");
        assert_eq!(exp.expand_word("$3"), "three");
        assert_eq!(exp.expand_word("$4"), "");
    }

    #[test]
    fn test_dollar_at_with_ifs() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec!["a".into(), "b".into()], 0);
        let result = exp.expand_word("$@");
        let parts: Vec<&str> = result.split('\x03').collect();
        assert_eq!(parts, vec!["a", "b"]);
    }

    #[test]
    fn test_glob_match_bracket_class() {
        assert!(glob_match_simple(&['[', 'a', '-', 'z', ']'], &['m']));
        assert!(!glob_match_simple(&['[', 'a', '-', 'z', ']'], &['1']));
    }

    #[test]
    fn test_glob_match_negated_bracket() {
        assert!(!glob_match_simple(&['[', '!', 'a', '-', 'z', ']'], &['m']));
        assert!(glob_match_simple(&['[', '!', 'a', '-', 'z', ']'], &['1']));
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match_simple(&['*', '*', '/'], &['a', '/']));
        assert!(glob_match_simple(&['*', '*', '/'], &['/', '/']));
        assert!(!glob_match_simple(&['*', '*', '/'], &['/', 'f', 'o', 'o']));
    }

    #[test]
    fn test_arith_expand_basic() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("$((2+3))"), "5");
    }

    #[test]
    fn test_arith_expand_division() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("$((10/3))"), "3");
    }

    #[test]
    fn test_arith_expand_nested() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("$((2*(3+4)))"), "14");
    }

    #[test]
    fn test_dollar_special_vars() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 42, vec![], 99);
        assert_eq!(exp.expand_word("$?"), "42");
        assert!(exp.expand_word("$$").parse::<u32>().is_ok());
        assert_eq!(exp.expand_word("$!"), "99");
    }

    #[test]
    fn test_dollar_hash_count() {
        let mut env = Env::new();
        env.set("X", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${#X}"), "5");
    }

    #[test]
    fn test_unset_triggers_nounset() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        exp.set_nounset(true);
        let result = exp.expand_word("${MISSING}");
        assert_eq!(result, "");
        assert!(exp.had_nounset_error());
    }

    #[test]
    fn test_default_assign_empty_var() {
        let mut env = Env::new();
        env.set("X", "");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let result = exp.expand_word("${X:=newval}");
        assert_eq!(result, "newval");
        let pending = exp.take_pending_sets();
        assert_eq!(pending[0].0, "X");
        assert_eq!(pending[0].1, "newval");
    }

    #[test]
    fn test_colon_u_uppercase() {
        let mut env = Env::new();
        env.set("V", "hello");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:u}"), "HELLO");
    }

    #[test]
    fn test_colon_l_lowercase() {
        let mut env = Env::new();
        env.set("V", "HELLO");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:l}"), "hello");
    }

    #[test]
    fn test_colon_r_remove_suffix() {
        let mut env = Env::new();
        env.set("V", "file.tar.gz");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:r}"), "file.tar");
    }

    #[test]
    fn test_colon_e_extension() {
        let mut env = Env::new();
        env.set("V", "file.tar.gz");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:e}"), "gz");
    }

    #[test]
    fn test_colon_t_tail() {
        let mut env = Env::new();
        env.set("V", "/usr/local/bin");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:t}"), "bin");
    }

    #[test]
    fn test_colon_h_head() {
        let mut env = Env::new();
        env.set("V", "/usr/local/bin");
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        assert_eq!(exp.expand_word("${V:h}"), "/usr/local");
    }

    #[test]
    fn test_tilde_expansion() {
        let mut env = Env::new();
        let mut exp = Expander::new(&mut env, 0, vec![], 0);
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(exp.expand_word("~"), home);
    }
}

#[cfg(test)]
mod array_expansion_tests {
    use super::*;
    use crate::shell::env::Env;

    #[test]
    fn test_array_at_and_length_expansion() {
        let mut env = Env::new();
        env.set("r_0", "a");
        env.set("r_1", "b");
        assert!(env.is_indexed_array("r"), "is_indexed_array");
        {
            let mut ex = Expander::new(&mut env, 0, vec![], 0);
            assert_eq!(ex.expand_var("r[0]"), "a");
            assert_eq!(ex.expand_var("r[@]"), "a b");
            assert_eq!(ex.expand_var("#r[@]"), "2");
        }
    }
}
