#[derive(Debug, Clone, PartialEq)]
pub enum RedirKind {
    Output,
    OutputAppend,
    Input,
    HereDocBody(String, bool),
    HereString(String),
    Clobber,
    OutputFd,
    OutputFdAppend,
    InputFd,
    RedirectFd,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompoundKind {
    And,
    Or,
    Semicolon,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Command {
        words: Vec<String>,
        redirects: Vec<Redirect>,
        background: bool,
    },
    Pipeline {
        commands: Vec<Node>,
        bang: bool,
    },
    Compound {
        kind: CompoundKind,
        left: Box<Node>,
        right: Box<Node>,
    },
    Subshell {
        body: Box<Node>,
    },
    BraceGroup {
        body: Box<Node>,
    },
    For {
        var: String,
        values: Vec<String>,
        body: Box<Node>,
    },
    ForArith {
        init: Option<String>,
        cond: Option<String>,
        incr: Option<String>,
        body: Box<Node>,
    },
    While {
        condition: Box<Node>,
        body: Box<Node>,
    },
    Until {
        condition: Box<Node>,
        body: Box<Node>,
    },
    If {
        condition: Box<Node>,
        then_body: Box<Node>,
        elif: Vec<(Box<Node>, Box<Node>)>,
        else_body: Option<Box<Node>>,
    },
    Case {
        word: String,
        arms: Vec<(Vec<String>, Box<Node>)>,
    },
    Function {
        name: String,
        body: Box<Node>,
    },
    Select {
        var: String,
        values: Vec<String>,
        body: Box<Node>,
    },
    Assignment {
        name: String,
        value: String,
    },
    Arithmetic {
        expr: String,
    },
    TestDoubleBracket {
        tokens: Vec<String>,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Redirect {
    pub fd: Option<u32>,
    pub kind: RedirKind,
    pub target: String,
}

impl Node {
    pub fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }
}
