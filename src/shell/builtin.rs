use std::fs;
use std::io::Write;
use std::path::Path;
use std::os::unix::fs::FileTypeExt;
use crate::config::Config;
use crate::shell::env::Env;

pub struct BuiltinResult {
    pub status: i32,
    pub exit: bool,
    pub exit_code: Option<i32>,
    pub source_file: Option<String>,
    pub clear_history: bool,
    pub eval_string: Option<String>,
}

impl BuiltinResult {
    pub fn ok() -> Self { Self { status: 0, exit: false, exit_code: None, source_file: None, clear_history: false, eval_string: None } }
    pub fn err(status: i32) -> Self { Self { status, exit: false, exit_code: None, source_file: None, clear_history: false, eval_string: None } }
    pub fn exit(code: i32) -> Self { Self { status: code, exit: true, exit_code: Some(code), source_file: None, clear_history: false, eval_string: None } }

    fn if_bool(val: bool) -> Self {
        if val { Self::ok() } else { Self::err(1) }
    }
}

pub fn run(args: &[String], env: &mut Env, cfg: &Config, last_status: i32) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let cmd = args[0].as_str();
    match cmd {
        "cd" => cmd_cd(&args[1..], env, cfg),
        "exit" => cmd_exit(&args[1..], last_status),
        "export" => cmd_export(&args[1..], env),
        "unset" => cmd_unset(&args[1..], env),
        "alias" => cmd_alias(&args[1..], env),
        "unalias" => cmd_unalias(&args[1..], env),
        "source" | "." => cmd_source(&args[1..], env),
        "history" => cmd_history(&args[1..], cfg),
        "set" => cmd_set(&args[1..], env),
        "unsetenv" => cmd_unsetenv(&args[1..], env),
        "env" => cmd_env(env, cfg),
        "pwd" => cmd_pwd(),
        "type" => cmd_type(&args[1..]),
        "which" => cmd_which(&args[1..]),
        "echo" => cmd_echo(&args[1..]),
        "printf" => cmd_printf(&args[1..]),
        "true" => BuiltinResult::ok(),
        "false" => BuiltinResult::err(1),
        "test" | "[" => cmd_test(&args[1..]),
        "let" => cmd_let(&args[1..], env),
        "exec" => BuiltinResult::ok(),
        "trap" => cmd_trap(&args[1..], env),
        "pushd" => cmd_pushd(&args[1..], env),
        "popd" => cmd_popd(&args[1..], env),
        "dirs" => cmd_dirs(&args[1..], env),
        "hash" => cmd_hash(&args[1..]),
        "math" => cmd_math(&args[1..], env, cfg),
        "regexmatch" => cmd_regexmatch(&args[1..], env),
        "module" => cmd_module(&args[1..], env),
        "readonly" => cmd_readonly(&args[1..], env),
        "local" => cmd_local(&args[1..], env),
        "builtin" => {
            if args.len() > 1 {
                let builtin_args = &args[1..];
                return run(builtin_args, env, cfg, last_status);
            }
            BuiltinResult::ok()
        }
        "caller" => {
            eprintln!("context: caller: no call stack");
            BuiltinResult::err(1)
        }
        "shopt" => cmd_shopt(&args[1..], env),
        "declare" | "typeset" => cmd_declare(&args[1..], env),
        "wait" => cmd_wait(&args[1..]),
        "kill" => cmd_kill(&args[1..]),
        "umask" => cmd_umask(&args[1..]),
        "command" => cmd_command(&args[1..]),
        "eval" => {
            let code = args[1..].join(" ");
            BuiltinResult { status: 0, exit: false, exit_code: None, source_file: None, clear_history: false, eval_string: Some(code) }
        }
        "select" => cmd_select(&args[1..], env),
        "getopts" => cmd_getopts(&args[1..], env),
        "realpath" => cmd_realpath(&args[1..]),
        "read" => cmd_read(&args[1..], env),
        "bindkey" => cmd_bindkey(&args[1..], env),
        _ => BuiltinResult::err(127),
    }
}

fn cmd_cd(args: &[String], env: &mut Env, cfg: &Config) -> BuiltinResult {
    let target = if args.is_empty() {
        env.home()
    } else if args[0] == "-" {
        env.get("OLDPWD")
            .unwrap_or(&env.home())
            .to_string()
    } else {
        if args[0] == "~" {
            env.home()
        } else if let Some(rest) = args[0].strip_prefix("~/") {
            format!("{}/{}", env.home(), rest)
        } else {
            args[0].clone()
        }
    };

    let new_dir = Path::new(&target);
    let old = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if let Err(e) = std::env::set_current_dir(new_dir) {
        if cfg.execution.cdspell {
            if let Some(suggestion) = spell_correct_dir(&target, &old) {
                eprintln!("context: cd: {}: {}. Did you mean '{}'?", target, e, suggestion);
                return BuiltinResult::err(1);
            }
        }
        eprintln!("context: cd: {}: {}", target, e);
        return BuiltinResult::err(1);
    }

    env.set("OLDPWD", &old);
    if let Ok(cwd) = std::env::current_dir() {
        env.set("PWD", &cwd.to_string_lossy());
        if args.first().map(|s| s.as_str()) == Some("-") {
            println!("{}", cwd.to_string_lossy());
        }
    }
    BuiltinResult::ok()
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut d = vec![vec![0usize; b_len + 1]; a_len + 1];
    for (i, row) in d.iter_mut().enumerate().take(a_len + 1) { row[0] = i; }
    for (j, cell) in d[0].iter_mut().enumerate().take(b_len + 1) { *cell = j; }
    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a.as_bytes()[i - 1] == b.as_bytes()[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }
    d[a_len][b_len]
}

fn spell_correct_dir(target: &str, cwd: &str) -> Option<String> {
    let parent = if target.contains('/') {
        Path::new(target).parent().unwrap_or(Path::new(cwd))
    } else {
        Path::new(cwd)
    };
    let prefix = if target.contains('/') {
        target.rsplit('/').next().unwrap_or("")
    } else {
        target
    };
    if prefix.is_empty() {
        return None;
    }
    let entries = std::fs::read_dir(parent).ok()?;
    let threshold = if prefix.len() <= 3 { 1 } else { 2 };
    let mut best: Option<(String, usize)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let dist = levenshtein_distance(prefix, &name);
        if dist <= threshold {
            match &best {
                Some((_, best_dist)) if dist < *best_dist => {
                    let full = if target.contains('/') {
                        let dir_part = target[..target.rfind('/').unwrap() + 1].to_string();
                        format!("{}{}", dir_part, name)
                    } else {
                        name.clone()
                    };
                    best = Some((full, dist));
                }
                None => {
                    let full = if target.contains('/') {
                        let dir_part = target[..target.rfind('/').unwrap() + 1].to_string();
                        format!("{}{}", dir_part, name)
                    } else {
                        name.clone()
                    };
                    best = Some((full, dist));
                }
                _ => {}
            }
        }
    }
    best.map(|(name, _)| name)
}

fn cmd_exit(args: &[String], last_status: i32) -> BuiltinResult {
    if args.len() > 1 {
        eprintln!("context: exit: too many arguments");
        return BuiltinResult::err(2);
    }
    let code = match args.first() {
        Some(s) => match s.parse::<i32>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("context: exit: {}: numeric argument required", s);
                return BuiltinResult::exit(2);
            }
        },
        None => last_status,
    };
    BuiltinResult::exit(code)
}

fn cmd_export(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set_exported(name, value, true);
        } else {
            env.export(arg);
        }
    }
    BuiltinResult::ok()
}

fn cmd_unset(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if arg.contains('[') && arg.ends_with(']') {
            if let Some(bracket_pos) = arg.find('[') {
                let name = &arg[..bracket_pos];
                let key = &arg[bracket_pos + 1..].trim_end_matches(']');
                env.assoc_unset(name, key);
                continue;
            }
        }
        env.unset(arg);
    }
    BuiltinResult::ok()
}

