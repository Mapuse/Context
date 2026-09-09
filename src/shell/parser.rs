use crate::shell::ast::*;
use crate::shell::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect_word(&mut self) -> Option<String> {
        match self.peek() {
            Token::Word(_)
            | Token::SingleQuoted(_)
            | Token::DoubleQuoted(_)
            | Token::Backtick(_) => match self.advance() {
                Token::Word(s)
                | Token::SingleQuoted(s)
                | Token::DoubleQuoted(s)
                | Token::Backtick(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    pub fn parse(&mut self) -> Node {
        self.skip_newlines();
        let node = self.parse_list();
        self.skip_newlines();
        node
    }

    fn parse_list(&mut self) -> Node {
        let mut left = self.parse_and_or();
        loop {
            match self.peek() {
                Token::Semi | Token::Newline => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_and_or();
                    if right.is_empty() {
                        return left;
                    }
                    left = Node::Compound {
                        kind: CompoundKind::Semicolon,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::Amp => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_and_or();
                    left = Node::Compound {
                        kind: CompoundKind::Background,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::Eof => break,
                _ => break,
            }
        }
        left
    }

    fn parse_and_or(&mut self) -> Node {
        let mut left = self.parse_pipeline();
        loop {
            match self.peek() {
                Token::DoubleAmp => {
                    self.advance();
                    let right = self.parse_pipeline();
                    left = Node::Compound {
                        kind: CompoundKind::And,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::DoublePipe => {
                    self.advance();
                    let right = self.parse_pipeline();
                    left = Node::Compound {
                        kind: CompoundKind::Or,
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        left
    }

    fn parse_pipeline(&mut self) -> Node {
        let mut bang = false;
        if let Token::Word(w) = self.peek()
            && w == "!"
        {
            self.advance();
            bang = true;
        }
        let first = self.parse_command();
        let mut commands = vec![first];
        while matches!(self.peek(), Token::Pipe) {
            self.advance();
            self.skip_newlines();
            commands.push(self.parse_command());
        }
        if commands.len() == 1 && !bang {
            return commands.remove(0);
        }
        Node::Pipeline { commands, bang }
    }

    fn parse_command(&mut self) -> Node {
        self.skip_newlines();
        match self.peek() {
            Token::LParen => self.parse_subshell(),
            Token::LBrace => self.parse_brace_group(),
            Token::LBraceBrace => self.parse_arith(),
            Token::DoubleLBracket => self.parse_test_double_bracket(),
            Token::If => self.parse_if(),
            Token::While => self.parse_while(),
            Token::Until => self.parse_until(),
            Token::For => self.parse_for(),
            Token::Case => self.parse_case(),
            Token::Function => self.parse_function_def(),
            Token::Select => self.parse_select(),
            Token::Coproc => self.parse_coproc(),
            Token::Word(_) => {
                if self.lookahead_is_func_def() {
                    self.parse_function_def_posix()
                } else {
                    self.parse_simple_command()
                }
            }
            _ => self.parse_simple_command(),
        }
    }

    fn lookahead_is_func_def(&mut self) -> bool {
        if let Token::Word(_) = self.peek() {
            let saved_pos = self.pos;
            self.pos += 1;
            let is_paren = matches!(self.peek(), Token::LParen);
            if is_paren {
                self.pos += 1;
                let is_close = matches!(self.peek(), Token::RParen);
                self.pos = saved_pos;
                return is_close;
            }
            self.pos = saved_pos;
        }
        false
    }

    fn parse_function_def_posix(&mut self) -> Node {
        let name = self.expect_word().unwrap_or_default();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
        }
        if matches!(self.peek(), Token::RParen) {
            self.advance();
        }
        self.skip_newlines();
        let body = Box::new(self.parse_brace_group());
        Node::Function { name, body }
    }

    fn parse_select(&mut self) -> Node {
        self.advance();
        let var = self.expect_word().unwrap_or_default();
        let values = if matches!(self.peek(), Token::In) {
            self.advance();
            let mut vals = Vec::new();
            while let Token::Word(_)
            | Token::SingleQuoted(_)
            | Token::DoubleQuoted(_)
            | Token::Backtick(_) = self.peek()
            {
                if let Some(v) = self.expect_word_quoted() {
                    vals.push(v);
                }
            }
            vals
        } else {
            vec!["$@".into()]
        };
        self.skip_newlines();
        if matches!(self.peek(), Token::Semi) {
            self.advance();
        }
        self.skip_newlines();
        if !matches!(self.peek(), Token::Do) {
            eprintln!("context: syntax error: expected `do` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();

        let mut body_cmds = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Token::Done | Token::Eof) {
                break;
            }
            body_cmds.push(self.parse_and_or());
            if matches!(self.peek(), Token::Semi) {
                self.advance();
            }
        }
        if matches!(self.peek(), Token::Done) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `done`");
        }

        let body = if body_cmds.len() == 1 {
            body_cmds.remove(0)
        } else if body_cmds.is_empty() {
            Node::Empty
        } else {
            let mut left = body_cmds.remove(0);
            for cmd in body_cmds {
                left = Node::Compound {
                    kind: CompoundKind::Semicolon,
                    left: Box::new(left),
                    right: Box::new(cmd),
                };
            }
            left
        };

        Node::Select {
            var,
            values,
            body: Box::new(body),
        }
    }

    fn parse_test_double_bracket(&mut self) -> Node {
        self.advance();
        let mut inner_tokens = Vec::new();
        let mut depth = 1u32;
        loop {
            match self.peek() {
                Token::DoubleLBracket => {
                    depth += 1;
                    inner_tokens.push("[[".to_string());
                    self.advance();
                }
                Token::DoubleRBracket => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        break;
                    }
                    inner_tokens.push("]]".to_string());
                    self.advance();
                }
                Token::Eof => break,
                t => {
                    match t {
                        Token::DoubleQuoted(s) => inner_tokens.push(format!("\x01{}\x01", s)),
                        Token::SingleQuoted(s) => inner_tokens.push(format!("\x02{}\x02", s)),
                        other => inner_tokens.push(other.to_string()),
                    }
                    self.advance();
                }
            }
        }
        Node::TestDoubleBracket {
            tokens: inner_tokens,
        }
    }

    fn expect_word_quoted(&mut self) -> Option<String> {
        match self.peek() {
            Token::SingleQuoted(_) => {
                if let Token::SingleQuoted(s) = self.advance() {
                    Some(format!("\x02{}\x02", s))
                } else {
                    None
                }
            }
            Token::DoubleQuoted(_) => {
                if let Token::DoubleQuoted(s) = self.advance() {
                    Some(format!("\x01{}\x01", s))
                } else {
                    None
                }
            }
            Token::Word(_) | Token::Backtick(_) => self.expect_word(),
            _ => None,
        }
    }

    fn collect_balanced_parens(&mut self) -> String {
        let mut depth = 1u32;
        let mut parts = Vec::new();
        parts.push("(".to_string());
        while depth > 0 {
            match self.peek() {
                Token::LParen => {
                    depth += 1;
                    parts.push(self.advance().to_string());
                }
                Token::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        parts.push(")".to_string());
                    } else {
                        parts.push(self.advance().to_string());
                    }
                }
                Token::Eof => break,
                _ => {
                    let s = self.advance().to_string();
                    if !s.is_empty() {
                        parts.push(s);
                    }
                }
            }
        }
        parts.join(" ")
    }

    fn parse_simple_command(&mut self) -> Node {
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        loop {
            match self.peek() {
                Token::Word(_)
                | Token::SingleQuoted(_)
                | Token::DoubleQuoted(_)
                | Token::Backtick(_) => {
                    if let Some(w) = self.expect_word_quoted() {
                        words.push(w);
                    }
                }
                Token::IoNumber(n) => {
                    // Only an fd number directly adjacent to a redirect
                    // operator (guaranteed by the lexer) is a fd prefix.
                    let n = n.clone();
                    let saved = self.pos;
                    self.advance();
                    if matches!(
                        self.peek(),
                        Token::Greater
                            | Token::DoubleGreater
                            | Token::Less
                            | Token::LessLess
                            | Token::LessLessLess
                            | Token::LessAmp
                            | Token::AmpGreater
                            | Token::AmpGreaterGreater
                            | Token::GreaterPipe
                            | Token::GreaterAmp
                            | Token::LessGreater
                    ) && let Ok(fd_num) = n.parse::<u32>()
                        && let Some(r) = self.parse_redirect_with_fd(Some(fd_num))
                    {
                        redirects.push(r);
                        continue;
                    }
                    self.pos = saved;
                    words.push(n);
                    self.advance();
                }
                Token::Greater
                | Token::DoubleGreater
                | Token::Less
                | Token::LessLess
                | Token::LessLessDash
                | Token::LessLessLess
                | Token::LessAmp
                | Token::AmpGreater
                | Token::AmpGreaterGreater
                | Token::GreaterPipe
                | Token::GreaterAmp
                | Token::LessGreater => {
                    if let Some(r) = self.parse_redirect() {
                        redirects.push(r);
                    }
                }
                Token::LessLParen | Token::GreaterLParen => {
                    let prefix = if matches!(self.peek(), Token::LessLParen) {
                        "<"
                    } else {
                        ">"
                    };
                    self.advance();
                    let inner = self.collect_balanced_parens();
                    words.push(format!("{}{}", prefix, inner));
                }
                Token::If
                | Token::Then
                | Token::Elif
                | Token::Else
                | Token::Fi
                | Token::For
                | Token::In
                | Token::Do
                | Token::Done
                | Token::While
                | Token::Until
                | Token::Case
                | Token::Esac
                | Token::Function
                | Token::Select
                | Token::Coproc => {
                    // Reserved words are only special in command position;
                    // after other words they are ordinary arguments
                    // (`echo done`, `case $x in` aside).
                    if words.is_empty() && redirects.is_empty() {
                        break;
                    }
                    let text = self.advance().to_string();
                    words.push(text);
                }
                _ => break,
            }
        }
        if words.is_empty() && redirects.is_empty() {
            return Node::Empty;
        }

        let mut assignments: Vec<(String, String)> = Vec::new();
        let mut cmd_start = 0;
        for (i, w) in words.iter().enumerate() {
            if let Some(eq_pos) = w.find('=')
                && eq_pos > 0
                && !w.starts_with('=')
            {
                let name = w[..eq_pos].to_string();
                let value = w[eq_pos + 1..].to_string();
                assignments.push((name, value));
                cmd_start = i + 1;
                continue;
            }
            break;
        }
        words = words[cmd_start..].to_vec();

        // Bare assignments (no command word) persist in shell state.
        if words.is_empty() && !assignments.is_empty() {
            let mut left = Node::Assignment {
                name: assignments[0].0.clone(),
                value: assignments[0].1.clone(),
            };
            for (name, value) in assignments.into_iter().skip(1) {
                left = Node::Compound {
                    kind: CompoundKind::Semicolon,
                    left: Box::new(left),
                    right: Box::new(Node::Assignment { name, value }),
                };
            }
            return left;
        }

        Node::Command {
            words,
            redirects,
            background: false,
            prefix_env: assignments,
        }
    }

    fn parse_redirect(&mut self) -> Option<Redirect> {
        self.parse_redirect_with_fd(None)
    }

    fn parse_redirect_with_fd(&mut self, fd: Option<u32>) -> Option<Redirect> {
        let kind = match self.peek() {
            Token::Greater => {
                self.advance();
                RedirKind::Output
            }
            Token::DoubleGreater => {
                self.advance();
                RedirKind::OutputAppend
            }
            Token::Less => {
                self.advance();
                RedirKind::Input
            }
            Token::LessLess => {
                self.advance();
                // The lexer captured the body verbatim; it follows the
                // operator token directly.
                let body = match self.peek() {
                    Token::HereDocBody(body, expand) => Some((body.clone(), *expand)),
                    _ => None,
                };
                match body {
                    Some((body, expand)) => {
                        self.advance();
                        RedirKind::HereDocBody(body, expand)
                    }
                    None => RedirKind::HereDocBody(String::new(), true),
                }
            }
            Token::AmpGreater => {
                self.advance();
                RedirKind::OutputFd
            }
            Token::AmpGreaterGreater => {
                self.advance();
                RedirKind::OutputFdAppend
            }
            Token::GreaterPipe => {
                self.advance();
                RedirKind::Clobber
            }
            Token::GreaterAmp => {
                self.advance();
                RedirKind::RedirectFd
            }
            Token::LessLessDash => {
                self.advance();
                // `<<-` bodies were tab-stripped by the lexer as well.
                let body = match self.peek() {
                    Token::HereDocBody(body, expand) => Some((body.clone(), *expand)),
                    _ => None,
                };
                match body {
                    Some((body, expand)) => {
                        self.advance();
                        RedirKind::HereDocBody(body, expand)
                    }
                    None => RedirKind::HereDocBody(String::new(), true),
                }
            }
            Token::LessLessLess => {
                self.advance();
                // Herestring takes exactly ONE word; the rest of the line
                // continues the command normally.
                let word = self.expect_word().unwrap_or_default();
                RedirKind::HereString(word)
            }
            Token::LessAmp => {
                self.advance();
                RedirKind::InputFd
            }
            Token::LessGreater => {
                self.advance();
                RedirKind::RedirectOpen
            }
            _ => return None,
        };
        let target = if matches!(kind, RedirKind::HereDocBody(..) | RedirKind::HereString(..)) {
            String::new()
        } else {
            self.expect_word().unwrap_or_default()
        };
        Some(Redirect { fd, kind, target })
    }

    fn parse_subshell(&mut self) -> Node {
        self.advance();
        let body = self.parse();
        if matches!(self.peek(), Token::RParen) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `)`");
        }
        Node::Subshell {
            body: Box::new(body),
        }
    }

    fn parse_brace_group(&mut self) -> Node {
        self.advance();
        self.skip_newlines();
        let body = self.parse();
        self.skip_newlines();
        if matches!(self.peek(), Token::RBrace) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `}}`");
        }
        Node::BraceGroup {
            body: Box::new(body),
        }
    }

    fn parse_if(&mut self) -> Node {
        self.advance();
        let condition = Box::new(self.parse());
        self.skip_newlines();
        if !matches!(self.peek(), Token::Then) {
            eprintln!("context: syntax error: expected `then` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();
        let then_body = Box::new(self.parse());
        let mut elif = Vec::new();
        let mut else_body = None;
        loop {
            self.skip_newlines();
            match self.peek() {
                Token::Elif => {
                    self.advance();
                    let cond = Box::new(self.parse());
                    self.skip_newlines();
                    if !matches!(self.peek(), Token::Then) {
                        eprintln!("context: syntax error: expected `then` after condition");
                        return Node::Empty;
                    }
                    self.advance();
                    self.skip_newlines();
                    let body = Box::new(self.parse());
                    elif.push((cond, body));
                }
                Token::Else => {
                    self.advance();
                    self.skip_newlines();
                    else_body = Some(Box::new(self.parse()));
                }
                Token::Fi => {
                    self.advance();
                    break;
                }
                Token::Eof => {
                    eprintln!("context: syntax error: unexpected end of input, expected `fi`");
                    break;
                }
                _ => break,
            }
        }
        Node::If {
            condition,
            then_body,
            elif,
            else_body,
        }
    }

    fn parse_while(&mut self) -> Node {
        self.advance();
        let condition = Box::new(self.parse());
        self.skip_newlines();
        if !matches!(self.peek(), Token::Do) {
            eprintln!("context: syntax error: expected `do` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();
        let body = Box::new(self.parse());
        self.skip_newlines();
        if matches!(self.peek(), Token::Done) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `done`");
        }
        Node::While { condition, body }
    }

    fn parse_until(&mut self) -> Node {
        self.advance();
        let condition = Box::new(self.parse());
        self.skip_newlines();
        if !matches!(self.peek(), Token::Do) {
            eprintln!("context: syntax error: expected `do` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();
        let body = Box::new(self.parse());
        self.skip_newlines();
        if matches!(self.peek(), Token::Done) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `done`");
        }
        Node::Until { condition, body }
    }

    fn parse_for(&mut self) -> Node {
        self.advance();
        if matches!(self.peek(), Token::LBraceBrace) {
            return self.parse_for_cstyle();
        }
        let var = self.expect_word().unwrap_or_default();
        let values = if matches!(self.peek(), Token::In) {
            self.advance();
            let mut vals = Vec::new();
            while let Token::Word(_)
            | Token::SingleQuoted(_)
            | Token::DoubleQuoted(_)
            | Token::Backtick(_) = self.peek()
            {
                if let Some(v) = self.expect_word_quoted() {
                    vals.push(v);
                }
            }
            vals
        } else {
            vec!["$@".into()]
        };
        self.skip_newlines();
        if matches!(self.peek(), Token::Semi) {
            self.advance();
        }
        self.skip_newlines();
        if !matches!(self.peek(), Token::Do) {
            eprintln!("context: syntax error: expected `do` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();

        let mut body_cmds = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Token::Done | Token::Eof) {
                break;
            }
            body_cmds.push(self.parse_and_or());
            if matches!(self.peek(), Token::Semi) {
                self.advance();
            }
        }
        if matches!(self.peek(), Token::Done) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `done`");
        }

        let body = if body_cmds.len() == 1 {
            body_cmds.remove(0)
        } else if body_cmds.is_empty() {
            Node::Empty
        } else {
            let mut left = body_cmds.remove(0);
            for cmd in body_cmds {
                left = Node::Compound {
                    kind: CompoundKind::Semicolon,
                    left: Box::new(left),
                    right: Box::new(cmd),
                };
            }
            left
        };

        Node::For {
            var,
            values,
            body: Box::new(body),
        }
    }

    fn parse_for_cstyle(&mut self) -> Node {
        self.advance();
        let mut parts: Vec<String> = Vec::new();
        let mut cur = String::new();
        loop {
            match self.peek() {
                Token::RBraceBrace => {
                    self.advance();
                    break;
                }
                Token::Eof => break,
                Token::Semi => {
                    parts.push(cur.trim().to_string());
                    cur.clear();
                    self.advance();
                }
                t => {
                    let s = t.to_string();
                    cur.push_str(&s);
                    if !s.ends_with('<') && !s.ends_with('>') {
                        cur.push(' ');
                    }
                    self.advance();
                }
            }
        }
        if !cur.trim().is_empty() {
            parts.push(cur.trim().to_string());
        }
        let get = |parts: &[String], i: usize| parts.get(i).filter(|s| !s.is_empty()).cloned();
        let init = get(&parts, 0);
        let cond = get(&parts, 1);
        let incr = get(&parts, 2);
        self.skip_newlines();
        if matches!(self.peek(), Token::Semi) {
            self.advance();
        }
        self.skip_newlines();
        if !matches!(self.peek(), Token::Do) {
            eprintln!("context: syntax error: expected `do` after condition");
            return Node::Empty;
        }
        self.advance();
        self.skip_newlines();

        let mut body_cmds = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Token::Done | Token::Eof) {
                break;
            }
            body_cmds.push(self.parse_and_or());
            if matches!(self.peek(), Token::Semi) {
                self.advance();
            }
        }
        if matches!(self.peek(), Token::Done) {
            self.advance();
        } else if matches!(self.peek(), Token::Eof) {
            eprintln!("context: syntax error: unexpected end of input, expected `done`");
        }

        let body = if body_cmds.len() == 1 {
            body_cmds.remove(0)
        } else if body_cmds.is_empty() {
            Node::Empty
        } else {
            let mut left = body_cmds.remove(0);
            for cmd in body_cmds {
                left = Node::Compound {
                    kind: CompoundKind::Semicolon,
                    left: Box::new(left),
                    right: Box::new(cmd),
                };
            }
            left
        };

        Node::ForArith {
            init,
            cond,
            incr,
            body: Box::new(body),
        }
    }

    fn parse_case(&mut self) -> Node {
        self.advance();
        let word = self.expect_word().unwrap_or_default();
        self.skip_newlines();
        if matches!(self.peek(), Token::In) {
            self.advance();
        }
        let mut arms = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Token::Esac) {
                self.advance();
                break;
            }
            if matches!(self.peek(), Token::Eof) {
                eprintln!("context: syntax error: unexpected end of input, expected `esac`");
                break;
            }
            let mut patterns = Vec::new();
            loop {
                if let Some(p) = self.expect_word() {
                    patterns.push(p);
                }
                if matches!(self.peek(), Token::Pipe) {
                    self.advance();
                    continue;
                }
                break;
            }
            if matches!(self.peek(), Token::RParen) {
                self.advance();
            }
            self.skip_newlines();
            let mut body_cmds = Vec::new();
            loop {
                self.skip_newlines();
                if matches!(
                    self.peek(),
                    Token::DoubleSemi
                        | Token::SemiAmp
                        | Token::SemiSemiAmp
                        | Token::Esac
                        | Token::Eof
                ) {
                    break;
                }
                body_cmds.push(self.parse_and_or());
                if matches!(self.peek(), Token::Semi) {
                    self.advance();
                }
            }
            let mut terminator = CaseTerminator::DoubleSemi;
            match self.peek() {
                Token::DoubleSemi => {
                    self.advance();
                    terminator = CaseTerminator::DoubleSemi;
                }
                Token::SemiAmp => {
                    self.advance();
                    terminator = CaseTerminator::AmpSemi;
                }
                Token::SemiSemiAmp => {
                    self.advance();
                    terminator = CaseTerminator::SemiAmp;
                }
                _ => {}
            }
            let body = if body_cmds.len() == 1 {
                body_cmds.remove(0)
            } else if body_cmds.is_empty() {
                Node::Empty
            } else {
                let mut left = body_cmds.remove(0);
                for cmd in body_cmds {
                    left = Node::Compound {
                        kind: CompoundKind::Semicolon,
                        left: Box::new(left),
                        right: Box::new(cmd),
                    };
                }
                left
            };
            arms.push((patterns, Box::new(body), terminator));
        }
        Node::Case { word, arms }
    }

    fn parse_function_def(&mut self) -> Node {
        self.advance();
        let name = self.expect_word().unwrap_or_default();
        self.skip_newlines();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
        }
        if matches!(self.peek(), Token::RParen) {
            self.advance();
        }
        self.skip_newlines();
        let body = Box::new(self.parse_brace_group());
        Node::Function { name, body }
    }

    fn parse_arith(&mut self) -> Node {
        self.advance();
        let mut expr = String::new();
        loop {
            match self.peek() {
                Token::RBraceBrace => {
                    self.advance();
                    break;
                }
                Token::Eof => break,
                t => {
                    expr.push_str(&t.to_string());
                    expr.push(' ');
                    self.advance();
                }
            }
        }
        expr.truncate(expr.trim_end().len());
        Node::Arithmetic { expr }
    }

    fn parse_coproc(&mut self) -> Node {
        self.advance();
        let name = if let Token::Word(_) = self.peek() {
            if let Some(w) = self.expect_word() {
                match w.as_str() {
                    "if" | "while" | "until" | "for" | "case" | "function" | "select" => {
                        self.pos -= 1;
                        None
                    }
                    _ => Some(w),
                }
            } else {
                None
            }
        } else {
            None
        };
        let body = Box::new(self.parse_command());
        Node::Coproc { name, body }
    }
}

pub fn parse(tokens: Vec<Token>) -> Node {
    Parser::new(tokens).parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::lexer::tokenize;

    #[test]
    fn test_simple_cmd() {
        let ast = parse(tokenize("echo hello"));
        match ast {
            Node::Command { words, .. } => {
                assert_eq!(words, vec!["echo", "hello"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn test_pipe() {
        let ast = parse(tokenize("cat foo | grep bar"));
        match ast {
            Node::Pipeline { commands, .. } => {
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("expected Pipeline"),
        }
    }

    #[test]
    fn test_and() {
        let ast = parse(tokenize("true && echo ok"));
        match ast {
            Node::Compound {
                kind: CompoundKind::And,
                ..
            } => {}
            _ => panic!("expected And compound"),
        }
    }

    #[test]
    fn test_if() {
        let ast = parse(tokenize("if true; then echo hi; fi"));
        match ast {
            Node::If { .. } => {}
            _ => panic!("expected If"),
        }
    }

    #[test]
    fn test_for() {
        let tokens = tokenize("for i in 1 2 3; do echo $i; done");
        eprintln!("tokens: {:?}", tokens);
        let ast = parse(tokens);
        eprintln!("ast: {:?}", ast);
        match ast {
            Node::For { var, values, .. } => {
                assert_eq!(var, "i");
                assert_eq!(values, vec!["1", "2", "3"]);
            }
            _ => panic!("expected For"),
        }
    }

    #[test]
    fn test_assignment() {
        let ast = parse(tokenize("FOO=bar"));
        match ast {
            Node::Assignment { name, value } => {
                assert_eq!(name, "FOO");
                assert_eq!(value, "bar");
            }
            _ => panic!("expected Assignment"),
        }
    }

    #[test]
    fn test_redirect() {
        let ast = parse(tokenize("echo x > /tmp/out.txt"));
        match ast {
            Node::Command {
                words, redirects, ..
            } => {
                assert_eq!(words, vec!["echo", "x"]);
                assert_eq!(redirects.len(), 1);
            }
            _ => panic!("expected Command with redirect"),
        }
    }

    #[test]
    fn test_background() {
        let ast = parse(tokenize("sleep 10 &"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Background,
                left,
                ..
            } => {
                assert!(matches!(*left, Node::Command { .. }));
            }
            _ => panic!("expected Background compound"),
        }
    }

    #[test]
    fn test_or() {
        let ast = parse(tokenize("false || echo fallback"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Or,
                ..
            } => {}
            _ => panic!("expected Or compound"),
        }
    }

    #[test]
    fn test_semicolon_compound() {
        let ast = parse(tokenize("echo a; echo b"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Semicolon,
                ..
            } => {}
            _ => panic!("expected Semicolon compound"),
        }
    }

    #[test]
    fn test_while() {
        let ast = parse(tokenize("while true; do echo loop; done"));
        match ast {
            Node::While { .. } => {}
            _ => panic!("expected While"),
        }
    }

    #[test]
    fn test_until() {
        let ast = parse(tokenize("until false; do echo loop; done"));
        match ast {
            Node::Until { .. } => {}
            _ => panic!("expected Until"),
        }
    }

    #[test]
    fn test_if_elif_else() {
        let ast = parse(tokenize(
            "if false; then echo a; elif true; then echo b; else echo c; fi",
        ));
        match ast {
            Node::If {
                elif, else_body, ..
            } => {
                assert_eq!(elif.len(), 1);
                assert!(else_body.is_some());
            }
            _ => panic!("expected If with elif and else"),
        }
    }

    #[test]
    fn test_case() {
        let ast = parse(tokenize("case x in a) echo A ;; b) echo B ;; esac"));
        match ast {
            Node::Case { word, arms } => {
                assert_eq!(word, "x");
                assert_eq!(arms.len(), 2);
            }
            _ => panic!("expected Case"),
        }
    }

    #[test]
    fn test_function() {
        let ast = parse(tokenize("function greet { echo hello; }"));
        match ast {
            Node::Function { name, .. } => {
                assert_eq!(name, "greet");
            }
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn test_subshell() {
        let ast = parse(tokenize("(echo hi)"));
        match ast {
            Node::Subshell { .. } => {}
            _ => panic!("expected Subshell"),
        }
    }

    #[test]
    fn test_redirect_append() {
        let ast = parse(tokenize("echo x >> /tmp/out.txt"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
            }
            _ => panic!("expected Command with redirect"),
        }
    }

    #[test]
    fn test_redirect_input() {
        let ast = parse(tokenize("cat < /tmp/in.txt"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
            }
            _ => panic!("expected Command with redirect"),
        }
    }

    #[test]
    fn test_redirect_heredoc() {
        let ast = parse(tokenize("cat <<EOF\nhello\nEOF"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
            }
            _ => panic!("expected Command with redirect"),
        }
    }

    #[test]
    fn test_pipeline_chain() {
        let ast = parse(tokenize("cat file | grep pattern | wc -l"));
        match ast {
            Node::Pipeline { commands, .. } => {
                assert_eq!(commands.len(), 3);
            }
            _ => panic!("expected Pipeline with 3 commands"),
        }
    }

    #[test]
    fn test_empty_input() {
        let ast = parse(tokenize(""));
        match ast {
            Node::Empty => {}
            _ => panic!("expected Empty"),
        }
    }

    #[test]
    fn test_bang_pipe() {
        let ast = parse(tokenize("! echo fail"));
        match ast {
            Node::Pipeline { bang, .. } => {
                assert!(bang);
            }
            _ => panic!("expected Pipeline with bang"),
        }
    }

    #[test]
    fn test_coproc_with_name() {
        let ast = parse(tokenize("coproc name { cmd; }"));
        match ast {
            Node::Coproc { name, body } => {
                assert_eq!(name.as_deref(), Some("name"));
                assert!(matches!(*body, Node::BraceGroup { .. }));
            }
            _ => panic!("expected Coproc"),
        }
    }

    #[test]
    fn test_coproc_without_name() {
        let ast = parse(tokenize("coproc { cmd; }"));
        match ast {
            Node::Coproc { name, body } => {
                assert!(name.is_none());
                assert!(matches!(*body, Node::BraceGroup { .. }));
            }
            _ => panic!("expected Coproc without name"),
        }
    }

    #[test]
    fn test_case_with_dollar_var() {
        let ast = parse(tokenize("case $x in pattern) echo hi ;; esac"));
        match ast {
            Node::Case { word, arms } => {
                assert_eq!(word, "$x");
                assert_eq!(arms.len(), 1);
                let patterns = &arms[0].0;
                assert_eq!(patterns, &vec!["pattern"]);
            }
            _ => panic!("expected Case"),
        }
    }

    #[test]
    fn test_for_cstyle() {
        let ast = parse(tokenize("for ((i=0; i<10; i++)) do echo $i; done"));
        match ast {
            Node::ForArith {
                init, cond, incr, ..
            } => {
                assert!(init.is_some());
                assert!(cond.is_some());
                assert!(incr.is_some());
            }
            _ => panic!("expected ForArith"),
        }
    }

    #[test]
    fn test_heredoc_quoted_delimiter() {
        let ast = parse(tokenize("cat <<'EOF'\nhello $USER\nEOF"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
                match &redirects[0].kind {
                    RedirKind::HereDocBody(body, expand) => {
                        assert_eq!(body, "hello $USER\n");
                        assert!(!expand);
                    }
                    _ => panic!("expected HereDocBody"),
                }
            }
            _ => panic!("expected Command with heredoc"),
        }
    }

    #[test]
    fn test_heredoc_verbatim_spacing() {
        let tokens = tokenize("cat <<EOF\nhello    world\t!\n  indented\nEOF\necho done");
        let mut p = Parser::new(tokens);
        let ast = p.parse();
        match ast {
            Node::Compound {
                kind: CompoundKind::Semicolon,
                left,
                right,
            } => {
                assert!(matches!(*left, Node::Command { .. }));
                assert!(matches!(*right, Node::Command { .. }));
                if let Node::Command { redirects, .. } = *left {
                    match &redirects[0].kind {
                        RedirKind::HereDocBody(body, expand) => {
                            assert_eq!(body, "hello    world\t!\n  indented\n");
                            assert!(*expand);
                        }
                        _ => panic!("expected HereDocBody"),
                    }
                }
            }
            other => panic!("expected Semicolon list, got {:?}", other),
        }
    }

    #[test]
    fn test_heredoc_dash_strips_tabs() {
        let ast = parse(tokenize("cat <<-EOF\n\ttabbed line\n\tEOF"));
        match ast {
            Node::Command { redirects, .. } => match &redirects[0].kind {
                RedirKind::HereDocBody(body, _) => {
                    assert_eq!(body, "tabbed line\n");
                }
                _ => panic!("expected HereDocBody"),
            },
            _ => panic!("expected Command with heredoc"),
        }
    }

    #[test]
    fn test_redirect_stderr_to_stdout() {
        let ast = parse(tokenize("cmd 2>&1"));
        match ast {
            Node::Command {
                redirects, words, ..
            } => {
                assert!(words.contains(&"cmd".to_string()));
                assert_eq!(redirects.len(), 1);
                assert_eq!(redirects[0].fd, Some(2));
                assert_eq!(redirects[0].kind, RedirKind::RedirectFd);
                assert_eq!(redirects[0].target, "1");
            }
            _ => panic!("expected Command with fd redirect"),
        }
    }

    #[test]
    fn test_three_stage_pipeline() {
        let ast = parse(tokenize("cmd1 | cmd2 | cmd3"));
        match ast {
            Node::Pipeline { commands, .. } => {
                assert_eq!(commands.len(), 3);
                for cmd in &commands {
                    assert!(matches!(cmd, Node::Command { .. }));
                }
            }
            _ => panic!("expected Pipeline with 3 commands"),
        }
    }

    #[test]
    fn test_background_command() {
        let ast = parse(tokenize("cmd &"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Background,
                left,
                right,
            } => {
                match *left {
                    Node::Command { words, .. } => assert_eq!(words, vec!["cmd"]),
                    _ => panic!("expected background Command"),
                }
                assert!(matches!(*right, Node::Empty));
            }
            _ => panic!("expected Background compound"),
        }
    }

    #[test]
    fn test_background_list_continues() {
        let ast = parse(tokenize("echo a & echo b"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Background,
                left,
                right,
            } => {
                assert!(matches!(*left, Node::Command { .. }));
                assert!(matches!(*right, Node::Command { .. }));
            }
            _ => panic!("expected Background compound with following command"),
        }
    }

    #[test]
    fn test_complex_cmd_sub() {
        let ast = parse(tokenize("echo $(cmd1; cmd2)"));
        match ast {
            Node::Command { words, .. } => {
                assert_eq!(words.len(), 2);
                assert!(words[1].starts_with("$("));
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn test_case_with_multiple_patterns() {
        let ast = parse(tokenize("case $x in a|b) echo AB ;; c) echo C ;; esac"));
        match ast {
            Node::Case { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].0, vec!["a", "b"]);
                assert_eq!(arms[1].0, vec!["c"]);
            }
            _ => panic!("expected Case with multiple patterns"),
        }
    }

    #[test]
    fn test_redirect_here_string() {
        let ast = parse(tokenize("cat <<< hello"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
                match &redirects[0].kind {
                    RedirKind::HereString(content) => {
                        assert_eq!(content, "hello");
                    }
                    _ => panic!("expected HereString"),
                }
            }
            _ => panic!("expected Command with here string"),
        }
    }

    #[test]
    fn test_redirect_clobber() {
        let ast = parse(tokenize("echo x >| file"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 1);
                assert_eq!(redirects[0].kind, RedirKind::Clobber);
            }
            _ => panic!("expected Command with clobber redirect"),
        }
    }

    #[test]
    fn test_nested_arith_in_cmd_sub() {
        let ast = parse(tokenize("echo $((2+3))"));
        match ast {
            Node::Command { words, .. } => {
                assert_eq!(words.len(), 2);
                assert!(words[1].contains("$("));
            }
            _ => panic!("expected Command with arithmetic"),
        }
    }

    #[test]
    fn test_posix_func_def() {
        let ast = parse(tokenize("myfunc() { echo hi; }"));
        match ast {
            Node::Function { name, .. } => {
                assert_eq!(name, "myfunc");
            }
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn test_double_amp_background() {
        let ast = parse(tokenize("echo a && echo b &"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Background,
                left,
                ..
            } => {
                assert!(matches!(
                    *left,
                    Node::Compound {
                        kind: CompoundKind::And,
                        ..
                    }
                ));
            }
            _ => panic!("expected Background compound wrapping And"),
        }
    }

    #[test]
    fn test_nested_if_in_for() {
        let ast = parse(tokenize("for i in 1 2; do if true; then echo $i; fi; done"));
        match ast {
            Node::For { var, body, .. } => {
                assert_eq!(var, "i");
                assert!(matches!(*body, Node::If { .. }));
            }
            _ => panic!("expected For with If body"),
        }
    }

    #[test]
    fn test_case_with_glob_pattern() {
        let ast = parse(tokenize(
            "case $x in *.txt) echo text ;; *) echo other ;; esac",
        ));
        match ast {
            Node::Case { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].0, vec!["*.txt"]);
                assert_eq!(arms[1].0, vec!["*"]);
            }
            _ => panic!("expected Case with glob patterns"),
        }
    }

    #[test]
    fn test_compound_nested_parens() {
        let ast = parse(tokenize("(echo a; echo b)"));
        match ast {
            Node::Subshell { body } => {
                assert!(matches!(
                    *body,
                    Node::Compound {
                        kind: CompoundKind::Semicolon,
                        ..
                    }
                ));
            }
            _ => panic!("expected Subshell"),
        }
    }

    #[test]
    fn test_redirect_multiple() {
        let ast = parse(tokenize("echo x > /tmp/a.txt 2> /tmp/b.txt"));
        match ast {
            Node::Command { redirects, .. } => {
                assert_eq!(redirects.len(), 2);
            }
            _ => panic!("expected Command with 2 redirects"),
        }
    }

    #[test]
    fn test_while_with_pipe_body() {
        let ast = parse(tokenize("while read line; do echo $line; done < input.txt"));
        match ast {
            Node::While { .. } => {}
            _ => panic!("expected While"),
        }
    }

    #[test]
    fn test_for_empty_values() {
        let ast = parse(tokenize("for i in; do echo $i; done"));
        match ast {
            Node::For { var, values, .. } => {
                assert_eq!(var, "i");
                assert!(values.is_empty());
            }
            _ => panic!("expected For with empty values"),
        }
    }

    #[test]
    fn test_case_with_semicolons() {
        let ast = parse(tokenize("case x in a) ;; b) ;; esac"));
        match ast {
            Node::Case { arms, .. } => {
                assert_eq!(arms.len(), 2);
            }
            _ => panic!("expected Case"),
        }
    }

    #[test]
    fn test_and_or_chain() {
        let ast = parse(tokenize("cmd1 && cmd2 || cmd3"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Or,
                ..
            } => {}
            _ => panic!("expected Or compound"),
        }
    }

    #[test]
    fn test_function_with_body() {
        let ast = parse(tokenize("myfunc() { echo hi; echo bye; }"));
        match ast {
            Node::Function { name, .. } => {
                assert_eq!(name, "myfunc");
            }
            _ => panic!("expected Function"),
        }
    }

    // C5: `cmd & cmd2` wraps the left side in a Background compound and the
    // right side still runs — no `&` is consumed inside the simple command.
    #[test]
    fn test_background_list() {
        let ast = parse(tokenize("echo a & echo b"));
        match ast {
            Node::Compound {
                kind: CompoundKind::Background,
                left,
                right,
            } => {
                match *left {
                    Node::Command {
                        words, background, ..
                    } => {
                        assert_eq!(words, vec!["echo", "a"]);
                        assert!(!background);
                    }
                    _ => panic!("expected left Command"),
                }
                match *right {
                    Node::Command { words, .. } => {
                        assert_eq!(words, vec!["echo", "b"]);
                    }
                    _ => panic!("expected right Command"),
                }
            }
            _ => panic!("expected Background compound"),
        }
    }

    // C8 residual: reserved words after other words are plain arguments.
    #[test]
    fn test_reserved_word_as_argument() {
        let ast = parse(tokenize("echo done"));
        match ast {
            Node::Command { words, .. } => assert_eq!(words, vec!["echo", "done"]),
            _ => panic!("expected Command"),
        }
    }
}
