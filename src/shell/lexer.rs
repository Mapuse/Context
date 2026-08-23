use std::fmt;
use std::sync::atomic::Ordering;

use crate::shell::expand::CURRENT_LINE;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    /// All-digits word immediately adjacent to a redirect operator
    /// (`2>`); only this form may act as an fd prefix.
    IoNumber(String),
    SingleQuoted(String),
    DoubleQuoted(String),
    Backtick(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBraceBrace,
    RBraceBrace,
    Pipe,
    DoublePipe,
    Amp,
    DoubleAmp,
    Semi,
    DoubleSemi,
    SemiAmp,
    SemiSemiAmp,
    DoubleGreater,
    Greater,
    Less,
    LessLess,
    LessLessLess,
    LessLessDash,
    LessGreater,
    LessAmp,
    LessLParen,
    GreaterLParen,
    AmpGreater,
    AmpGreaterGreater,
    GreaterPipe,
    GreaterAmp,
    /// Verbatim heredoc body captured by the lexer, plus its expand flag.
    /// Emitted directly after the corresponding `<<`/`<<-` operator token.
    HereDocBody(String, bool),
    DoubleLBracket,
    DoubleRBracket,
    Newline,
    Eof,

    If,
    Then,
    Elif,
    Else,
    Fi,
    For,
    In,
    Do,
    Done,
    While,
    Until,
    Case,
    Esac,
    Function,
    Select,
    Coproc,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Word(s) => write!(f, "{}", s),
            Token::IoNumber(s) => write!(f, "{}", s),
            Token::SingleQuoted(s) => write!(f, "'{}'", s),
            Token::DoubleQuoted(s) => write!(f, "\"{}\"", s),
            Token::Backtick(s) => write!(f, "`{}`", s),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBraceBrace => write!(f, "(("),
            Token::RBraceBrace => write!(f, "))"),
            Token::Pipe => write!(f, "|"),
            Token::DoublePipe => write!(f, "||"),
            Token::Amp => write!(f, "&"),
            Token::DoubleAmp => write!(f, "&&"),
            Token::Semi => write!(f, ";"),
            Token::DoubleSemi => write!(f, ";;"),
            Token::SemiAmp => write!(f, ";&"),
            Token::SemiSemiAmp => write!(f, ";;&"),
            Token::DoubleGreater => write!(f, ">>"),
            Token::Greater => write!(f, ">"),
            Token::Less => write!(f, "<"),
            Token::LessLess => write!(f, "<<"),
            Token::LessLessLess => write!(f, "<<<"),
            Token::LessLessDash => write!(f, "<<-"),
            Token::LessGreater => write!(f, "<>"),
            Token::LessAmp => write!(f, "<&"),
            Token::LessLParen => write!(f, "<("),
            Token::GreaterLParen => write!(f, ">("),
            Token::AmpGreater => write!(f, "&>"),
            Token::AmpGreaterGreater => write!(f, "&>>"),
            Token::GreaterPipe => write!(f, ">|"),
            Token::GreaterAmp => write!(f, ">&"),
            Token::HereDocBody(body, _) => write!(f, "{}", body),
            Token::DoubleLBracket => write!(f, "[["),
            Token::DoubleRBracket => write!(f, "]]"),
            Token::Newline => writeln!(f),
            Token::Eof => write!(f, ""),
            Token::If => write!(f, "if"),
            Token::Then => write!(f, "then"),
            Token::Elif => write!(f, "elif"),
            Token::Else => write!(f, "else"),
            Token::Fi => write!(f, "fi"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Do => write!(f, "do"),
            Token::Done => write!(f, "done"),
            Token::While => write!(f, "while"),
            Token::Until => write!(f, "until"),
            Token::Case => write!(f, "case"),
            Token::Esac => write!(f, "esac"),
            Token::Function => write!(f, "function"),
            Token::Select => write!(f, "select"),
            Token::Coproc => write!(f, "coproc"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    /// Heredoc operators (`<<`, `<<-`) seen on the current line whose
    /// delimiter word has not been read yet (strip_tabs flag per op).
    pending_heredoc_ops: Vec<bool>,
    /// Heredocs whose delimiter was read; bodies are captured when the
    /// line's newline is reached: (delimiter, expand, strip_tabs).
    active_heredocs: Vec<(String, bool, bool)>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            pending_heredoc_ops: Vec::new(),
            active_heredocs: Vec::new(),
        }
    }

    fn keyword_or_word(word: &str) -> Token {
        match word {
            "if" => Token::If,
            "then" => Token::Then,
            "elif" => Token::Elif,
            "else" => Token::Else,
            "fi" => Token::Fi,
            "for" => Token::For,
            "in" => Token::In,
            "do" => Token::Do,
            "done" => Token::Done,
            "while" => Token::While,
            "until" => Token::Until,
            "case" => Token::Case,
            "esac" => Token::Esac,
            "function" => Token::Function,
            "select" => Token::Select,
            "coproc" => Token::Coproc,
            _ => Token::Word(word.to_string()),
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        self.pos += 1;
        if ch == Some('\n') {
            CURRENT_LINE.fetch_add(1, Ordering::Relaxed);
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_word(&mut self) -> String {
        let mut word = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                ' ' | '\t' | '\r' | '\n' | '|' | '&' | ';' | '(' | ')'
                | '<' | '>' => break,
                '#' if word.is_empty() => {
                    self.skip_comment();
                    break;
                }
                '$' => word.push_str(&self.read_dollar_word()),
                '"' => {
                    self.advance();
                    let s = self.read_double_quoted();
                    word.push('\x01');
                    word.push_str(&s);
                    word.push('\x01');
                }
                '\'' => {
                    self.advance();
                    let s = self.read_single_quoted();
                    word.push('\x02');
                    word.push_str(&s);
                    word.push('\x02');
                }
                '`' => {
                    self.advance();
                    let s = self.read_backtick();
                    word.push('`');
                    word.push_str(&s);
                    word.push('`');
                }
                '\\' => {
                    self.advance();
                    if let Some(next) = self.peek() {
                        if next == '\n' {
                            self.advance();
                            continue;
                        }
                        word.push(self.advance().unwrap());
                    }
                }
                _ => {
                    self.advance();
                    word.push(ch);
                }
            }
        }
        word
    }

    /// Read a `$`-expansion inline: `$var`, `${...}`, `$(...)`, `$((...))`,
    /// `$'...'` and the special variables. The leading `$` is consumed.
    fn read_dollar_word(&mut self) -> String {
        self.advance();
        let Some(ch) = self.peek() else { return "$".to_string() };
        match ch {
            '\'' => {
                self.advance();
                let s = self.read_ansi_c_quoted();
                format!("$'{}'", s)
            }
            '(' => {
                self.advance();
                let mut sub = String::from("$(");
                let mut depth = 1u32;
                let mut in_single = false;
                let mut in_double = false;
                while let Some(c) = self.peek() {
                    if c == '\'' && !in_double {
                        in_single = !in_single;
                        sub.push(c);
                        self.advance();
                    } else if c == '"' && !in_single {
                        in_double = !in_double;
                        sub.push(c);
                        self.advance();
                    } else if in_single || in_double {
                        sub.push(c);
                        self.advance();
                    } else {
                        match c {
                            '(' => { depth += 1; sub.push(c); self.advance(); }
                            ')' => {
                                depth -= 1;
                                sub.push(c);
                                self.advance();
                                if depth == 0 { break; }
                            }
                            _ => { sub.push(c); self.advance(); }
                        }
                    }
                }
                sub
            }
            '{' => {
                self.advance();
                let mut var = String::from("${");
                let mut depth = 1u32;
                let mut in_single = false;
                let mut in_double = false;
                while let Some(c) = self.peek() {
                    if c == '\'' && !in_double {
                        in_single = !in_single;
                        var.push(c);
                        self.advance();
                    } else if c == '"' && !in_single {
                        in_double = !in_double;
                        var.push(c);
                        self.advance();
                    } else if in_single || in_double {
                        var.push(c);
                        self.advance();
                    } else {
                        match c {
                            '{' => { depth += 1; var.push(c); }
                            '}' => {
                                var.push(c);
                                depth -= 1;
                                if depth == 0 {
                                    self.advance();
                                    break;
                                }
                            }
                            _ => { var.push(c); }
                        }
                        self.advance();
                    }
                }
                var
            }
            '?' | '$' | '!' | '@' | '*' | '#' | '-' | '_' => {
                self.advance();
                format!("${}", ch)
            }
            '0'..='9' => {
                self.advance();
                format!("${}", ch)
            }
            _ => {
                let mut var = String::from("$");
                while let Some(ch) = self.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        var.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
                var
            }
        }
    }

    fn read_single_quoted(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.advance() {
            if ch == '\'' {
                return s;
            }
            s.push(ch);
        }
        s
    }

    fn read_double_quoted(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.advance() {
            match ch {
                '"' => return s,
                '\\' => {
                    if let Some(next) = self.advance()
                        && next != '\n' {
                            match next {
                                '$' | '`' | '\\' | '"' => s.push(next),
                                _ => {
                                    s.push('\\');
                                    s.push(next);
                                }
                            }
                        }
                }
                _ => s.push(ch),
            }
        }
        s
    }

    fn read_backtick(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.advance() {
            if ch == '`' {
                return s;
            }
            if ch == '\\' {
                if let Some(next) = self.advance() {
                    if next == '`' {
                        s.push('`');
                    } else {
                        s.push('\\');
                        s.push(next);
                    }
                }
            } else {
                s.push(ch);
            }
        }
        s
    }

    fn read_ansi_c_quoted(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.advance() {
            if ch == '\'' {
                return s;
            }
            if ch == '\\' {
                if let Some(next) = self.advance() {
                    match next {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '\'' => s.push('\''),
                        '"' => s.push('"'),
                        'a' => s.push('\x07'),
                        'b' => s.push('\x08'),
                        'e' => s.push('\x1b'),
                        'f' => s.push('\x0c'),
                        'v' => s.push('\x0b'),
                        '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' => {
                            let mut oct = String::new();
                            oct.push(next);
                            while let Some(d) = self.peek() {
                                if d.is_ascii_digit() && d <= '7' && oct.len() < 3 {
                                    oct.push(d);
                                    self.advance();
                                } else { break; }
                            }
                            if let Ok(byte) = u8::from_str_radix(&oct, 8) {
                                s.push(byte as char);
                            }
                        }
                        'x' => {
                            let mut hex = String::new();
                            let mut count = 0;
                            while let Some(d) = self.peek() {
                                if d.is_ascii_hexdigit() && count < 2 {
                                    hex.push(d);
                                    self.advance();
                                    count += 1;
                                } else { break; }
                            }
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                s.push(byte as char);
                            }
                        }
                        'u' => {
                            let mut hex = String::new();
                            let mut count = 0;
                            while let Some(d) = self.peek() {
                                if d.is_ascii_hexdigit() && count < 4 {
                                    hex.push(d);
                                    self.advance();
                                    count += 1;
                                } else { break; }
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16)
                                && let Some(ch) = char::from_u32(code) {
                                    s.push(ch);
                                }
                        }
                        'U' => {
                            let mut hex = String::new();
                            let mut count = 0;
                            while let Some(d) = self.peek() {
                                if d.is_ascii_hexdigit() && count < 8 {
                                    hex.push(d);
                                    self.advance();
                                    count += 1;
                                } else { break; }
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16)
                                && let Some(ch) = char::from_u32(code) {
                                    s.push(ch);
                                }
                        }
                        'c' => {
                            if let Some(ch) = self.advance() {
                                if ch == '\\' {
                                    if let Some(next_ch) = self.advance() {
                                        let ctrl = match next_ch {
                                            'a' => 0x01, 'b' => 0x02, 'c' => 0x03,
                                            'd' => 0x04, 'e' => 0x05, 'f' => 0x06,
                                            'g' => 0x07, 'h' => 0x08, 'i' => 0x09,
                                            'j' => 0x0a, 'k' => 0x0b, 'l' => 0x0c,
                                            'm' => 0x0d, 'n' => 0x0e, 'o' => 0x0f,
                                            'p' => 0x10, 'q' => 0x11, 'r' => 0x12,
                                            's' => 0x13, 't' => 0x14, 'u' => 0x15,
                                            'v' => 0x16, 'w' => 0x17, 'x' => 0x18,
                                            'y' => 0x19, 'z' => 0x1a,
                                            '[' => 0x1b, '\\' => 0x1c, ']' => 0x1d,
                                            '^' => 0x1e, '_' => 0x1f, '?' => 0x7f,
                                            _ => (next_ch as u32 & 0x1f) as u8,
                                        };
                                        s.push(ctrl as char);
                                    }
                                } else if ch == '?' {
                                    s.push(0x7f as char);
                                } else {
                                    s.push((ch as u32 & 0x1f) as u8 as char);
                                }
                            }
                        }
                        _ => s.push(next),
                    }
                }
            } else {
                s.push(ch);
            }
        }
        s
    }

    fn skip_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let Some(ch) = self.peek() else {
                tokens.push(Token::Eof);
                break;
            };
            match ch {
                '\n' => {
                    self.advance();
                    if !self.active_heredocs.is_empty() {
                        let heredocs = std::mem::take(&mut self.active_heredocs);
                        tokens.extend(self.capture_heredoc_bodies(&heredocs));
                    }
                    tokens.push(Token::Newline);
                }
                '#' => {
                    self.skip_comment();
                }
                '\'' => {
                    self.advance();
                    let s = self.read_single_quoted();
                    if self.pending_heredoc_ops.is_empty() {
                        tokens.push(Token::SingleQuoted(s));
                    } else {
                        // Quoted heredoc delimiter: body is not expanded.
                        let strip_tabs = self.pending_heredoc_ops.remove(0);
                        self.active_heredocs.push((s, false, strip_tabs));
                    }
                }
                '"' => {
                    self.advance();
                    let s = self.read_double_quoted();
                    if self.pending_heredoc_ops.is_empty() {
                        tokens.push(Token::DoubleQuoted(s));
                    } else {
                        let strip_tabs = self.pending_heredoc_ops.remove(0);
                        self.active_heredocs.push((s, false, strip_tabs));
                    }
                }
                '`' => {
                    self.advance();
                    tokens.push(Token::Backtick(self.read_backtick()));
                }
                '(' => {
                    self.advance();
                    if self.peek() == Some('(') {
                        self.advance();
                        tokens.push(Token::LBraceBrace);
                    } else {
                        tokens.push(Token::LParen);
                    }
                }
                ')' => {
                    self.advance();
                    if self.peek() == Some(')') {
                        self.advance();
                        tokens.push(Token::RBraceBrace);
                    } else {
                        tokens.push(Token::RParen);
                    }
                }
                '{' | '}' => {
                    // A brace is a group delimiter only when followed by a
                    // word terminator; otherwise it belongs to the word so
                    // brace expansion can see it (`echo {a,b,c}`).
                    let next = self.input.get(self.pos + 1).copied();
                    let is_delimited = matches!(next,
                        Some(' ') | Some('\t') | Some('\r') | Some('\n') | Some(';') | None);
                    if is_delimited {
                        tokens.push(if ch == '{' { Token::LBrace } else { Token::RBrace });
                        self.advance();
                    } else {
                        tokens.push(Token::Word(self.read_word()));
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        tokens.push(Token::DoublePipe);
                    } else {
                        tokens.push(Token::Pipe);
                    }
                }
                '&' => {
                    self.advance();
                    match self.peek() {
                        Some('&') => {
                            self.advance();
                            tokens.push(Token::DoubleAmp);
                        }
                        Some('>') => {
                            self.advance();
                            if self.peek() == Some('>') {
                                self.advance();
                                tokens.push(Token::AmpGreaterGreater);
                            } else {
                                tokens.push(Token::AmpGreater);
                            }
                        }
                        _ => tokens.push(Token::Amp),
                    }
                }
                ';' => {
                    self.advance();
                    if self.peek() == Some(';') {
                        self.advance();
                        if self.peek() == Some('&') {
                            self.advance();
                            tokens.push(Token::SemiSemiAmp);
                        } else {
                            tokens.push(Token::DoubleSemi);
                        }
                    } else if self.peek() == Some('&') {
                        self.advance();
                        tokens.push(Token::SemiAmp);
                    } else {
                        tokens.push(Token::Semi);
                    }
                }
                '>' => {
                    self.advance();
                    match self.peek() {
                        Some('>') => {
                            self.advance();
                            if self.peek() == Some('|') {
                                self.advance();
                                tokens.push(Token::DoubleGreater);
                                tokens.push(Token::Pipe);
                            } else {
                                tokens.push(Token::DoubleGreater);
                            }
                        }
                        Some('|') => {
                            self.advance();
                            tokens.push(Token::GreaterPipe);
                        }
                        Some('&') => {
                            self.advance();
                            tokens.push(Token::GreaterAmp);
                        }
                        Some('(') => {
                            self.advance();
                            tokens.push(Token::GreaterLParen);
                        }
                        _ => tokens.push(Token::Greater),
                    }
                }
                '<' => {
                    self.advance();
                    match self.peek() {
                        Some('<') => {
                            self.advance();
                            if self.peek() == Some('-') {
                                self.advance();
                                tokens.push(Token::LessLessDash);
                                self.pending_heredoc_ops.push(true);
                            } else if self.peek() == Some('<') {
                                self.advance();
                                tokens.push(Token::LessLessLess);
                            } else {
                                tokens.push(Token::LessLess);
                                self.pending_heredoc_ops.push(false);
                            }
                        }
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::LessGreater);
                        }
                        Some('&') => {
                            self.advance();
                            tokens.push(Token::LessAmp);
                        }
                        Some('(') => {
                            self.advance();
                            tokens.push(Token::LessLParen);
                        }
                        _ => tokens.push(Token::Less),
                    }
                }
                '$' => {
                    let mut w = self.read_dollar_word();
                    if !matches!(self.peek(), None | Some(' ') | Some('\t') | Some('\r') | Some('\n')
                        | Some('|') | Some('&') | Some(';') | Some('(') | Some(')')
                        | Some('{') | Some('}') | Some('<') | Some('>')) {
                        w.push_str(&self.read_word());
                    }
                    tokens.push(Token::Word(w));
                }
                '[' => {
                    self.advance();
                    if self.peek() == Some('[') {
                        self.advance();
                        tokens.push(Token::DoubleLBracket);
                    } else {
                        // A lone `[` begins a word (`[a-z]*`, `[ -f x ]`);
                        // keep it glued to the rest of the word.
                        let mut w = String::from("[");
                        w.push_str(&self.read_word());
                        tokens.push(Token::Word(w));
                    }
                }
                ']' => {
                    self.advance();
                    if self.peek() == Some(']') {
                        self.advance();
                        tokens.push(Token::DoubleRBracket);
                    } else {
                        let mut w = String::from("]");
                        w.push_str(&self.read_word());
                        tokens.push(Token::Word(w));
                    }
                }
                '\\' => {
                    self.advance();
                    if let Some(next) = self.peek() {
                        if next == '\n' {
                            self.advance();
                            continue;
                        }
                        tokens.push(Token::Word(self.advance().unwrap().to_string()));
                    }
                }
                _ => {
                    let word = self.read_word();
                    if !self.pending_heredoc_ops.is_empty() {
                        // This word is the heredoc delimiter, not a token.
                        let strip_tabs = self.pending_heredoc_ops.remove(0);
                        self.active_heredocs.push((word, true, strip_tabs));
                        continue;
                    }
                    // An all-digits word adjacent to a redirect operator is
                    // an fd number (`2>/dev/null`); separated by a space it
                    // is just an argument (`seq 1 3 5 > f`).
                    let adjacent_redirect = matches!(self.peek(), Some('<') | Some('>'));
                    if !word.is_empty()
                        && word.bytes().all(|b| b.is_ascii_digit())
                        && adjacent_redirect {
                            tokens.push(Token::IoNumber(word));
                        } else {
                            tokens.push(Self::keyword_or_word(&word));
                        }
                }
            }
        }
        tokens
    }

    /// Read heredoc bodies verbatim from the input (positioned at the start
    /// of the first body line) until each delimiter line is consumed.
    fn capture_heredoc_bodies(&mut self, heredocs: &[(String, bool, bool)]) -> Vec<Token> {
        let mut out = Vec::new();
        for (delim, expand, strip_tabs) in heredocs {
            let mut body = String::new();
            loop {
                if self.pos >= self.input.len() {
                    break;
                }
                let mut line = String::new();
                let mut saw_newline = false;
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        saw_newline = true;
                        break;
                    }
                    line.push(c);
                }
                let candidate = if *strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if candidate == delim {
                    break;
                }
                body.push_str(candidate);
                if saw_newline {
                    body.push('\n');
                } else {
                    break;
                }
            }
            out.push(Token::HereDocBody(body, *expand));
        }
        out
    }

}