fn cmd_alias(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_aliases() {
            println!("alias {}='{}'", k, v);
        }
        return BuiltinResult::ok();
    }
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            let value = value.trim_matches(|c| c == '\'' || c == '"');
            env.set_alias(name, value);
        } else {
            match env.get_alias(arg) {
                Some(v) => println!("alias {}='{}'", arg, v),
                None => {
                    eprintln!("context: alias: {}: not found", arg);
                    return BuiltinResult::err(1);
                }
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_unalias(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if env.get_alias(arg).is_none() {
            eprintln!("context: unalias: {}: not found", arg);
            return BuiltinResult::err(1);
        }
        env.unset_alias(arg);
    }
    BuiltinResult::ok()
}

fn cmd_source(args: &[String], env: &Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: source: filename argument required");
        return BuiltinResult::err(1);
    }
    let path = if args[0] == "~" {
        env.home()
    } else if let Some(rest) = args[0].strip_prefix("~/") {
        format!("{}/{}", env.home(), rest)
    } else {
        args[0].clone()
    };
    match fs::read_to_string(&path) {
        Ok(_) => BuiltinResult { status: 0, exit: false, exit_code: None, source_file: Some(path), clear_history: false, eval_string: None },
        Err(e) => {
            eprintln!("context: source: {}: {}", path, e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_history(args: &[String], cfg: &Config) -> BuiltinResult {
    let history_path = crate::config::loader::history_path(cfg);
    if args.first().map(|s| s.as_str()) == Some("-c") {
        match fs::write(&history_path, "") {
            Ok(_) => BuiltinResult { status: 0, exit: false, exit_code: None, source_file: None, clear_history: true, eval_string: None },
            Err(e) => {
                eprintln!("context: history: -c: {}", e);
                BuiltinResult::err(1)
            }
        }
    } else if args.first().map(|s| s.as_str()) == Some("-w") {
        BuiltinResult::ok()
    } else {
        match fs::read_to_string(&history_path) {
            Ok(contents) => {
                let max = args.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let lines: Vec<&str> = contents.lines().collect();
                let start = if max > 0 && max < lines.len() {
                    lines.len() - max
                } else {
                    0
                };
                for (i, line) in lines[start..].iter().enumerate() {
                    println!("{:5}  {}", start + i + 1, line);
                }
                BuiltinResult::ok()
            }
            Err(_) => BuiltinResult::ok(),
        }
    }
}

fn cmd_set(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for (k, v) in env.all_vars() {
            println!("{}={}", k, v);
        }
        return BuiltinResult::ok();
    }
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            env.set_positional(args[i..].to_vec());
            break;
        } else if arg == "-e" {
            env.set("_OPT_E", "1");
        } else if arg == "+e" {
            env.set("_OPT_E", "");
        } else if arg == "-u" {
            env.set("_OPT_U", "1");
        } else if arg == "+u" {
            env.set("_OPT_U", "");
        } else if arg == "-x" {
            env.set("_OPT_X", "1");
        } else if arg == "+x" {
            env.set("_OPT_X", "");
        } else if arg == "-a" {
            env.set("_OPT_A", "1");
        } else if arg == "+a" {
            env.set("_OPT_A", "");
        } else if arg == "-o" {
            i += 1;
            if i < args.len() {
                match args[i].as_str() {
                    "errexit" | "exitonerror" => env.set("_OPT_E", "1"),
                    "nounset" | "undefinedvariable" => env.set("_OPT_U", "1"),
                    "xtrace" | "verbose" => env.set("_OPT_X", "1"),
                    "allexport" | "all" => env.set("_OPT_A", "1"),
                    "noclobber" => env.set("_OPT_N", "1"),
                    "noglob" => env.set("_OPT_G", "1"),
                    "interactive" | "i" => {}
                    "posix" => {}
                    "nullglob" => {}
                    "pipefail" => env.set("_OPT_PIPEFAIL", "1"),
                    _ => {
                        eprintln!("context: set: -o: {}: unknown option", args[i]);
                        return BuiltinResult::err(2);
                    }
                }
            } else {
                let opt_names = [
                    ("errexit", "_OPT_E"), ("nounset", "_OPT_U"),
                    ("xtrace", "_OPT_X"), ("allexport", "_OPT_A"),
                    ("noclobber", "_OPT_N"), ("noglob", "_OPT_G"),
                    ("pipefail", "_OPT_PIPEFAIL"),
                ];
                for (name, var) in &opt_names {
                    let state = if env.get(var).map(|s| s == "1").unwrap_or(false) { "on" } else { "off" };
                    println!("-o {}={}", name, state);
                }
            }
        } else if arg == "+o" {
            i += 1;
            if i < args.len() {
                match args[i].as_str() {
                    "errexit" | "exitonerror" => env.set("_OPT_E", ""),
                    "nounset" | "undefinedvariable" => env.set("_OPT_U", ""),
                    "xtrace" | "verbose" => env.set("_OPT_X", ""),
                    "allexport" | "all" => env.set("_OPT_A", ""),
                    "noclobber" => env.set("_OPT_N", ""),
                    "noglob" => env.set("_OPT_G", ""),
                    "pipefail" => env.set("_OPT_PIPEFAIL", ""),
                    _ => {
                        eprintln!("context: set: +o: {}: unknown option", args[i]);
                        return BuiltinResult::err(2);
                    }
                }
            }
        } else if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set(name, value);
        } else if !arg.starts_with('+') && !arg.starts_with('-') {

        } else {
            eprintln!("context: set: {}: unknown option", arg);
        }
        i += 1;
    }
    BuiltinResult::ok()
}

fn cmd_unsetenv(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        env.unset(arg);
    }
    BuiltinResult::ok()
}

fn cmd_env(env: &Env, cfg: &Config) -> BuiltinResult {
    let mask = cfg.security.mask_secrets;
    for (k, v) in env.all_vars() {
        if mask && (k.contains("SECRET") || k.contains("TOKEN") || k.contains("PASSWORD") || k.contains("API_KEY") || k.contains("PRIVATE")) {
            println!("{}=***", k);
        } else {
            println!("{}={}", k, v);
        }
    }
    BuiltinResult::ok()
}

