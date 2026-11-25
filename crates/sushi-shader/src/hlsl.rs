//! Reads the HLSL of Direct3D 9 (shader models 2 and 3): preprocessor, lexer, parser.
use std::collections::HashMap;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    Bool,
    Int,
    Float,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Void,
    /// One to four components; one is a plain scalar.
    Vec(Scalar, u32),
    /// Rows and columns of floats.
    Mat(u32, u32),
    Sampler,
    Struct(String),
    Array(Box<Ty>, u32),
}

impl Ty {
    pub const FLOAT: Ty = Ty::Vec(Scalar::Float, 1);
    pub const INT: Ty = Ty::Vec(Scalar::Int, 1);
    pub const BOOL: Ty = Ty::Vec(Scalar::Bool, 1);
    pub fn float(n: u32) -> Ty {
        Ty::Vec(Scalar::Float, n)
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Float(f64),
    Int(i64),
    Bool(bool),
    Var(String),
    Call(String, Vec<Expr>),
    /// `float3(...)`, or a cast.
    Make(Ty, Vec<Expr>),
    Cast(Ty, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Unary(&'static str, Box<Expr>),
    /// `++` or `--`; whether it comes before.
    Step(&'static str, bool, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    /// `=` has an empty operator.
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Select(Box<Expr>, Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Declare(Ty, String, Option<Expr>),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    /// Start, condition, step, body.
    For(Vec<Stmt>, Option<Expr>, Vec<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    Discard,
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub ty: Ty,
    pub name: String,
    pub semantic: Option<String>,
    pub direction: Direction,
    /// Used when a call leaves the parameter out.
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub result: Ty,
    pub name: String,
    pub params: Vec<Field>,
    pub semantic: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub ty: Ty,
    pub name: String,
    pub value: Option<Expr>,
    /// Not set by the application.
    pub fixed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub structs: HashMap<String, Vec<Field>>,
    pub globals: HashMap<String, Global>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Float(f64),
    Int(i64),
    Op(&'static str),
}

const OPS: &[&str] = &[
    "<<=", ">>=", "++", "--", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<=", ">=", "&&", "||",
    "<<", ">>", "&=", "|=", "^=", "::", "##", "+", "-", "*", "/", "%", "=", "<", ">", "!", "&",
    "|", "^", "~", "?", ":", ";", ",", ".", "(", ")", "[", "]", "{", "}", "#", "@",
];

fn lex(text: &str) -> Result<Vec<Tok>> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
            let end = (i..c.len())
                .find(|&j| !(c[j].is_ascii_alphanumeric() || c[j] == '_' || c[j] == '$'))
                .unwrap_or(c.len());
            out.push(Tok::Word(c[i..end].iter().collect()));
            i = end;
        } else if ch.is_ascii_digit()
            || (ch == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            if ch == '0' && matches!(c.get(i + 1), Some('x' | 'X')) {
                let end = (i + 2..c.len())
                    .find(|&j| !c[j].is_ascii_hexdigit())
                    .unwrap_or(c.len());
                let digits: String = c[i + 2..end].iter().collect();
                out.push(Tok::Int(
                    i64::from_str_radix(&digits, 16).map_err(|_| "bad number")?,
                ));
                i = end;
                continue;
            }
            let mut end = i;
            let mut float = false;
            while end < c.len() {
                match c[end] {
                    d if d.is_ascii_digit() => {}
                    '.' => float = true,
                    'e' | 'E' => {
                        float = true;
                        if matches!(c.get(end + 1), Some('+' | '-')) {
                            end += 1;
                        }
                    }
                    _ => break,
                }
                end += 1;
            }
            let digits: String = c[i..end].iter().collect();
            if matches!(c.get(end), Some('f' | 'F' | 'h' | 'H')) {
                float = true;
                end += 1;
            }
            out.push(match float {
                true => Tok::Float(digits.parse().map_err(|_| format!("bad number {digits}"))?),
                false => Tok::Int(digits.parse().map_err(|_| format!("bad number {digits}"))?),
            });
            i = end;
        } else if ch == '"' {
            // Strings appear only in pragmas, which are gone by now.
            return Err("string in shader".into());
        } else {
            let op = OPS
                .iter()
                .find(|o| o.chars().enumerate().all(|(k, x)| c.get(i + k) == Some(&x)))
                .ok_or_else(|| format!("unexpected {ch:?}"))?;
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

struct Macro {
    params: Option<Vec<String>>,
    body: Vec<Tok>,
}

/// Follows `#define`, `#ifdef` and their like, and replaces macros.
fn preprocess(text: &str) -> Result<Vec<Tok>> {
    let mut macros: HashMap<String, Macro> = HashMap::new();
    let mut out = Vec::new();
    // Whether this branch is on, and whether any branch was.
    let mut branches: Vec<(bool, bool)> = Vec::new();
    for line in text.lines() {
        let on = branches.iter().all(|b| b.0);
        let line = line.trim();
        let Some(directive) = line.strip_prefix('#') else {
            if on {
                let toks = lex(line)?;
                expand(&toks, &macros, &mut out, 0)?;
            }
            continue;
        };
        let directive = directive.trim();
        let (word, rest) = directive
            .split_once(char::is_whitespace)
            .unwrap_or((directive, ""));
        let rest = rest.trim();
        match word {
            "ifdef" | "ifndef" => {
                let yes = macros.contains_key(rest) == (word == "ifdef");
                branches.push((yes, yes));
            }
            "if" => {
                let mut toks = Vec::new();
                expand(&lex(rest)?, &macros, &mut toks, 0)?;
                let yes = matches!(toks.as_slice(), [Tok::Int(n)] if *n != 0)
                    || match toks.as_slice() {
                        [Tok::Int(a), Tok::Op(o), Tok::Int(b)] => match *o {
                            "==" => a == b,
                            "!=" => a != b,
                            ">" => a > b,
                            "<" => a < b,
                            ">=" => a >= b,
                            "<=" => a <= b,
                            _ => return Err(format!("unsupported #if {rest}")),
                        },
                        [Tok::Int(_)] => false,
                        _ => return Err(format!("unsupported #if {rest}")),
                    };
                branches.push((yes, yes));
            }
            "else" => {
                let b = branches.last_mut().ok_or("stray #else")?;
                *b = (!b.1, true);
            }
            "endif" => {
                branches.pop().ok_or("stray #endif")?;
            }
            _ if !on => {}
            "define" => {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                let name = rest[..end].to_string();
                let mut body = &rest[end..];
                let mut params = None;
                if body.starts_with('(') {
                    let close = body.find(')').ok_or("bad macro")?;
                    params = Some(body[1..close].split(',').map(|p| p.trim().into()).collect());
                    body = &body[close + 1..];
                }
                let body = lex(body)?;
                macros.insert(name, Macro { params, body });
            }
            "undef" => {
                macros.remove(rest);
            }
            "pragma" | "line" | "error" => {}
            w => return Err(format!("unknown directive #{w}")),
        }
    }
    Ok(out)
}

fn expand(
    toks: &[Tok],
    macros: &HashMap<String, Macro>,
    out: &mut Vec<Tok>,
    depth: u32,
) -> Result<()> {
    if depth > 16 {
        return Err("macros too deep".into());
    }
    let mut i = 0;
    while i < toks.len() {
        let found = match &toks[i] {
            Tok::Word(w) => macros.get(w),
            _ => None,
        };
        match found {
            Some(Macro { params: None, body }) => {
                expand(body, macros, out, depth + 1)?;
                i += 1;
            }
            Some(Macro {
                params: Some(names),
                body,
            }) if toks.get(i + 1) == Some(&Tok::Op("(")) => {
                let mut args: Vec<Vec<Tok>> = vec![Vec::new()];
                let mut level = 0;
                i += 1;
                loop {
                    let t = toks.get(i).ok_or("macro call across lines")?;
                    i += 1;
                    match t {
                        Tok::Op("(") => {
                            level += 1;
                            if level == 1 {
                                continue;
                            }
                        }
                        Tok::Op(")") => {
                            level -= 1;
                            if level == 0 {
                                break;
                            }
                        }
                        Tok::Op(",") if level == 1 => {
                            args.push(Vec::new());
                            continue;
                        }
                        _ => {}
                    }
                    args.last_mut().expect("one argument").push(t.clone());
                }
                let mut filled = Vec::new();
                for t in body {
                    match t {
                        Tok::Word(w) if names.contains(w) => {
                            let k = names.iter().position(|n| n == w).expect("found");
                            filled.extend(args.get(k).cloned().unwrap_or_default());
                        }
                        t => filled.push(t.clone()),
                    }
                }
                expand(&filled, macros, out, depth + 1)?;
            }
            _ => {
                out.push(toks[i].clone());
                i += 1;
            }
        }
    }
    Ok(())
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
    program: Program,
}

const QUALIFIERS: &[&str] = &[
    "uniform",
    "extern",
    "volatile",
    "shared",
    "inline",
    "row_major",
    "column_major",
];

fn named_type(w: &str) -> Option<Ty> {
    let (base, scalar) = [
        ("float", Scalar::Float),
        ("half", Scalar::Float),
        ("double", Scalar::Float),
        ("int", Scalar::Int),
        ("uint", Scalar::Int),
        ("bool", Scalar::Bool),
    ]
    .into_iter()
    .find_map(|(n, s)| Some((w.strip_prefix(n)?, s)))?;
    let digit = |c: u8| (b'1'..=b'4').contains(&c).then(|| u32::from(c - b'0'));
    match base.as_bytes() {
        [] => Some(Ty::Vec(scalar, 1)),
        [n] => Some(Ty::Vec(scalar, digit(*n)?)),
        [r, b'x', c] => Some(Ty::Mat(digit(*r)?, digit(*c)?)),
        _ => None,
    }
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn is(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }
    fn is_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(w)) if w == word)
    }
    fn eat(&mut self, op: &str) -> bool {
        let yes = self.is(op);
        self.at += usize::from(yes);
        yes
    }
    fn eat_word(&mut self, word: &str) -> bool {
        let yes = self.is_word(word);
        self.at += usize::from(yes);
        yes
    }
    fn expect(&mut self, op: &str) -> Result<()> {
        if self.eat(op) {
            return Ok(());
        }
        let near: Vec<_> = self.toks[self.at.saturating_sub(6)..]
            .iter()
            .take(10)
            .collect();
        Err(format!("expected {op:?} near {near:?}"))
    }
    fn word(&mut self) -> Result<String> {
        match self.peek().cloned() {
            Some(Tok::Word(w)) => {
                self.at += 1;
                Ok(w)
            }
            other => Err(format!("expected a name, found {other:?}")),
        }
    }

    fn type_at(&self, at: usize) -> Option<Ty> {
        match self.toks.get(at) {
            Some(Tok::Word(w)) => match w.as_str() {
                "void" => Some(Ty::Void),
                "sampler" | "sampler2D" | "samplerCUBE" | "sampler3D" | "texture" => {
                    Some(Ty::Sampler)
                }
                w if self.program.structs.contains_key(w) => Some(Ty::Struct(w.into())),
                w => named_type(w),
            },
            _ => None,
        }
    }

    fn ty(&mut self) -> Result<Ty> {
        let t = self
            .type_at(self.at)
            .ok_or_else(|| format!("not a type: {:?}", self.peek()))?;
        self.at += 1;
        Ok(t)
    }

    /// Array sizes after a name, outermost first; an empty one is 0.
    fn array(&mut self, ty: Ty) -> Result<Ty> {
        let mut sizes = Vec::new();
        while self.eat("[") {
            if self.eat("]") {
                sizes.push(0);
                continue;
            }
            let e = self.expr()?;
            sizes.push(constant(&e).ok_or("array size is not constant")? as u32);
            self.expect("]")?;
        }
        Ok(sizes
            .into_iter()
            .rev()
            .fold(ty, |t, n| Ty::Array(Box::new(t), n)))
    }

    fn semantic(&mut self) -> Result<Option<String>> {
        if !self.eat(":") {
            return Ok(None);
        }
        let w = self.word()?;
        if w == "register" {
            self.expect("(")?;
            while !self.eat(")") {
                self.at += 1;
            }
            return Ok(None);
        }
        Ok(Some(w))
    }

    fn top(&mut self) -> Result<()> {
        if self.eat(";") {
            return Ok(());
        }
        if self.eat_word("struct") {
            let name = self.word()?;
            self.expect("{")?;
            let mut fields = Vec::new();
            while !self.eat("}") {
                while ["centroid", "linear"].iter().any(|w| self.eat_word(w)) {}
                let ty = self.ty()?;
                loop {
                    let name = self.word()?;
                    let ty = self.array(ty.clone())?;
                    let semantic = self.semantic()?;
                    fields.push(Field {
                        ty,
                        name,
                        semantic,
                        direction: Direction::In,
                        default: None,
                    });
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect(";")?;
            }
            self.eat(";");
            self.program.structs.insert(name, fields);
            return Ok(());
        }
        let mut fixed = false;
        loop {
            match self.peek() {
                Some(Tok::Word(w)) if w == "static" || w == "const" => {
                    fixed |= w == "static" || w == "const"
                }
                Some(Tok::Word(w)) if QUALIFIERS.contains(&w.as_str()) => {}
                _ => break,
            }
            self.at += 1;
        }
        let ty = self.ty()?;
        loop {
            let name = self.word()?;
            if self.is("(") {
                return self.function(ty, name);
            }
            let ty = self.array(ty.clone())?;
            self.semantic()?;
            let value = if self.eat("=") {
                Some(self.initial()?)
            } else {
                None
            };
            let ty = sized(ty, value.as_ref());
            let fixed = fixed && value.is_some();
            let global = Global {
                ty,
                name: name.clone(),
                value,
                fixed,
            };
            self.program.globals.entry(name).or_insert(global);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")
    }

    fn function(&mut self, result: Ty, name: String) -> Result<()> {
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.eat(")") {
            let mut direction = Direction::In;
            loop {
                match self.peek() {
                    Some(Tok::Word(w)) if w == "in" || w == "const" => {}
                    Some(Tok::Word(w)) if w == "out" => direction = Direction::Out,
                    Some(Tok::Word(w)) if w == "inout" => direction = Direction::InOut,
                    Some(Tok::Word(w)) if QUALIFIERS.contains(&w.as_str()) => {}
                    _ => break,
                }
                self.at += 1;
            }
            let ty = self.ty()?;
            if ty == Ty::Void {
                continue;
            }
            let name = self.word()?;
            let ty = self.array(ty)?;
            let semantic = self.semantic()?;
            let default = if self.eat("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            params.push(Field {
                ty,
                name,
                semantic,
                direction,
                default,
            });
            self.eat(",");
        }
        let semantic = self.semantic()?;
        if self.eat(";") {
            return Ok(());
        }
        self.expect("{")?;
        let body = self.block()?;
        self.program.functions.push(Function {
            result,
            name,
            params,
            semantic,
            body,
        });
        Ok(())
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        let mut out = Vec::new();
        while !self.eat("}") {
            if self.peek().is_none() {
                return Err("unclosed block".into());
            }
            self.statement(&mut out)?;
        }
        Ok(out)
    }

    fn declares(&self) -> bool {
        let qualifier = |t: Option<&Tok>| matches!(t, Some(Tok::Word(w)) if w == "static" || w == "const" || w == "uniform");
        if qualifier(self.peek()) {
            return true;
        }
        self.type_at(self.at).is_some() && matches!(self.toks.get(self.at + 1), Some(Tok::Word(_)))
    }

    fn declaration(&mut self, out: &mut Vec<Stmt>) -> Result<()> {
        while ["static", "const", "uniform"]
            .iter()
            .any(|w| self.eat_word(w))
        {}
        let ty = self.ty()?;
        loop {
            let name = self.word()?;
            let ty = self.array(ty.clone())?;
            let value = if self.eat("=") {
                Some(self.initial()?)
            } else {
                None
            };
            let ty = sized(ty, value.as_ref());
            out.push(Stmt::Declare(ty, name, value));
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")
    }

    fn single(&mut self) -> Result<Stmt> {
        let mut out = Vec::new();
        self.statement(&mut out)?;
        Ok(match out.len() {
            1 => out.remove(0),
            _ => Stmt::Block(out),
        })
    }

    fn statement(&mut self, out: &mut Vec<Stmt>) -> Result<()> {
        if self.eat(";") {
            return Ok(());
        }
        if self.eat("{") {
            let b = self.block()?;
            out.push(Stmt::Block(b));
            return Ok(());
        }
        // Attributes such as [unroll].
        if self.is("[") {
            while !self.eat("]") {
                self.at += 1;
            }
            return self.statement(out);
        }
        if self.declares() {
            return self.declaration(out);
        }
        if self.eat_word("if") {
            self.expect("(")?;
            let c = self.expr()?;
            self.expect(")")?;
            let yes = self.single()?;
            let no = if self.eat_word("else") {
                Some(Box::new(self.single()?))
            } else {
                None
            };
            out.push(Stmt::If(c, Box::new(yes), no));
            return Ok(());
        }
        if self.eat_word("for") {
            self.expect("(")?;
            let mut first = Vec::new();
            if !self.eat(";") {
                if self.declares() {
                    self.declaration(&mut first)?;
                } else {
                    loop {
                        first.push(Stmt::Expr(self.assignment()?));
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect(";")?;
                }
            }
            let cond = if self.is(";") {
                None
            } else {
                Some(self.expr()?)
            };
            self.expect(";")?;
            let mut step = Vec::new();
            while !self.eat(")") {
                step.push(self.assignment()?);
                self.eat(",");
            }
            let body = self.single()?;
            out.push(Stmt::For(first, cond, step, Box::new(body)));
            return Ok(());
        }
        if self.eat_word("while") {
            self.expect("(")?;
            let c = self.expr()?;
            self.expect(")")?;
            let body = self.single()?;
            out.push(Stmt::While(c, Box::new(body)));
            return Ok(());
        }
        if self.eat_word("do") {
            let body = self.single()?;
            if !self.eat_word("while") {
                return Err("expected while".into());
            }
            self.expect("(")?;
            let c = self.expr()?;
            self.expect(")")?;
            self.expect(";")?;
            out.push(Stmt::DoWhile(Box::new(body), c));
            return Ok(());
        }
        let s = if self.eat_word("return") {
            let value = if self.is(";") {
                None
            } else {
                Some(self.expr()?)
            };
            Stmt::Return(value)
        } else if self.eat_word("break") {
            Stmt::Break
        } else if self.eat_word("continue") {
            Stmt::Continue
        } else if self.eat_word("discard") {
            Stmt::Discard
        } else {
            Stmt::Expr(self.expr()?)
        };
        self.expect(";")?;
        out.push(s);
        Ok(())
    }

    fn initial(&mut self) -> Result<Expr> {
        if !self.eat("{") {
            return self.assignment();
        }
        let mut v = Vec::new();
        while !self.eat("}") {
            v.push(self.initial()?);
            self.eat(",");
        }
        Ok(Expr::List(v))
    }

    fn expr(&mut self) -> Result<Expr> {
        let mut e = self.assignment()?;
        // Only the last value of a comma expression counts; none here has effects.
        while self.eat(",") {
            e = self.assignment()?;
        }
        Ok(e)
    }

    fn assignment(&mut self) -> Result<Expr> {
        let left = self.ternary()?;
        for (op, short) in [
            ("=", ""),
            ("+=", "+"),
            ("-=", "-"),
            ("*=", "*"),
            ("/=", "/"),
            ("%=", "%"),
        ] {
            if self.eat(op) {
                let right = self.assignment()?;
                return Ok(Expr::Assign(short, Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn ternary(&mut self) -> Result<Expr> {
        let c = self.binary(0)?;
        if !self.eat("?") {
            return Ok(c);
        }
        let yes = self.assignment()?;
        self.expect(":")?;
        let no = self.assignment()?;
        Ok(Expr::Select(Box::new(c), Box::new(yes), Box::new(no)))
    }

    fn binary(&mut self, level: usize) -> Result<Expr> {
        const LEVELS: &[&[&str]] = &[
            &["||"],
            &["&&"],
            &["|"],
            &["^"],
            &["&"],
            &["==", "!="],
            &["<", ">", "<=", ">="],
            &["<<", ">>"],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        let Some(ops) = LEVELS.get(level) else {
            return self.unary();
        };
        let mut left = self.binary(level + 1)?;
        while let Some(op) = ops.iter().find(|o| self.is(o)) {
            self.at += 1;
            let right = self.binary(level + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr> {
        for op in ["-", "!", "~"] {
            if self.eat(op) {
                let e = self.unary()?;
                return Ok(match (op, e) {
                    ("-", Expr::Float(v)) => Expr::Float(-v),
                    ("-", Expr::Int(v)) => Expr::Int(-v),
                    (_, e) => Expr::Unary(op, Box::new(e)),
                });
            }
        }
        if self.eat("+") {
            return self.unary();
        }
        for op in ["++", "--"] {
            if self.eat(op) {
                let e = self.unary()?;
                return Ok(Expr::Step(&op[..1], true, Box::new(e)));
            }
        }
        // A cast is a type alone in parentheses.
        if self.is("(")
            && self.type_at(self.at + 1).is_some()
            && self.toks.get(self.at + 2) == Some(&Tok::Op(")"))
        {
            self.at += 1;
            let ty = self.ty()?;
            self.expect(")")?;
            let e = self.unary()?;
            return Ok(Expr::Cast(ty, Box::new(e)));
        }
        self.postfix()
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        self.expect("(")?;
        let mut v = Vec::new();
        while !self.eat(")") {
            v.push(self.assignment()?);
            if !self.is(")") {
                self.expect(",")?;
            }
        }
        Ok(v)
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = match self.peek().cloned() {
            Some(Tok::Float(v)) => {
                self.at += 1;
                Expr::Float(v)
            }
            Some(Tok::Int(v)) => {
                self.at += 1;
                Expr::Int(v)
            }
            Some(Tok::Op("(")) => {
                self.at += 1;
                let e = self.expr()?;
                self.expect(")")?;
                e
            }
            Some(Tok::Word(w)) if w == "true" || w == "false" => {
                self.at += 1;
                Expr::Bool(w == "true")
            }
            Some(Tok::Word(w)) => {
                if let Some(ty) = self.type_at(self.at)
                    && self.toks.get(self.at + 1) == Some(&Tok::Op("("))
                {
                    self.at += 1;
                    Expr::Make(ty, self.args()?)
                } else {
                    self.at += 1;
                    match self.is("(") {
                        true => Expr::Call(w, self.args()?),
                        false => Expr::Var(w),
                    }
                }
            }
            other => return Err(format!("unexpected {other:?}")),
        };
        loop {
            if self.eat(".") {
                e = Expr::Member(Box::new(e), self.word()?);
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(i));
            } else if self.eat("++") {
                e = Expr::Step("+", false, Box::new(e));
            } else if self.eat("--") {
                e = Expr::Step("-", false, Box::new(e));
            } else {
                return Ok(e);
            }
        }
    }
}

/// Fills in the size of `x[] = {...}`.
fn sized(ty: Ty, value: Option<&Expr>) -> Ty {
    match (ty, value) {
        (Ty::Array(t, 0), Some(Expr::List(v))) => Ty::Array(t, v.len() as u32),
        (ty, _) => ty,
    }
}

/// Value of a constant whole number.
pub fn constant(e: &Expr) -> Option<i64> {
    Some(match e {
        Expr::Int(v) => *v,
        Expr::Float(v) => *v as i64,
        Expr::Binary(op, a, b) => {
            let (a, b) = (constant(a)?, constant(b)?);
            match *op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a.checked_div(b)?,
                _ => return None,
            }
        }
        _ => return None,
    })
}

pub fn parse(text: &str) -> Result<Program> {
    let mut p = Parser {
        toks: preprocess(text)?,
        at: 0,
        program: Program::default(),
    };
    while p.peek().is_some() {
        p.top()?;
    }
    Ok(p.program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macros_and_conditions() {
        let t = preprocess(
            "#define N 4\n#define TWICE(x) ((x)*2)\n#ifdef _XBOX\nbad\n#else\nN TWICE(N+1)\n#endif",
        );
        let want = lex("4 ((4+1)*2)");
        assert_eq!(t, want);
    }

    #[test]
    fn parses_functions_and_globals() {
        let p = parse(
            "float4 a[3]; static const float2 k[] = {float2(1,2), float2(3,4)};\n\
             struct I { float4 p : POSITION0; };\n\
             float f(I i, out float x) { for (int j = 0; j < 3; j++) x += a[j].x; return (float)x ? 1 : -.5f; }",
        )
        .unwrap();
        assert_eq!(p.globals["a"].ty, Ty::Array(Box::new(Ty::float(4)), 3));
        assert_eq!(p.globals["k"].ty, Ty::Array(Box::new(Ty::float(2)), 2));
        assert!(p.globals["k"].fixed && !p.globals["a"].fixed);
        assert_eq!(p.functions[0].params[1].direction, Direction::Out);
    }
}