pub fn tokenize(input: &str) -> Vec<Token> {
    Lexer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let tokens = tokenize("ls -la");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Word("ls".into()));
        assert_eq!(tokens[1], Token::Word("-la".into()));
        assert_eq!(tokens[2], Token::Eof);
    }

    #[test]
    fn test_pipes() {
        let tokens = tokenize("cat foo | grep bar");
        assert!(tokens.contains(&Token::Pipe));
    }

    #[test]
    fn test_and_or() {
        let tokens = tokenize("true && false || echo err");
        assert!(tokens.contains(&Token::DoubleAmp));
        assert!(tokens.contains(&Token::DoublePipe));
    }

    #[test]
    fn test_quotes() {
        let tokens = tokenize(r#"echo "hello world" 'foo bar'"#);
        assert!(tokens.contains(&Token::DoubleQuoted("hello world".into())));
        assert!(tokens.contains(&Token::SingleQuoted("foo bar".into())));
    }

    #[test]
    fn test_redirects() {
        let tokens = tokenize("echo x > /dev/null");
        assert!(tokens.contains(&Token::Greater));
        let tokens2 = tokenize("echo x &>/dev/null");
        assert!(tokens2.contains(&Token::AmpGreater));
    }

    #[test]
    fn test_heredoc() {
        let tokens = tokenize("cat <<EOF");
        assert!(tokens.contains(&Token::LessLess));
        // The delimiter word is consumed as a heredoc delimiter, not a token.
        assert!(!tokens.contains(&Token::Word("EOF".into())));
    }

    #[test]
    fn test_subshell() {
        let tokens = tokenize("(echo hi)");
        assert!(tokens.contains(&Token::LParen));
        assert!(tokens.contains(&Token::RParen));
    }

    #[test]
    fn test_comment() {
        let tokens = tokenize("echo x # this is a comment");
        assert!(!tokens.contains(&Token::Word("#".into())));
    }

    #[test]
    fn test_glob() {
        let tokens = tokenize("ls *.rs");
        assert!(tokens.contains(&Token::Word("*.rs".into())));
    }

    #[test]
    fn test_empty() {
        let tokens = tokenize("");
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_here_string() {
        let tokens = tokenize("cat <<< hello");
        assert!(tokens.contains(&Token::LessLessLess));
    }

    #[test]
    fn test_less_amp() {
        let tokens = tokenize("cat <&0");
        assert!(tokens.contains(&Token::LessAmp));
    }

    #[test]
    fn test_hash_mid_word() {
        let tokens = tokenize("echo file#comment");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Word("echo".into()));
        assert_eq!(tokens[1], Token::Word("file#comment".into()));
    }

    #[test]
    fn test_hash_at_start_is_comment() {
        let tokens = tokenize("# this is a comment");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Eof);
    }

    #[test]
    fn test_hash_after_space_is_comment() {
        let tokens = tokenize("echo x # comment");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Word("echo".into()));
        assert_eq!(tokens[1], Token::Word("x".into()));
        assert_eq!(tokens[2], Token::Eof);
    }

    #[test]
    fn test_dollar_paren() {
        let tokens = tokenize("echo $(ls -la)");
        assert!(tokens.contains(&Token::Word("$(ls -la)".into())));
    }

    #[test]
    fn test_dollar_paren_nested() {
        let tokens = tokenize("echo $(echo $(whoami))");
        assert!(tokens.contains(&Token::Word("$(echo $(whoami))".into())));
    }

    #[test]
    fn test_dollar_brace_var() {
        let tokens = tokenize("echo ${HOME}");
        assert!(tokens.contains(&Token::Word("${HOME}".into())));
    }

    #[test]
    fn test_dollar_special_vars() {
        let tokens = tokenize("echo $?");
        assert!(tokens.contains(&Token::Word("$?".into())));
        let tokens = tokenize("echo $$");
        assert!(tokens.contains(&Token::Word("$$".into())));
    }

    #[test]
    fn test_escape_in_word() {
        let tokens = tokenize(r"echo hello\ world");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1], Token::Word("hello world".into()));
    }

    #[test]
    fn test_double_redirect() {
        let tokens = tokenize("echo x >> file");
        assert!(tokens.contains(&Token::DoubleGreater));
    }

    #[test]
    fn test_clobber_redirect() {
        let tokens = tokenize("echo x >| file");
        assert!(tokens.contains(&Token::GreaterPipe));
    }

    #[test]
    fn test_double_semi() {
        let tokens = tokenize("case x in a) ;; esac");
        assert!(tokens.contains(&Token::DoubleSemi));
    }

    #[test]
    fn test_amp_greater_greater() {
        let tokens2 = tokenize("echo x &>> file");
        assert!(tokens2.contains(&Token::AmpGreaterGreater));
    }

    #[test]
    fn test_backtick() {
        let tokens = tokenize("echo `whoami`");
        assert!(tokens.contains(&Token::Backtick("whoami".into())));
    }

    #[test]
    fn test_brace_expansion() {
        let tokens = tokenize("echo {a,b,c}");
        assert_eq!(tokens[1], Token::Word("{a,b,c}".into()));
        assert!(!tokens.iter().any(|t| matches!(t, Token::LBrace | Token::RBrace)));
        let tokens = tokenize("f() { echo hi; }");
        assert!(tokens.contains(&Token::LBrace));
        assert!(tokens.contains(&Token::RBrace));
    }

    #[test]
    fn test_here_string_quoted() {
        let tokens = tokenize("cat <<<\"hello world\"");
        assert!(tokens.contains(&Token::LessLessLess));
        assert!(tokens.contains(&Token::DoubleQuoted("hello world".into())));
    }

    #[test]
    fn test_dollar_paren_in_double_quotes() {
        let tokens = tokenize(r#"echo "$(cmd)""#);
        let found = tokens.iter().any(|t| matches!(t, Token::DoubleQuoted(w) if w.contains("$(cmd)")));
        assert!(found);
    }

    #[test]
    fn test_dollar_brace_question() {
        let tokens = tokenize("echo ${var:?message}");
        assert!(tokens.contains(&Token::Word("${var:?message}".into())));
    }

    #[test]
    fn test_redirect_stderr_to_devnull() {
        let tokens = tokenize("cmd 2>/dev/null");
        assert!(tokens.contains(&Token::IoNumber("2".into())));
        assert!(tokens.contains(&Token::Greater));
        assert!(tokens.contains(&Token::Word("/dev/null".into())));
    }

    #[test]
    fn test_amp_background() {
        let tokens = tokenize("sleep 10 &");
        assert!(tokens.contains(&Token::Amp));
        assert!(tokens.contains(&Token::Word("sleep".into())));
        assert!(tokens.contains(&Token::Word("10".into())));
    }

    #[test]
    fn test_coproc_keyword() {
        let tokens = tokenize("coproc cmd");
        assert!(tokens.contains(&Token::Coproc));
        assert!(tokens.contains(&Token::Word("cmd".into())));
    }

    #[test]
    fn test_double_less_less_dash() {
        let tokens = tokenize("cat <<-EOF");
        assert!(tokens.contains(&Token::LessLessDash));
        assert!(!tokens.contains(&Token::Word("EOF".into())));
    }

    #[test]
    fn test_process_substitution_in() {
        let tokens = tokenize("diff <(cmd1) <(cmd2)");
        assert!(tokens.contains(&Token::LessLParen));
    }

    #[test]
    fn test_process_substitution_out() {
        let tokens = tokenize("echo >(cmd)");
        assert!(tokens.contains(&Token::GreaterLParen));
    }

    #[test]
    fn test_redirect_fd_num() {
        let tokens = tokenize("echo x 3>&1");
        assert!(tokens.contains(&Token::IoNumber("3".into())));
        assert!(tokens.contains(&Token::GreaterAmp));
    }

    #[test]
    fn test_digit_args_not_fd() {
        let tokens = tokenize("seq 1 3 5 > /tmp/f");
        assert!(tokens.contains(&Token::Word("1".into())));
        assert!(tokens.contains(&Token::Word("3".into())));
        assert!(tokens.contains(&Token::Word("5".into())));
        assert!(!tokens.iter().any(|t| matches!(t, Token::IoNumber(_))));
    }

    #[test]
    fn test_case_keywords() {
        let tokens = tokenize("case x in a) ;; esac");
        assert!(tokens.contains(&Token::Case));
        assert!(tokens.contains(&Token::In));
        assert!(tokens.contains(&Token::Esac));
        assert!(tokens.contains(&Token::DoubleSemi));
    }

    #[test]
    fn test_select_keyword() {
        let tokens = tokenize("select i in 1 2 3; do echo $i; done");
        assert!(tokens.contains(&Token::Select));
        assert!(tokens.contains(&Token::Do));
        assert!(tokens.contains(&Token::Done));
    }

    #[test]
    fn test_dollar_zero() {
        let tokens = tokenize("echo $0");
        assert!(tokens.contains(&Token::Word("$0".into())));
    }

    #[test]
    fn test_dollar_hash() {
        let tokens = tokenize("echo $#");
        assert!(tokens.contains(&Token::Word("$#".into())));
    }

    #[test]
    fn test_dollar_bang() {
        let tokens = tokenize("echo $!");
        assert!(tokens.contains(&Token::Word("$!".into())));
    }

    #[test]
    fn test_dollar_underscore() {
        let tokens = tokenize("echo $_");
        assert!(tokens.contains(&Token::Word("$_".into())));
    }

    #[test]
    fn test_ansi_c_quoted() {
        let tokens = tokenize(r#"echo $'hello\nworld'"#);
        let found = tokens.iter().any(|t| matches!(t, Token::Word(w) if w.starts_with("$'")));
        assert!(found);
    }

    #[test]
    fn test_backtick_in_word() {
        let tokens = tokenize("echo `whoami`");
        assert!(tokens.contains(&Token::Backtick("whoami".into())));
    }

    #[test]
    fn test_less_greater_redirect() {
        let tokens = tokenize("cmd <> file");
        assert!(tokens.contains(&Token::LessGreater));
    }

    #[test]
    fn test_double_bracket() {
        let tokens = tokenize("[[ -f file ]]");
        assert!(tokens.contains(&Token::DoubleLBracket));
        assert!(tokens.contains(&Token::DoubleRBracket));
    }

    #[test]
    fn test_word_with_equals() {
        let tokens = tokenize("VAR=value");
        assert!(tokens.contains(&Token::Word("VAR=value".into())));
    }

    #[test]
    fn test_escape_newline_in_word() {
        let tokens = tokenize("echo hello\\\nworld");
        let word_tokens: Vec<&Token> = tokens.iter().filter(|t| matches!(t, Token::Word(w) if w == "helloworld")).collect();
        assert_eq!(word_tokens.len(), 1);
    }

    #[test]
    fn test_dollar_star() {
        let tokens = tokenize("echo $*");
        assert!(tokens.contains(&Token::Word("$*".into())));
    }

    #[test]
    fn test_dollar_at() {
        let tokens = tokenize("echo $@");
        assert!(tokens.contains(&Token::Word("$@".into())));
    }

    #[test]
    fn test_dollar_double_paren() {
        let tokens = tokenize("echo $((1+2))");
        assert!(tokens.contains(&Token::Word("$((1+2))".into())));
    }

    #[test]
    fn test_semi_amp_case() {
        let tokens = tokenize("case x in a) cmd ;;& esac");
        assert!(tokens.contains(&Token::SemiSemiAmp));
    }

    #[test]
    fn test_amp_semi_case() {
        let tokens = tokenize("case x in a) cmd ;& esac");
        assert!(tokens.contains(&Token::SemiAmp));
    }

    #[test]
    fn test_less_less_less_here_string() {
        let tokens = tokenize("cat <<<hello");
        assert!(tokens.contains(&Token::LessLessLess));
    }

    #[test]
    fn test_redirect_input_fd() {
        let tokens = tokenize("cat <&0");
        assert!(tokens.contains(&Token::LessAmp));
    }

    #[test]
    fn test_redirect_open() {
        let tokens = tokenize("cmd <> file");
        assert!(tokens.contains(&Token::LessGreater));
    }
}