fn cmd_pwd() -> BuiltinResult {
    match std::env::current_dir() {
        Ok(p) => {
            println!("{}", p.display());
            BuiltinResult::ok()
        }
        Err(e) => {
            eprintln!("context: pwd: {}", e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_type(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: type: name argument required");
        return BuiltinResult::err(1);
    }
    let builtins = ["cd","exit","export","unset","alias","unalias","source","history","set","env","pwd","type","which","echo","printf","test","let","trap","pushd","popd","dirs","hash","math","regexmatch","module","readonly","builtin","shopt","enable","declare","typeset","local","exec",".","false","true","wait","kill","umask","command","eval","select","getopts","realpath"];
    for arg in args {
        if builtins.contains(&arg.as_str()) {
            println!("{} is a shell builtin", arg);
        } else if let Some(path) = find_in_path(arg) {
            println!("{} is {}", arg, path);
        } else {
            eprintln!("context: type: {}: not found", arg);
            return BuiltinResult::err(1);
        }
    }
    BuiltinResult::ok()
}

fn cmd_which(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let mut status = 0;
    for arg in args {
        if let Some(path) = find_in_path(arg) {
            println!("{}", path);
        } else {
            eprintln!("context: which: {}: not found", arg);
            status = 1;
        }
    }
    BuiltinResult::err(status)
}

fn find_in_path(cmd: &str) -> Option<String> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        let full = Path::new(dir).join(cmd);
        if full.is_file() {
            return Some(full.to_string_lossy().to_string());
        }
    }
    None
}

fn cmd_echo(args: &[String]) -> BuiltinResult {
    let mut start = 0;
    let mut newline = true;
    let mut escape = false;

    if !args.is_empty() && args[0].starts_with('-') && args[0].len() > 1 && !args[0].contains(' ') {
        let flag_str = &args[0][1..];
        let mut valid = true;
        for ch in flag_str.chars() {
            match ch {
                'n' => newline = false,
                'e' => escape = true,
                'E' => escape = false,
                _ => { valid = false; break; }
            }
        }
        if valid {
            start = 1;
        }
    }

    let mut output = String::new();
    for (i, arg) in args[start..].iter().enumerate() {
        if i > 0 { output.push(' '); }
        if escape {
            output.push_str(&escape_echo(arg));
        } else {
            output.push_str(arg);
        }
    }
    if newline {
        output.push('\n');
    }
    print!("{}", output);
    BuiltinResult::ok()
}

fn escape_echo(s: &str) -> String {
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
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                'a' => result.push('\x07'),
                'b' => result.push('\x08'),
                'e' => result.push('\x1b'),
                'f' => result.push('\x0c'),
                'v' => result.push('\x0b'),
                '0' => {
                    if i + 1 < len && chars[i + 1] == 'x' {
                        i += 2;
                        let mut hex = String::new();
                        while i < len && chars[i].is_ascii_hexdigit() {
                            hex.push(chars[i]);
                            i += 1;
                        }
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                        }
                    } else {
                        let mut oct = String::new();
                        i += 1;
                        while i < len && oct.len() < 3 && matches!(chars[i], '0'..='7') {
                            oct.push(chars[i]);
                            i += 1;
                        }
                        if !oct.is_empty() {
                            if let Ok(byte) = u8::from_str_radix(&oct, 8) {
                                result.push(byte as char);
                            }
                        }
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

fn cmd_printf(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let format = &args[0];
    let fmt_chars: Vec<char> = format.chars().collect();
    let fmt_len = fmt_chars.len();
    let mut output = String::new();
    let mut arg_idx = 0;
    let mut i = 0;
    while i < fmt_len {
        if fmt_chars[i] == '\\' && i + 1 < fmt_len {
            i += 1;
            match fmt_chars[i] {
                'n' => output.push('\n'),
                't' => output.push('\t'),
                'r' => output.push('\r'),
                '\\' => output.push('\\'),
                'a' => output.push('\x07'),
                'b' => output.push('\x08'),
                'e' => output.push('\x1b'),
                'f' => output.push('\x0c'),
                'v' => output.push('\x0b'),
                '0' => {
                    if i + 1 < fmt_len && fmt_chars[i + 1] == 'x' {
                        i += 2;
                        let mut hex = String::new();
                        while i < fmt_len && fmt_chars[i].is_ascii_hexdigit() {
                            hex.push(fmt_chars[i]);
                            i += 1;
                        }
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            output.push(byte as char);
                        }
                    }
                    continue;
                }
                _ => {
                    output.push('\\');
                    output.push(fmt_chars[i]);
                }
            }
        } else if fmt_chars[i] == '%' && i + 1 < fmt_len {
            i += 1;
            if fmt_chars[i] == '%' {
                output.push('%');
            } else {
                let mut left_align = false;
                if fmt_chars[i] == '-' {
                    left_align = true;
                    i += 1;
                }
                let mut zero_pad = false;
                if i < fmt_len && fmt_chars[i] == '0' {
                    zero_pad = true;
                    i += 1;
                }
                let mut width: usize = 0;
                while i < fmt_len && fmt_chars[i].is_ascii_digit() {
                    width = width * 10 + (fmt_chars[i] as usize - '0' as usize);
                    i += 1;
                }
                let mut precision: Option<usize> = None;
                if i < fmt_len && fmt_chars[i] == '.' {
                    i += 1;
                    let mut prec: usize = 0;
                    while i < fmt_len && fmt_chars[i].is_ascii_digit() {
                        prec = prec * 10 + (fmt_chars[i] as usize - '0' as usize);
                        i += 1;
                    }
                    precision = Some(prec);
                }
                if i >= fmt_len {
                    output.push('%');
                    continue;
                }
                match fmt_chars[i] {
                    's' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                        let s = match precision {
                            Some(p) => if val.len() > p { &val[..p] } else { val },
                            None => val,
                        };
                        if s.len() < width {
                            let pad = " ".repeat(width - s.len());
                            if left_align {
                                output.push_str(s);
                                output.push_str(&pad);
                            } else {
                                output.push_str(&pad);
                                output.push_str(s);
                            }
                        } else {
                            output.push_str(s);
                        }
                        arg_idx += 1;
                    }
                    'd' | 'i' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        let s = format!("{}", n);
                        let padded = if s.len() < width {
                            let fill = if zero_pad { "0" } else { " " };
                            let pad_len = width - s.len();
                            let pad: String = fill.repeat(pad_len);
                            if left_align { format!("{}{}", s, pad) } else { format!("{}{}", pad, s) }
                        } else {
                            s
                        };
                        output.push_str(&padded);
                        arg_idx += 1;
                    }
                    'x' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        let s = format!("{:x}", n);
                        let padded = if s.len() < width {
                            let fill = if zero_pad { "0" } else { " " };
                            let pad_len = width - s.len();
                            let pad: String = fill.repeat(pad_len);
                            if left_align { format!("{}{}", s, pad) } else { format!("{}{}", pad, s) }
                        } else {
                            s
                        };
                        output.push_str(&padded);
                        arg_idx += 1;
                    }
                    'o' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        let s = format!("{:o}", n);
                        let padded = if s.len() < width {
                            let fill = if zero_pad { "0" } else { " " };
                            let pad_len = width - s.len();
                            let pad: String = fill.repeat(pad_len);
                            if left_align { format!("{}{}", s, pad) } else { format!("{}{}", pad, s) }
                        } else {
                            s
                        };
                        output.push_str(&padded);
                        arg_idx += 1;
                    }
                    'f' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        let n: f64 = val.parse().unwrap_or(0.0);
                        let prec = precision.unwrap_or(6);
                        let s = format!("{:.prec$}", n, prec = prec);
                        let padded = if s.len() < width {
                            let fill = if zero_pad { "0" } else { " " };
                            let pad_len = width - s.len();
                            let pad: String = fill.repeat(pad_len);
                            if left_align { format!("{}{}", s, pad) } else { format!("{}{}", pad, s) }
                        } else {
                            s
                        };
                        output.push_str(&padded);
                        arg_idx += 1;
                    }
                    'e' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("0");
                        let n: f64 = val.parse().unwrap_or(0.0);
                        let prec = precision.unwrap_or(6);
                        let s = format!("{:.prec$e}", n, prec = prec);
                        output.push_str(&s);
                        arg_idx += 1;
                    }
                    'c' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                        if let Some(ch) = val.chars().next() {
                            output.push(ch);
                        }
                        arg_idx += 1;
                    }
                    'b' => {
                        let val = args.get(arg_idx + 1).map(|s| s.as_str()).unwrap_or("");
                        output.push_str(&escape_echo(val));
                        arg_idx += 1;
                    }
                    _ => {
                        output.push('%');
                        output.push(fmt_chars[i]);
                    }
                }
            }
        } else {
            output.push(fmt_chars[i]);
        }
        i += 1;
    }
    print!("{}", output);
    BuiltinResult::ok()
}

fn cmd_test(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::err(2);
    }
    if args.len() == 1 {
        return if args[0].is_empty() { BuiltinResult::err(1) } else { BuiltinResult::ok() };
    }
    if args.len() == 3 {
        let op = &args[1];
        let a = &args[0];
        let b = &args[2];
        return match op.as_str() {
            "=" | "==" => BuiltinResult::if_bool(a == b),
            "!=" => BuiltinResult::if_bool(a != b),
            "-eq" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a == b)
            }
            "-ne" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a != b)
            }
            "-lt" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a < b)
            }
            "-le" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a <= b)
            }
            "-gt" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a > b)
            }
            "-ge" => {
                let a: i64 = a.parse().unwrap_or(0);
                let b: i64 = b.parse().unwrap_or(0);
                BuiltinResult::if_bool(a >= b)
            }
            "-nt" => {
                let pa = std::path::Path::new(a);
                let pb = std::path::Path::new(b);
                match (pa.metadata(), pb.metadata()) {
                    (Ok(ma), Ok(mb)) => {
                        let ta = ma.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let tb = mb.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        BuiltinResult::if_bool(ta > tb)
                    }
                    _ => BuiltinResult::err(1),
                }
            }
            "-ot" => {
                let pa = std::path::Path::new(a);
                let pb = std::path::Path::new(b);
                match (pa.metadata(), pb.metadata()) {
                    (Ok(ma), Ok(mb)) => {
                        let ta = ma.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let tb = mb.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        BuiltinResult::if_bool(ta < tb)
                    }
                    _ => BuiltinResult::err(1),
                }
            }
            _ => BuiltinResult::err(2),
        };
    }
    if args.len() == 2 {
        let op = &args[0];
        let a = &args[1];
        return match op.as_str() {
            "-z" => BuiltinResult::if_bool(a.is_empty()),
            "-n" => BuiltinResult::if_bool(!a.is_empty()),
            "-e" => BuiltinResult::if_bool(std::path::Path::new(a).exists()),
            "-f" => BuiltinResult::if_bool(std::path::Path::new(a).is_file()),
            "-d" => BuiltinResult::if_bool(std::path::Path::new(a).is_dir()),
            "-r" => {
                let c_path = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                BuiltinResult::if_bool(unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 })
            }
            "-w" => {
                let c_path = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                if unsafe { libc::stat(c_path.as_ptr(), &mut st) } == 0 {
                    let euid = unsafe { libc::geteuid() };
                    let egid = unsafe { libc::getegid() };
                    let writable = if euid == 0 {
                        true
                    } else if st.st_uid == euid {
                        (st.st_mode & libc::S_IWUSR) != 0
                    } else if st.st_gid == egid {
                        (st.st_mode & libc::S_IWGRP) != 0
                    } else {
                        (st.st_mode & libc::S_IWOTH) != 0
                    };
                    BuiltinResult::if_bool(writable)
                } else {
                    BuiltinResult::err(1)
                }
            }
            "-x" => {
                let c_path = std::ffi::CString::new(a.as_str()).unwrap_or_default();
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                if unsafe { libc::stat(c_path.as_ptr(), &mut st) } == 0 {
                    let euid = unsafe { libc::geteuid() };
                    let egid = unsafe { libc::getegid() };
                    let executable = if euid == 0 {
                        true
                    } else if st.st_uid == euid {
                        (st.st_mode & libc::S_IXUSR) != 0
                    } else if st.st_gid == egid {
                        (st.st_mode & libc::S_IXGRP) != 0
                    } else {
                        (st.st_mode & libc::S_IXOTH) != 0
                    };
                    BuiltinResult::if_bool(executable)
                } else {
                    BuiltinResult::err(1)
                }
            }
            "-s" => BuiltinResult::if_bool(std::fs::metadata(a).map(|m| m.len() > 0).unwrap_or(false)),
            "-L" | "-h" => BuiltinResult::if_bool(std::path::Path::new(a).is_symlink()),
            "-S" => BuiltinResult::if_bool(
                std::fs::metadata(a)
                    .map(|m| m.file_type().is_socket())
                    .unwrap_or(false)
            ),
            "-p" => BuiltinResult::if_bool(
                std::fs::metadata(a)
                    .map(|m| m.file_type().is_fifo())
                    .unwrap_or(false)
            ),
            "-c" => BuiltinResult::if_bool(
                std::fs::metadata(a)
                    .map(|m| m.file_type().is_char_device())
                    .unwrap_or(false)
            ),
            "!" => BuiltinResult::if_bool(a.is_empty()),
            _ => BuiltinResult::err(2),
        };
    }
    BuiltinResult::err(2)
}

fn cmd_let(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            let result = eval_arithmetic(value, env);
            env.set(name, &result.to_string());
        }
    }
    BuiltinResult::ok()
}

struct ArithmeticParser {
    chars: Vec<char>,
    pos: usize,
    env: *const Env,
}

impl ArithmeticParser {
    fn new(expr: &str, env: *const Env) -> Self {
        Self {
            chars: expr.chars().collect(),
            pos: 0,
            env,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_expr(&mut self) -> i64 {
        let mut result = self.parse_term();
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '+' || op == '-' {
                self.advance();
                let rhs = self.parse_term();
                if op == '+' { result += rhs; } else { result -= rhs; }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_term(&mut self) -> i64 {
        let mut result = self.parse_factor();
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '*' || op == '/' || op == '%' {
                self.advance();
                let rhs = self.parse_factor();
                match op {
                    '*' => result *= rhs,
                    '/' => { if rhs != 0 { result /= rhs; } }
                    '%' if rhs != 0 => { result %= rhs; }
                    _ => {}
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        result
    }

    fn parse_factor(&mut self) -> i64 {
        self.skip_whitespace();
        if let Some(ch) = self.peek() {
            if ch == '(' {
                self.advance();
                let val = self.parse_expr();
                self.skip_whitespace();
                if self.peek() == Some(')') {
                    self.advance();
                }
                return val;
            }
            if ch == '+' {
                self.advance();
                return self.parse_factor();
            }
            if ch == '-' {
                self.advance();
                return -self.parse_factor();
            }
            if ch.is_ascii_digit() || ch == '\'' {
                return self.parse_number();
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                return self.parse_variable();
            }
        }
        0
    }

    fn parse_number(&mut self) -> i64 {
        let start = self.pos;
        if self.peek() == Some('0') && self.pos + 1 < self.chars.len() {
            let next = self.chars[self.pos + 1];
            if next == 'x' || next == 'X' {
                self.advance();
                self.advance();
                let mut val: i64 = 0;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_hexdigit() {
                        val = val * 16 + ch.to_digit(16).unwrap() as i64;
                        self.advance();
                    } else {
                        break;
                    }
                }
                return val;
            }
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<i64>().unwrap_or(0)
    }

    fn parse_variable(&mut self) -> i64 {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        if name == "RANDOM" {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            std::time::SystemTime::now().hash(&mut hasher);
            return (hasher.finish() % 32768) as i64;
        }
        unsafe {
            self.env.as_ref().and_then(|e| e.get(&name))
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0)
        }
    }
}

fn eval_arithmetic(expr: &str, env: &Env) -> i64 {
    let mut parser = ArithmeticParser::new(expr, env as *const Env);
    parser.parse_expr()
}

fn cmd_trap(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        let traps = env.all_traps();
        if traps.is_empty() {
            return BuiltinResult::ok();
        }
        let mut entries: Vec<_> = traps.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (signal, command) in entries {
            if command.is_empty() {
                println!("trap -- '' {}", signal);
            } else {
                println!("trap -- '{}' {}", command, signal);
            }
        }
        return BuiltinResult::ok();
    }

    if args.len() == 1 {
        eprintln!("context: trap: signal argument required");
        return BuiltinResult::err(1);
    }

    let command = &args[0];
    let signal = &args[1];

    if command == "-" {
        env.remove_trap(signal);
    } else {
        let command = command.trim_matches(|c| c == '\'' || c == '"');
        env.set_trap(signal, command);
    }
    BuiltinResult::ok()
}

fn cmd_pushd(args: &[String], env: &mut Env) -> BuiltinResult {
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let mut stack: Vec<String> = dirs_str.split(':').filter(|s| !s.is_empty()).map(String::from).collect();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut swap = false;
    let mut target: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--" {
            i += 1;
            if i < args.len() {
                target = Some(if args[i] == "~" {
                    env.home()
                } else if let Some(rest) = args[i].strip_prefix("~/") {
                    format!("{}/{}", env.home(), rest)
                } else {
                    args[i].clone()
                });
                i += 1;
            }
            continue;
        }
        if args[i] == "+n" || (args[i].starts_with('+') && args[i].len() > 1) {
            let n: usize = args[i][1..].parse().unwrap_or(0);
            stack.insert(0, cwd.clone());
            if n < stack.len() {
                let val = stack.remove(n);
                stack.insert(0, val);
            }
            env.set("DIRSTACK", &stack.join(":"));
            i += 1;
            continue;
        }
        if args[i] == "-n" || (args[i].starts_with('-') && args[i].len() > 1 && args[i][1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
            let n: usize = args[i][1..].parse().unwrap_or(0);
            stack.insert(0, cwd.clone());
            let idx = if n < stack.len() { stack.len() - 1 - n } else { 0 };
            let val = stack.remove(idx);
            stack.insert(0, val);
            env.set("DIRSTACK", &stack.join(":"));
            i += 1;
            continue;
        }
        target = Some(if args[i] == "~" {
            env.home()
        } else if let Some(rest) = args[i].strip_prefix("~/") {
            format!("{}/{}", env.home(), rest)
        } else {
            args[i].clone()
        });
        i += 1;
    }

    if target.is_none() && args.is_empty() {
        swap = true;
    }

    if swap {
        if stack.is_empty() {
            eprintln!("context: pushd: no other directory yet");
            return BuiltinResult::err(1);
        }
        let top = stack.remove(0);
        stack.insert(0, cwd.clone());
        stack.insert(0, top.clone());
        env.set("DIRSTACK", &stack.join(":"));
        if let Err(e) = std::env::set_current_dir(&top) {
            eprintln!("context: pushd: {}: {}", top, e);
            return BuiltinResult::err(1);
        }
        return BuiltinResult::ok();
    }

    if let Some(dir) = target {
        stack.insert(0, cwd.clone());
        env.set("DIRSTACK", &stack.join(":"));
        if let Err(e) = std::env::set_current_dir(&dir) {
            eprintln!("context: pushd: {}: {}", dir, e);
            return BuiltinResult::err(1);
        }
        if let Ok(cwd) = std::env::current_dir() {
            println!("{}", cwd.display());
        }
    }
    BuiltinResult::ok()
}

fn cmd_popd(args: &[String], env: &mut Env) -> BuiltinResult {
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let mut stack: Vec<String> = dirs_str.split(':').filter(|s| !s.is_empty()).map(String::from).collect();

    if args.is_empty() {
        match stack.pop() {
            Some(dir) => {
                env.set("DIRSTACK", &stack.join(":"));
                if let Err(e) = std::env::set_current_dir(&dir) {
                    eprintln!("context: popd: {}: {}", dir, e);
                    return BuiltinResult::err(1);
                }
                if let Ok(cwd) = std::env::current_dir() {
                    println!("{}", cwd.display());
                }
                BuiltinResult::ok()
            }
            None => {
                eprintln!("context: popd: directory stack empty");
                BuiltinResult::err(1)
            }
        }
    } else {
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--" {
                i += 1;
                continue;
            }
            if args[i] == "+n" || (args[i].starts_with('+') && args[i].len() > 1) {
                let n: usize = args[i][1..].parse().unwrap_or(0);
                if n < stack.len() {
                    let dir = stack.remove(n);
                    if let Err(e) = std::env::set_current_dir(&dir) {
                        eprintln!("context: popd: {}: {}", dir, e);
                        return BuiltinResult::err(1);
                    }
                } else {
                    eprintln!("context: popd: {} not in directory stack", args[i]);
                    return BuiltinResult::err(1);
                }
            } else if args[i] == "-n" || (args[i].starts_with('-') && args[i].len() > 1 && args[i][1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
                let n: usize = args[i][1..].parse().unwrap_or(0);
                if n < stack.len() {
                    let idx = stack.len() - 1 - n;
                    let dir = stack.remove(idx);
                    if let Err(e) = std::env::set_current_dir(&dir) {
                        eprintln!("context: popd: {}: {}", dir, e);
                        return BuiltinResult::err(1);
                    }
                } else {
                    eprintln!("context: popd: {} not in directory stack", args[i]);
                    return BuiltinResult::err(1);
                }
            } else {
                eprintln!("context: popd: unknown option: {}", args[i]);
                return BuiltinResult::err(1);
            }
            i += 1;
        }
        env.set("DIRSTACK", &stack.join(":"));
        if let Ok(cwd) = std::env::current_dir() {
            println!("{}", cwd.display());
        }
        BuiltinResult::ok()
    }
}

fn cmd_dirs(args: &[String], env: &mut Env) -> BuiltinResult {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let home = env.home();
    let dirs_str = env.get("DIRSTACK").unwrap_or("").to_string();
    let stack: Vec<&str> = dirs_str.split(':').filter(|s| !s.is_empty()).collect();
    let mut numbered = false;
    let mut long_paths = false;
    let mut one_per_line = false;
    let mut show_named = false;
    let mut clear_named = false;

    for arg in args {
        match arg.as_str() {
            "-v" => numbered = true,
            "-l" => long_paths = true,
            "-p" => one_per_line = true,
            "-n" => show_named = true,
            "-c" => clear_named = true,
            "-" => { long_paths = false; numbered = false; one_per_line = false; }
            _ => {}
        }
    }

    if clear_named {
        let names: Vec<String> = env.all_named_dirs().keys().cloned().collect();
        for name in &names {
            env.unset_named_dir(name);
        }
        return BuiltinResult::ok();
    }

    if show_named {
        let dirs = env.all_named_dirs();
        let mut entries: Vec<(String, String)> = dirs.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, path) in &entries {
            println!("{}={}", name, path);
        }
        return BuiltinResult::ok();
    }

    let mut entries: Vec<String> = Vec::new();
    let mut numbered_entries: Vec<(usize, String)> = Vec::new();

    if long_paths {
        entries.push(cwd.clone());
        numbered_entries.push((0, cwd.clone()));
        for (i, d) in stack.iter().enumerate() {
            entries.push(d.to_string());
            numbered_entries.push((i + 1, d.to_string()));
        }
    } else {
        let short = cwd.replacen(&home, "~", 1);
        entries.push(short.clone());
        numbered_entries.push((0, short));
        for (i, d) in stack.iter().enumerate() {
            let short = d.replacen(&home, "~", 1);
            entries.push(short.clone());
            numbered_entries.push((i + 1, short));
        }
    }

    if numbered {
        let output: Vec<String> = numbered_entries.iter()
            .map(|(i, d)| format!("{} {}", i, d))
            .collect();
        if one_per_line {
            println!("{}", output.join("\n"));
        } else {
            println!("{}", output.join(" "));
        }
    } else {
        if one_per_line {
            println!("{}", entries.join("\n"));
        } else {
            println!("{}", entries.join(" "));
        }
    }
    BuiltinResult::ok()
}

fn cmd_hash(args: &[String]) -> BuiltinResult {
    use std::sync::LazyLock;
    static PATH_CACHE: LazyLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

    if args.is_empty() {
        let cache = PATH_CACHE.lock().unwrap();
        if cache.is_empty() {
            return BuiltinResult::ok();
        }
        let mut entries: Vec<_> = cache.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (name, path) in entries {
            println!("{}={}", name, path);
        }
        return BuiltinResult::ok();
    }
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-r" {
            PATH_CACHE.lock().unwrap().clear();
            i += 1;
        } else if args[i] == "-p" {
            i += 1;
            if i < args.len() {
                let name = &args[i];
                if let Some(path) = find_in_path(name) {
                    PATH_CACHE.lock().unwrap().insert(name.to_string(), path);
                } else {
                    eprintln!("context: hash: {}: not found", name);
                }
                i += 1;
            }
        } else if args[i] == "-d" {
            eprintln!("context: hash: -d is a context extension, use 'hash -p' for command caching");
            return BuiltinResult::err(1);
        } else {
            let name = &args[i];
            if let Some(path) = find_in_path(name) {
                PATH_CACHE.lock().unwrap().insert(name.to_string(), path);
            } else {
                eprintln!("context: hash: {}: not found", name);
            }
            i += 1;
        }
    }
    BuiltinResult::ok()
}

fn cmd_math(args: &[String], env: &mut Env, cfg: &Config) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: math: expression required");
        return BuiltinResult::err(1);
    }
    let expr = args.join(" ");
    let expr = expand_env_in_math(&expr, env);
    match eval_float(&expr) {
        Ok(val) => {
            let precision = cfg.execution.float_precision;
            let formatted = format!("{:.prec$}", val, prec = precision);
            let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
            println!("{}", formatted);
            env.set("_", formatted);
        }
        Err(e) => {
            eprintln!("context: math: {}", e);
            return BuiltinResult::err(1);
        }
    }
    BuiltinResult::ok()
}

fn expand_env_in_math(expr: &str, env: &Env) -> String {
    let mut result = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut name = String::new();
            name.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(val) = env.get(&name) {
                result.push_str(val);
            } else {
                result.push_str(&name);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn eval_float(expr: &str) -> Result<f64, String> {
    let mut parser = FloatParser::new(expr);
    let val = parser.parse_add_sub()?;
    Ok(val)
}

struct FloatParser {
    chars: Vec<char>,
    pos: usize,
}

impl FloatParser {
    fn new(expr: &str) -> Self {
        Self { chars: expr.chars().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() { self.pos += 1; }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() { self.pos += 1; } else { break; }
        }
    }

    fn parse_add_sub(&mut self) -> Result<f64, String> {
        let mut result = self.parse_mul_div()?;
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '+' || op == '-' {
                self.advance();
                let rhs = self.parse_mul_div()?;
                if op == '+' { result += rhs; } else { result -= rhs; }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        Ok(result)
    }

    fn parse_mul_div(&mut self) -> Result<f64, String> {
        let mut result = self.parse_unary()?;
        self.skip_whitespace();
        while let Some(op) = self.peek() {
            if op == '*' || op == '/' || op == '%' {
                self.advance();
                let rhs = self.parse_unary()?;
                match op {
                    '*' => result *= rhs,
                    '/' => {
                        if rhs == 0.0 { return Err("division by zero".into()); }
                        result /= rhs;
                    }
                    '%' => {
                        if rhs == 0.0 { return Err("division by zero".into()); }
                        result %= rhs;
                    }
                    _ => {}
                }
                self.skip_whitespace();
            } else {
                break;
            }
        }
        Ok(result)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        if let Some(ch) = self.peek() {
            if ch == '+' {
                self.advance();
                return self.parse_unary();
            }
            if ch == '-' {
                self.advance();
                return self.parse_unary().map(|v| -v);
            }
            if ch == '(' {
                self.advance();
                let val = self.parse_add_sub()?;
                self.skip_whitespace();
                if self.peek() == Some(')') { self.advance(); }
                return Ok(val);
            }
        }
        self.parse_number()
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E'
                || ch == '+' && (self.chars.get(self.pos.wrapping_sub(1)) == Some(&'e') || self.chars.get(self.pos.wrapping_sub(1)) == Some(&'E'))
                || ch == '-' && (self.chars.get(self.pos.wrapping_sub(1)) == Some(&'e') || self.chars.get(self.pos.wrapping_sub(1)) == Some(&'E'))
            {
                self.advance();
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(format!("unexpected character: {:?}", self.peek()));
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f64>().map_err(|e| format!("parse error: {}", e))
    }
}

fn cmd_regexmatch(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.len() < 2 {
        eprintln!("context: regexmatch: usage: regexmatch string pattern [capture_var ...]");
        return BuiltinResult::err(1);
    }
    let string = &args[0];
    let pattern = &args[1];
    let rest = &args[2..];
    match regex::Regex::new(pattern) {
        Ok(re) => {
            if let Some(caps) = re.captures(string) {
                for (i, m) in caps.iter().enumerate() {
                    if let Some(mat) = m {
                        let var_name = if i < rest.len() { rest[i].as_str() } else { &format!("MATCH_{}", i) };
                        env.set(var_name, mat.as_str());
                    }
                }
                env.set("MATCH", string);
                BuiltinResult::ok()
            } else {
                BuiltinResult::err(1)
            }
        }
        Err(e) => {
            eprintln!("context: regexmatch: invalid pattern: {}", e);
            BuiltinResult::err(1)
        }
    }
}

fn cmd_module(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: module: usage: module load|unload|list|info <name>");
        return BuiltinResult::err(1);
    }
    let modules_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".config"))
        .join("context/modules");
    let _ = std::fs::create_dir_all(&modules_dir);

    match args[0].as_str() {
        "list" => {
            match std::fs::read_dir(&modules_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let init = entry.path().join("init.context");
                        let status = if init.exists() { "loaded" } else { "no init" };
                        println!("{} ({})", name, status);
                    }
                }
                Err(_) => println!("No modules directory found"),
            }
            BuiltinResult::ok()
        }
        "load" => {
            if args.len() < 2 {
                eprintln!("context: module: load requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let init_path = modules_dir.join(name).join("init.context");
            if !init_path.exists() {
                eprintln!("context: module: {}: not found", name);
                return BuiltinResult::err(1);
            }
            match std::fs::read_to_string(&init_path) {
                Ok(contents) => {
                    env.set("_LOADED_MODULE", name);
                    let module_env = env.clone();
                    let tokens = crate::shell::lexer::tokenize(&contents);
                    let ast = crate::shell::parser::parse(tokens);
                    crate::shell::executor::Executor::new(module_env.clone(), crate::config::Config::default()).execute(&ast);
                    env.merge_from(&module_env);
                    println!("Module {} loaded", name);
                    BuiltinResult::ok()
                }
                Err(e) => {
                    eprintln!("context: module: {}: {}", init_path.display(), e);
                    BuiltinResult::err(1)
                }
            }
        }
        "unload" => {
            if args.len() < 2 {
                eprintln!("context: module: unload requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let fini_path = modules_dir.join(name).join("fini.context");
            if fini_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&fini_path) {
                    let tokens = crate::shell::lexer::tokenize(&contents);
                    let ast = crate::shell::parser::parse(tokens);
                    crate::shell::executor::Executor::new(env.clone(), crate::config::Config::default()).execute(&ast);
                }
            }
            println!("Module {} unloaded", name);
            BuiltinResult::ok()
        }
        "info" => {
            if args.len() < 2 {
                eprintln!("context: module: info requires a module name");
                return BuiltinResult::err(1);
            }
            let name = &args[1];
            let module_dir = modules_dir.join(name);
            if !module_dir.exists() {
                eprintln!("context: module: {}: not found", name);
                return BuiltinResult::err(1);
            }
            let desc_path = module_dir.join("README");
            if desc_path.exists() {
                if let Ok(desc) = std::fs::read_to_string(&desc_path) {
                    println!("{}", desc);
                }
            } else {
                println!("Module: {}", name);
                let init = module_dir.join("init.context");
                let fini = module_dir.join("fini.context");
                println!("  init.context: {}", if init.exists() { "yes" } else { "no" });
                println!("  fini.context: {}", if fini.exists() { "yes" } else { "no" });
            }
            BuiltinResult::ok()
        }
        _ => {
            eprintln!("context: module: unknown subcommand: {}", args[0]);
            eprintln!("context: module: usage: load|unload|list|info <name>");
            BuiltinResult::err(1)
        }
    }
}

fn cmd_readonly(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        for k in env.all_vars().keys() {
            println!("readonly {}", k);
        }
        return BuiltinResult::ok();
    }
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set(name, value);
            env.set_readonly(name);
        } else {
            env.set_readonly(arg);
        }
    }
    BuiltinResult::ok()
}

fn cmd_declare(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut export = false;
    let mut readonly = false;
    let mut assoc = false;
    let mut vars = Vec::new();
    for arg in args {
        if arg == "-x" || arg == "--export" {
            export = true;
        } else if arg == "-r" || arg == "--readonly" {
            readonly = true;
        } else if arg == "-A" || arg == "--assoc" {
            assoc = true;
        } else {
            vars.push(arg.as_str());
        }
    }
    for var in vars {
        if assoc && !var.contains('=') {
            env.create_assoc_array(var);
            continue;
        }
        if let Some(eq_pos) = var.find('=') {
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];
            if assoc && name.contains('[') {
                if let Some(bracket_pos) = name.find('[') {
                    let arr_name = &name[..bracket_pos];
                    let key = &name[bracket_pos + 1..].trim_end_matches(']');
                    env.create_assoc_array(arr_name);
                    env.assoc_set(arr_name, key, value);
                    continue;
                }
            }
            env.set_exported(name, value, export);
            if readonly {
                env.set_readonly(name);
            }
        } else {
            if assoc {
                env.create_assoc_array(var);
            } else {
                env.export(var);
                if readonly {
                    env.set_readonly(var);
                }
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_wait(args: &[String]) -> BuiltinResult {
    let mut nonblock = false;
    let mut pid_args: Vec<&String> = Vec::new();
    for arg in args {
        if arg == "-n" {
            nonblock = true;
        } else {
            pid_args.push(arg);
        }
    }
    if pid_args.is_empty() {
        let mut last_status = 0;
        loop {
            let mut status: i32 = 0;
            let flags = if nonblock { libc::WNOHANG } else { 0 };
            let ret = unsafe { libc::waitpid(-1, &mut status, flags) };
            if ret <= 0 { break; }
            last_status = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
                         else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
                         else if libc::WIFSTOPPED(status) { 128 + libc::WSTOPSIG(status) }
                         else { 1 };
        }
        return BuiltinResult::err(last_status);
    }
    let mut last_status = 0;
    for arg in pid_args {
        if let Ok(pid) = arg.parse::<i32>() {
            let flags = if nonblock { libc::WNOHANG } else { 0 };
            let mut status: i32 = 0;
            let ret = unsafe { libc::waitpid(pid, &mut status, flags) };
            if ret == -1 {
                eprintln!("context: wait: {}: no such child", pid);
                return BuiltinResult::err(127);
            }
            if ret > 0 {
                last_status = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
                             else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
                             else if libc::WIFSTOPPED(status) { 128 + libc::WSTOPSIG(status) }
                             else { 1 };
            }
        } else {
            eprintln!("context: wait: {}: invalid pid", arg);
            return BuiltinResult::err(127);
        }
    }
    BuiltinResult::err(last_status)
}

fn cmd_kill(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: kill: usage: kill [-s SIGSPEC | -n SIGNUM | -SIGSPEC] pid | jobspec ...");
        return BuiltinResult::err(1);
    }
    let mut signal: i32 = libc::SIGTERM;
    let mut i = 0;
    if args[0].starts_with('-') && !args[0].chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(true) {
        let sig = &args[0][1..];
        signal = match sig {
            "HUP" | "1" => libc::SIGHUP,
            "INT" | "2" => libc::SIGINT,
            "QUIT" | "3" => libc::SIGQUIT,
            "KILL" | "9" => libc::SIGKILL,
            "TERM" | "15" => libc::SIGTERM,
            "CONT" | "18" => libc::SIGCONT,
            "STOP" | "19" => libc::SIGSTOP,
            "TSTP" | "20" => libc::SIGTSTP,
            "USR1" | "10" => libc::SIGUSR1,
            "USR2" | "12" => libc::SIGUSR2,
            _ => {
                if let Ok(n) = sig.parse::<i32>() { n } else {
                    eprintln!("context: kill: {}: invalid signal specification", sig);
                    return BuiltinResult::err(1);
                }
            }
        };
        i = 1;
    } else if args[0].starts_with('-') {
        if let Ok(n) = args[0][1..].parse::<i32>() {
            signal = n;
            i = 1;
        }
    }
    if i >= args.len() {
        eprintln!("context: kill: usage: kill [-s SIGSPEC | -n SIGNUM | -SIGSPEC] pid | jobspec ...");
        return BuiltinResult::err(1);
    }
    let mut status = 0;
    for arg in &args[i..] {
        if let Ok(pid) = arg.parse::<i32>() {
            let ret = unsafe { libc::kill(pid, signal) };
            if ret == -1 {
                eprintln!("context: kill: ({}) - {}", pid, std::io::Error::last_os_error());
                status = 1;
            }
        } else {
            eprintln!("context: kill: {}: invalid pid", arg);
            status = 1;
        }
    }
    BuiltinResult::err(status)
}

fn cmd_umask(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        let mask = unsafe { libc::umask(0) };
        unsafe { libc::umask(mask); }
        println!("{:04o}", mask);
        return BuiltinResult::ok();
    }
    let mask_str = &args[0];
    match u32::from_str_radix(mask_str, 8) {
        Ok(mask) => {
            unsafe { libc::umask(mask as libc::mode_t); }
            BuiltinResult::ok()
        }
        Err(_) => {
            eprintln!("context: umask: {}: invalid octal number", mask_str);
            BuiltinResult::err(2)
        }
    }
}

fn cmd_command(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let mut i = 0;
    let mut use_posix_path = false;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => { use_posix_path = true; i += 1; }
            "-v" => {
                if i + 1 < args.len() {
                    let name = &args[i + 1];
                    if let Some(path) = find_in_path(name) {
                        println!("{}", path);
                    } else {
                        eprintln!("context: command: {}: not found", name);
                        return BuiltinResult::err(1);
                    }
                    return BuiltinResult::ok();
                }
                return BuiltinResult::err(1);
            }
            "-V" => {
                if i + 1 < args.len() {
                    let name = &args[i + 1];
                    let msg = match find_in_path(name) {
                        Some(p) => format!("{} is {}", name, p),
                        None => format!("{}: not found", name),
                    };
                    eprintln!("{}", msg);
                }
                return BuiltinResult::ok();
            }
            _ => break,
        }
    }
    if i >= args.len() {
        return BuiltinResult::ok();
    }
    if use_posix_path {
        let posix_path = "/system/local/bin:/system/bin:/bin";
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("PATH", posix_path);
        let result = exec_command(&args[i..]);
        if let Some(old) = old_path {
            std::env::set_var("PATH", &old);
        }
        result
    } else {
        exec_command(&args[i..])
    }
}

fn exec_command(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::ok();
    }
    let cmd = &args[0];
    let path = if cmd.contains('/') {
        cmd.clone()
    } else if let Some(p) = find_in_path(cmd) {
        p
    } else {
        eprintln!("context: command: {}: not found", cmd);
        return BuiltinResult::err(127);
    };
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("context: command: fork failed");
            BuiltinResult::err(1)
        }
        0 => {
            let c_args: Vec<std::ffi::CString> = args.iter()
                .filter_map(|w| std::ffi::CString::new(w.as_str()).ok())
                .collect();
            let mut c_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
            c_ptrs.push(std::ptr::null());
            let c_cmd = std::ffi::CString::new(path).unwrap_or_else(|_| std::ffi::CString::new("sh").unwrap());
            unsafe { libc::execvp(c_cmd.as_ptr(), c_ptrs.as_ptr()); }
            std::process::exit(126);
        }
        pid => {
            let mut status: i32 = 0;
            unsafe { libc::waitpid(pid, &mut status, 0); }
            let exit = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) }
                       else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) }
                       else { 1 };
            BuiltinResult::err(exit)
        }
    }
}

fn cmd_select(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: select: usage: select NAME [in WORDS ...;] do COMMANDS; done");
        return BuiltinResult::err(2);
    }
    let var_name = &args[0];
    let items: Vec<String> = if args.len() > 1 && args[1] == "in" {
        args[2..].iter()
            .map(|s| s.trim_end_matches(';').to_string())
            .filter(|s| s != "do" && s != "done" && !s.is_empty())
            .collect()
    } else {
        eprintln!("context: select: missing 'in'");
        return BuiltinResult::err(2);
    };
    if items.is_empty() {
        return BuiltinResult::err(2);
    }
    let stdin = std::io::stdin();
    loop {
        for (i, item) in items.iter().enumerate() {
            println!("  {}) {}", i + 1, item);
        }
        eprint!("#? ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() { continue; }
                if line == "EOF" || line == "quit" || line == "exit" { break; }
                if let Ok(n) = line.parse::<usize>() {
                    if n > 0 && n <= items.len() {
                        env.set(var_name, &items[n - 1]);
                        break;
                    }
                }
                eprintln!("context: select: invalid selection");
            }
            Err(_) => break,
        }
    }
    BuiltinResult::ok()
}

fn cmd_getopts(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.len() < 2 {
        eprintln!("context: getopts: usage: getopts OPTSTRING NAME [ARG...]");
        return BuiltinResult::err(2);
    }
    let optstring = &args[0];
    let name = &args[1];
    let shell_args: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        env.get("1").map(|s| vec![s.to_string()]).unwrap_or_default()
    };
    let optind: usize = env.get("OPTIND")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let offset: usize = env.get("_GETOPT_OFFSET")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if optind > shell_args.len() {
        env.set("OPTARG", "");
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    let current = &shell_args[optind - 1];
    if current == "--" {
        env.set("OPTARG", "");
        env.set("OPTIND", &(optind + 1).to_string());
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    if !current.starts_with('-') || current.len() < 2 {
        env.set("OPTARG", "");
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    let start = 1 + offset;
    if start >= current.len() {
        env.set("OPTARG", "");
        env.set("OPTIND", &(optind + 1).to_string());
        env.set("_GETOPT_OFFSET", "0");
        return BuiltinResult::err(1);
    }
    let opt_char = current.as_bytes()[start] as char;
    let remaining = start + 1 < current.len();
    if optstring.contains(opt_char) {
        env.set(name, &opt_char.to_string());
        if optstring.contains(format!("{}:", opt_char).as_str()) {
            if remaining {
                env.set("OPTARG", &current[(start + 1)..]);
                env.set("OPTIND", &(optind + 1).to_string());
                env.set("_GETOPT_OFFSET", "0");
            } else if optind < shell_args.len() {
                env.set("OPTARG", &shell_args[optind]);
                env.set("OPTIND", &(optind + 2).to_string());
                env.set("_GETOPT_OFFSET", "0");
            } else {
                eprintln!("context: getopts: {} requires an argument", opt_char);
                env.set("OPTARG", "");
                env.set("OPTIND", &(optind + 1).to_string());
                env.set("_GETOPT_OFFSET", "0");
                return BuiltinResult::err(2);
            }
        } else {
            env.set("OPTARG", "");
            if remaining {
                env.set("_GETOPT_OFFSET", &(offset + 1).to_string());
            } else {
                env.set("OPTIND", &(optind + 1).to_string());
                env.set("_GETOPT_OFFSET", "0");
            }
        }
        BuiltinResult::ok()
    } else {
        eprintln!("context: getopts: {}: invalid option", opt_char);
        env.set("OPTARG", &opt_char.to_string());
        if remaining {
            env.set("_GETOPT_OFFSET", &(offset + 1).to_string());
        } else {
            env.set("OPTIND", &(optind + 1).to_string());
            env.set("_GETOPT_OFFSET", "0");
        }
        BuiltinResult::err(2)
    }
}

fn cmd_realpath(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        eprintln!("context: realpath: filename argument required");
        return BuiltinResult::err(1);
    }
    for arg in args {
        let path = Path::new(arg);
        match std::fs::canonicalize(path) {
            Ok(canonical) => println!("{}", canonical.display()),
            Err(e) => {
                eprintln!("context: realpath: {}: {}", arg, e);
                return BuiltinResult::err(1);
            }
        }
    }
    BuiltinResult::ok()
}

fn cmd_local(args: &[String], env: &mut Env) -> BuiltinResult {
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            env.set_local(name, value);
        } else {
            env.set_local(arg, "");
        }
    }
    BuiltinResult::ok()
}

fn cmd_read(args: &[String], env: &mut Env) -> BuiltinResult {
    let mut raw = false;
    let mut silent = false;
    let mut prompt = String::new();
    let mut array_name = String::new();
    let mut delim = '\n';
    let mut timeout: Option<u64> = None;
    let mut var_names = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-r" => { raw = true; i += 1; }
            "-s" => { silent = true; i += 1; }
            "-a" => {
                i += 1;
                if i < args.len() {
                    array_name = args[i].clone();
                    i += 1;
                }
            }
            "-p" => {
                i += 1;
                if i < args.len() {
                    prompt = args[i].clone();
                    i += 1;
                }
            }
            "-d" => {
                i += 1;
                if i < args.len() {
                    delim = args[i].chars().next().unwrap_or('\n');
                    i += 1;
                }
            }
            "-t" => {
                i += 1;
                if i < args.len() {
                    timeout = args[i].parse::<u64>().ok();
                    i += 1;
                }
            }
            "-" => {
                break;
            }
            _ => {
                var_names.push(args[i].clone());
                i += 1;
            }
        }
    }

    if !prompt.is_empty() {
        eprint!("{}", prompt);
        let _ = std::io::stderr().flush();
    }

    let stdin = std::io::stdin();
    let mut line = String::new();
    let start = std::time::Instant::now();
    loop {
        if let Some(t) = timeout {
            if start.elapsed().as_millis() as u64 >= t * 1000 {
                break;
            }
        }
        let mut buf = [0u8; 1];
        match std::io::Read::read(&mut stdin.lock(), &mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let ch = buf[0] as char;
                if ch == delim {
                    break;
                }
                if ch == '\x03' {
                    eprintln!("^C");
                    return BuiltinResult::err(130);
                }
                if ch == '\x04' && line.is_empty() {
                    return BuiltinResult::err(1);
                }
                if ch == '\x7f' || ch == '\x08' {
                    line.pop();
                    if !silent {
                        eprint!("\x1b[2D \x1b[2D");
                        let _ = std::io::stderr().flush();
                    }
                    continue;
                }
                if !raw && ch == '\\' {
                    let mut next_buf = [0u8; 1];
                    if std::io::Read::read(&mut stdin.lock(), &mut next_buf).is_ok() {
                        let next = next_buf[0] as char;
                        match next {
                            '\n' => {
                                // backslash-newline continuation: skip both
                                continue;
                            }
                            'n' => { line.push('\n'); }
                            't' => { line.push('\t'); }
                            '\\' => { line.push('\\'); }
                            _ => { line.push('\\'); line.push(next); }
                        }
                    }
                    continue;
                }
                line.push(ch);
                if !silent {
                    eprint!("{}", ch);
                    let _ = std::io::stderr().flush();
                }
            }
            Err(_) => break,
        }
    }

    if !silent {
        eprintln!();
        let _ = std::io::stderr().flush();
    }

    let line = if line.ends_with('\n') {
        line[..line.len() - 1].to_string()
    } else {
        line
    };

    if !array_name.is_empty() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            env.set_local(&format!("{}_{}", array_name, i), word);
        }
        env.set_local(&format!("{}[@]", array_name), &words.join(" "));
        env.set_local(&format!("{}[#]", array_name), &words.len().to_string());
    } else if var_names.is_empty() {
        env.set("REPLY", &line);
    } else if var_names.len() == 1 {
        env.set_local(&var_names[0], &line);
    } else {
        let words: Vec<&str> = line.split_whitespace().collect();
        for (i, name) in var_names.iter().enumerate() {
            let val = if i < words.len() { words[i] } else { "" };
            env.set_local(name, val);
        }
    }

    BuiltinResult::ok()
}

fn cmd_shopt(args: &[String], env: &mut Env) -> BuiltinResult {
    let known_opts = [
        "cdable_vars", "cdspell", "checkhash", "checkwinsize", "cmdhist",
        "compat31", "compat32", "compat40", "compat41", "compat42", "compat43", "compat44",
        "complete_fullquote", "direxpand", "dirspell", "dotglob", "execfail",
        "expand_aliases", "extdebug", "extglob", "extquote", "failglob",
        "force_fignore", "globasciiranges", "globstar", "globskipdots",
        "histappend", "histreedit", "histverify", "hostcomplete", "huponexit",
        "inherit_errexit", "interactive_comments", "lastpipe", "localvar_inherit",
        "localvar_unset", "login_shell", "mailwarn", "no_empty_cmd_comp",
        "nocaseglob", "nocasematch", "nullglob", "patsub_replacement",
        "progcomp", "progvars", "promptvars", "restricted", "shift_verbose",
        "sourcepath", "xpg_echo",
    ];

    if args.is_empty() {
        for name in &known_opts {
            let val = env.get(&format!("_SHOPT_{}", name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(*name == "expand_aliases");
            let state = if val { "on" } else { "off" };
            println!("{}{}\t{}", name, " ".repeat(25 - name.len()), state);
        }
        return BuiltinResult::ok();
    }

    let mut i = 0;
    let mut query = false;
    let mut enable = true;
    if args[0] == "-s" {
        i = 1;
    } else if args[0] == "-u" || args[0] == "-q" {
        i = 1;
        query = true;
    } else if args[0].starts_with('+') {
        enable = false;
        i = 1;
    } else if args[0].starts_with('-') {
        eprintln!("context: shopt: {}: invalid option", args[0]);
        return BuiltinResult::err(2);
    }

    if i >= args.len() {
        for name in &known_opts {
            let val = env.get(&format!("_SHOPT_{}", name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(*name == "expand_aliases");
            let state = if val { "on" } else { "off" };
            println!("{}{}\t{}", name, " ".repeat(25 - name.len()), state);
        }
        return BuiltinResult::ok();
    }

    while i < args.len() {
        let opt_name = &args[i];
        if opt_name.starts_with('-') || opt_name.starts_with('+') {
            i += 1;
            continue;
        }
        if query {
            let val = env.get(&format!("_SHOPT_{}", opt_name.to_uppercase()))
                .map(|s| s == "1")
                .unwrap_or(*opt_name == "expand_aliases");
            let matches = if enable { val } else { !val };
            if !matches {
                return BuiltinResult::err(1);
            }
        } else {
            let var_name = format!("_SHOPT_{}", opt_name.to_uppercase());
            if enable {
                env.set(&var_name, "1");
            } else {
                env.set(&var_name, "0");
            }
        }
        i += 1;
    }
    BuiltinResult::ok()
}

fn cmd_bindkey(args: &[String], env: &mut Env) -> BuiltinResult {
    if args.is_empty() {
        let bindings_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(".config"))
            .join("context/keybindings");
        if let Ok(entries) = std::fs::read_dir(&bindings_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Some(widget) = std::fs::read_to_string(entry.path()).ok().map(|s| s.trim().to_string()) {
                        println!("{} -> {}", name, widget);
                    }
                }
            }
        } else {
            println!("no keybindings configured (use: bindkey <key> <widget>)");
        }
        return BuiltinResult::ok();
    }
    if args.len() < 2 {
        eprintln!("context: bindkey: usage: bindkey <key-sequence> <widget-name>");
        return BuiltinResult::err(1);
    }
    let key = &args[0];
    let widget = &args[1];
    let valid_widgets = ["accept-line", "backward-char", "forward-char",
        "backward-delete-char", "delete-char", "backward-word", "forward-word",
        "beginning-of-line", "end-of-line", "kill-line", "backward-kill-line",
        "kill-word", "backward-kill-word", "yank", "accept-suggestion",
        "accept-suggestion-word", "history-search-backward", "history-search-forward",
        "clear-screen", "undo", "redo", "transpose-chars"];
    if !valid_widgets.contains(&widget.as_str()) {
        eprintln!("context: bindkey: unknown widget '{}'. Valid widgets:", widget);
        for w in &valid_widgets {
            eprintln!("  {}", w);
        }
        return BuiltinResult::err(1);
    }
    let bindings_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".config"))
        .join("context/keybindings");
    let _ = std::fs::create_dir_all(&bindings_dir);
    let safe_name = key.replace(['/', ' '], "_");
    let path = bindings_dir.join(&safe_name);
    match std::fs::write(&path, widget) {
        Ok(()) => {
            env.set("_BINDKEY_LAST", key);
            BuiltinResult::ok()
        }
        Err(e) => {
            eprintln!("context: bindkey: failed to save binding: {}", e);
            BuiltinResult::err(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env() -> Env {
        Env::new()
    }

    fn run_builtin(cmd: &str, args: &[String], env: &mut Env) -> BuiltinResult {
        let cfg = Config::default();
        let mut full_args = vec![cmd.to_string()];
        full_args.extend_from_slice(args);
        run(&full_args, env, &cfg, 0)
    }

    #[test]
    fn test_getopts_advances_past_double_dash() {
        let mut env = make_env();
        env.set("OPTIND", "1");
        let args: Vec<String> = vec![
            "ab:".into(),
            "OPT".into(),
            "-a".into(),
            "--".into(),
            "foo".into(),
        ];
        let result = run_builtin("getopts", &args, &mut env);
        assert_eq!(result.status, 0);
        assert_eq!(env.get("OPT").unwrap(), "a");
        assert_eq!(env.get("OPTIND").unwrap(), "2");

        let result2 = run_builtin("getopts", &args, &mut env);
        assert_eq!(result2.status, 1);
        assert_eq!(env.get("OPTIND").unwrap(), "3");
    }

    #[test]
    fn test_realpath_current_dir() {
        let mut env = make_env();
        let result = run_builtin("realpath", &[".".into()], &mut env);
        assert_eq!(result.status, 0);
    }

    #[test]
    fn test_realpath_nonexistent() {
        let mut env = make_env();
        let result = run_builtin("realpath", &["/nonexistent/path/xyz".into()], &mut env);
        assert_eq!(result.status, 1);
    }

    #[test]
    fn test_realpath_no_args() {
        let mut env = make_env();
        let result = run_builtin("realpath", &[], &mut env);
        assert_eq!(result.status, 1);
    }
}
