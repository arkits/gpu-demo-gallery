//! The part of Lua 5.0 that frame scripts use, with the engine's C comments, `#include`
//! and vector types.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glam::Mat4;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    Text(Rc<str>),
    Table(Rc<RefCell<Table>>),
    Function(Rc<Function>),
    /// A function of the engine, by name.
    Native(Rc<str>),
    /// Three or four numbers, shared like a table.
    Vector(Rc<RefCell<[f32; 4]>>, usize),
    Matrix(Mat4),
    /// A light of the current scene.
    Light(usize),
}

#[derive(Debug, Default)]
pub struct Table {
    list: HashMap<i64, Value>,
    named: HashMap<Rc<str>, Value>,
}

#[derive(Debug)]
pub struct Function {
    code: Rc<(Vec<Rc<str>>, Vec<Stmt>)>,
    /// Variables of the functions around it.
    scopes: Vec<Scope>,
}

type Scope = Rc<RefCell<HashMap<Rc<str>, Value>>>;

impl Value {
    pub fn text(s: &str) -> Self {
        Value::Text(s.into())
    }
    pub fn vector(v: [f32; 4], n: usize) -> Self {
        Value::Vector(Rc::new(RefCell::new(v)), n)
    }
    pub fn table() -> Self {
        Value::Table(Default::default())
    }
    pub fn truth(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }
    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Number(v) => Some(*v),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            Value::Text(t) => t.trim().parse().ok(),
            _ => None,
        }
    }
    /// The same thing, seen as numbers for a shader.
    pub fn floats(&self) -> Option<Vec<[f32; 4]>> {
        match self {
            Value::Number(v) => Some(vec![[*v as f32; 4]]),
            Value::Bool(b) => Some(vec![[f32::from(u8::from(*b)); 4]]),
            Value::Vector(v, _) => Some(vec![*v.borrow()]),
            Value::Matrix(m) => Some(m.transpose().to_cols_array_2d().to_vec()),
            _ => None,
        }
    }
    pub fn shader_values(&self) -> std::collections::BTreeMap<String, Vec<[f32; 4]>> {
        match self {
            Value::Table(t) => t
                .borrow()
                .named
                .iter()
                .filter_map(|(k, v)| Some((k.to_string(), v.floats()?)))
                .collect(),
            _ => Default::default(),
        }
    }
    pub fn get(&self, key: &str) -> Value {
        match self {
            Value::Table(t) => t.borrow().named.get(key).cloned().unwrap_or(Value::Nil),
            _ => Value::Nil,
        }
    }
    pub fn set(&self, key: &str, value: Value) {
        if let Value::Table(t) = self {
            t.borrow_mut().named.insert(key.into(), value);
        }
    }
    fn show(&self) -> String {
        match self {
            Value::Number(v) if v.fract() == 0.0 => format!("{}", *v as i64),
            Value::Number(v) => format!("{v}"),
            Value::Text(t) => t.to_string(),
            Value::Nil => "nil".into(),
            Value::Bool(b) => b.to_string(),
            v => format!("{v:?}"),
        }
    }
    fn equals(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Light(a), Value::Light(b)) => a == b,
            _ => false,
        }
    }
}

/// What scripts can ask the engine for.
pub trait Host {
    /// Whether the engine has a function of this name.
    fn has(&self, name: &str) -> bool;
    /// A global that the engine keeps.
    fn global(&self, name: &str) -> Option<Value>;
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value>;
    fn light(&mut self, light: usize, method: &str, args: &[Value]) -> Result<Value>;
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(Rc<str>),
    Text(Rc<str>),
    Number(f64),
    Op(&'static str),
}

const OPS: &[&str] = &[
    "...", "..", "==", "~=", "<=", ">=", "+", "-", "*", "/", "^", "=", "<", ">", "(", ")", "{",
    "}", "[", "]", ";", ":", ",", ".",
];

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

fn lex(text: &str) -> Result<Vec<Tok>> {
    let c: Vec<char> = text.chars().collect();
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, x)| c.get(i + k) == Some(&x));
    let find = |i: usize, s: &str| (i..c.len()).find(|&j| at(j, s));
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if at(i, "--[[") {
            i = find(i, "]]").ok_or("unclosed comment")? + 2;
        } else if at(i, "--") || at(i, "//") {
            i = find(i, "\n").unwrap_or(c.len());
        } else if at(i, "/*") {
            i = find(i, "*/").ok_or("unclosed comment")? + 2;
        } else if at(i, "[[") {
            let end = find(i, "]]").ok_or("unclosed string")?;
            let s: String = c[i + 2..end].iter().collect();
            out.push(Tok::Text(s.strip_prefix('\n').unwrap_or(&s).into()));
            i = end + 2;
        } else if ch == '"' || ch == '\'' {
            let mut s = String::new();
            i += 1;
            loop {
                match c.get(i) {
                    None => return Err("unclosed string".into()),
                    Some(q) if *q == ch => break,
                    Some('\\') => {
                        i += 1;
                        s.push(match c.get(i) {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some(x) => *x,
                            None => return Err("unclosed string".into()),
                        });
                    }
                    Some(x) => s.push(*x),
                }
                i += 1;
            }
            i += 1;
            out.push(Tok::Text(s.into()));
        } else if ch.is_ascii_digit()
            || (ch == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let end = (i..c.len())
                .find(|&j| !(c[j].is_ascii_digit() || c[j] == '.' || c[j] == 'e'))
                .unwrap_or(c.len());
            let s: String = c[i..end].iter().collect();
            out.push(Tok::Number(
                s.parse().map_err(|_| format!("bad number {s}"))?,
            ));
            i = end;
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let end = (i..c.len())
                .find(|&j| !(c[j].is_ascii_alphanumeric() || c[j] == '_'))
                .unwrap_or(c.len());
            out.push(Tok::Word(c[i..end].iter().collect::<String>().into()));
            i = end;
        } else {
            let op = OPS
                .iter()
                .find(|o| at(i, o))
                .ok_or_else(|| format!("unexpected {ch:?}"))?;
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

#[derive(Debug)]
enum Expr {
    Value(Value),
    Name(Rc<str>),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Method(Box<Expr>, Rc<str>, Vec<Expr>),
    Function(Rc<(Vec<Rc<str>>, Vec<Stmt>)>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Table(Vec<(Option<Expr>, Expr)>),
}

#[derive(Debug)]
enum Stmt {
    Local(Vec<Rc<str>>, Vec<Expr>),
    Assign(Vec<Expr>, Vec<Expr>),
    Call(Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    For(Rc<str>, Expr, Expr, Option<Expr>, Vec<Stmt>),
    Do(Vec<Stmt>),
    Return(Option<Expr>),
    Break,
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn is(&self, op: &str) -> bool {
        match self.peek() {
            Some(Tok::Op(o)) => *o == op,
            Some(Tok::Word(w)) => &**w == op && KEYWORDS.contains(&op),
            _ => false,
        }
    }
    fn eat(&mut self, op: &str) -> bool {
        let yes = self.is(op);
        self.at += usize::from(yes);
        yes
    }
    fn expect(&mut self, op: &str) -> Result<()> {
        match self.eat(op) {
            true => Ok(()),
            false => {
                let near: Vec<_> = self.toks[self.at.saturating_sub(5)..]
                    .iter()
                    .take(9)
                    .collect();
                Err(format!("expected {op:?} near {near:?}"))
            }
        }
    }
    fn name(&mut self) -> Result<Rc<str>> {
        match self.peek().cloned() {
            Some(Tok::Word(w)) if !KEYWORDS.contains(&&*w) => {
                self.at += 1;
                Ok(w)
            }
            t => Err(format!("expected a name, found {t:?}")),
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        let mut out = Vec::new();
        while self.peek().is_some()
            && !["end", "else", "elseif", "until"]
                .iter()
                .any(|w| self.is(w))
        {
            out.push(self.statement()?);
            while self.eat(";") {}
        }
        Ok(out)
    }

    fn statement(&mut self) -> Result<Stmt> {
        if self.eat("local") {
            let mut names = vec![self.name()?];
            while self.eat(",") {
                names.push(self.name()?);
            }
            let values = if self.eat("=") {
                self.list()?
            } else {
                Vec::new()
            };
            return Ok(Stmt::Local(names, values));
        }
        if self.eat("function") {
            let mut target = Expr::Name(self.name()?);
            while self.eat(".") {
                let key = Expr::Value(Value::Text(self.name()?));
                target = Expr::Index(Box::new(target), Box::new(key));
            }
            let f = self.function()?;
            return Ok(Stmt::Assign(vec![target], vec![f]));
        }
        if self.eat("if") {
            let mut arms = Vec::new();
            let mut last = Vec::new();
            loop {
                let c = self.expr(0)?;
                self.expect("then")?;
                arms.push((c, self.block()?));
                if self.eat("elseif") {
                    continue;
                }
                if self.eat("else") {
                    last = self.block()?;
                }
                self.expect("end")?;
                return Ok(Stmt::If(arms, last));
            }
        }
        if self.eat("while") {
            let c = self.expr(0)?;
            self.expect("do")?;
            let b = self.block()?;
            self.expect("end")?;
            return Ok(Stmt::While(c, b));
        }
        if self.eat("repeat") {
            let b = self.block()?;
            self.expect("until")?;
            return Ok(Stmt::Repeat(b, self.expr(0)?));
        }
        if self.eat("for") {
            let n = self.name()?;
            self.expect("=")?;
            let a = self.expr(0)?;
            self.expect(",")?;
            let b = self.expr(0)?;
            let s = if self.eat(",") {
                Some(self.expr(0)?)
            } else {
                None
            };
            self.expect("do")?;
            let body = self.block()?;
            self.expect("end")?;
            return Ok(Stmt::For(n, a, b, s, body));
        }
        if self.eat("do") {
            let b = self.block()?;
            self.expect("end")?;
            return Ok(Stmt::Do(b));
        }
        if self.eat("return") {
            let end = self.peek().is_none()
                || ["end", "else", "elseif", "until", ";"]
                    .iter()
                    .any(|w| self.is(w));
            return Ok(Stmt::Return(if end { None } else { Some(self.expr(0)?) }));
        }
        if self.eat("break") {
            return Ok(Stmt::Break);
        }
        let first = self.suffixed()?;
        if matches!(first, Expr::Call(..) | Expr::Method(..)) {
            return Ok(Stmt::Call(first));
        }
        let mut targets = vec![first];
        while self.eat(",") {
            targets.push(self.suffixed()?);
        }
        self.expect("=")?;
        Ok(Stmt::Assign(targets, self.list()?))
    }

    fn list(&mut self) -> Result<Vec<Expr>> {
        let mut v = vec![self.expr(0)?];
        while self.eat(",") {
            v.push(self.expr(0)?);
        }
        Ok(v)
    }

    fn function(&mut self) -> Result<Expr> {
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.eat(")") {
            params.push(self.name()?);
            self.eat(",");
        }
        let body = self.block()?;
        self.expect("end")?;
        Ok(Expr::Function(Rc::new((params, body))))
    }

    fn expr(&mut self, level: usize) -> Result<Expr> {
        const LEVELS: &[&[&str]] = &[
            &["or"],
            &["and"],
            &["<", ">", "<=", ">=", "~=", "=="],
            &[".."],
            &["+", "-"],
            &["*", "/"],
        ];
        let Some(ops) = LEVELS.get(level) else {
            return self.unary();
        };
        let mut left = self.expr(level + 1)?;
        while let Some(op) = ops.iter().find(|o| self.is(o)) {
            self.at += 1;
            let (a, b) = (Box::new(left), Box::new(self.expr(level + 1)?));
            left = match *op {
                "and" => Expr::And(a, b),
                "or" => Expr::Or(a, b),
                op => Expr::Binary(op, a, b),
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr> {
        for op in ["not", "-"] {
            if self.eat(op) {
                return Ok(Expr::Unary(op, Box::new(self.unary()?)));
            }
        }
        let base = self.simple()?;
        match self.eat("^") {
            true => Ok(Expr::Binary("^", Box::new(base), Box::new(self.unary()?))),
            false => Ok(base),
        }
    }

    fn simple(&mut self) -> Result<Expr> {
        if self.eat("nil") {
            return Ok(Expr::Value(Value::Nil));
        }
        for w in ["true", "false"] {
            if self.eat(w) {
                return Ok(Expr::Value(Value::Bool(w == "true")));
            }
        }
        if self.eat("function") {
            return self.function();
        }
        match self.peek().cloned() {
            Some(Tok::Number(v)) => {
                self.at += 1;
                Ok(Expr::Value(Value::Number(v)))
            }
            Some(Tok::Text(t)) => {
                self.at += 1;
                Ok(Expr::Value(Value::Text(t)))
            }
            Some(Tok::Op("{")) => self.table(),
            _ => self.suffixed(),
        }
    }

    fn table(&mut self) -> Result<Expr> {
        self.expect("{")?;
        let mut fields = Vec::new();
        while !self.eat("}") {
            if self.eat("[") {
                let k = self.expr(0)?;
                self.expect("]")?;
                self.expect("=")?;
                fields.push((Some(k), self.expr(0)?));
            } else if matches!(self.peek(), Some(Tok::Word(_)))
                && self.toks.get(self.at + 1) == Some(&Tok::Op("="))
            {
                let k = Expr::Value(Value::Text(self.name()?));
                self.expect("=")?;
                fields.push((Some(k), self.expr(0)?));
            } else {
                fields.push((None, self.expr(0)?));
            }
            if !self.eat(",") {
                self.eat(";");
            }
        }
        Ok(Expr::Table(fields))
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        // `f{...}` and `f"..."` pass one table or string.
        if self.is("{") {
            return Ok(vec![self.table()?]);
        }
        if let Some(Tok::Text(t)) = self.peek().cloned() {
            self.at += 1;
            return Ok(vec![Expr::Value(Value::Text(t))]);
        }
        self.expect("(")?;
        let mut v = Vec::new();
        while !self.eat(")") {
            v.push(self.expr(0)?);
            if !self.is(")") {
                self.expect(",")?;
            }
        }
        Ok(v)
    }

    fn suffixed(&mut self) -> Result<Expr> {
        let mut e = if self.eat("(") {
            let e = self.expr(0)?;
            self.expect(")")?;
            e
        } else {
            Expr::Name(self.name()?)
        };
        loop {
            if self.eat(".") {
                let k = Expr::Value(Value::Text(self.name()?));
                e = Expr::Index(Box::new(e), Box::new(k));
            } else if self.eat("[") {
                let k = self.expr(0)?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(k));
            } else if self.eat(":") {
                let n = self.name()?;
                e = Expr::Method(Box::new(e), n, self.args()?);
            } else if self.is("(") || self.is("{") || matches!(self.peek(), Some(Tok::Text(_))) {
                e = Expr::Call(Box::new(e), self.args()?);
            } else {
                return Ok(e);
            }
        }
    }
}

enum Flow {
    Go,
    Break,
    Return(Value),
}

thread_local! {
    /// The C library's `rand()` state, shared by `math.random` and the engine's random
    /// parameters as in the original: Microsoft's, which starts at 1.
    static SEED: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

/// MSVC `rand()` (0x51d7b4 in Toy Shop): 0 to 32767.
pub fn crt_rand() -> u32 {
    SEED.with(|s| {
        let next = s.get().wrapping_mul(214_013).wrapping_add(2_531_011);
        s.set(next);
        (next >> 16) & 0x7fff
    })
}

/// MSVC `srand()`.
pub fn crt_srand(seed: u32) {
    SEED.with(|s| s.set(seed));
}

/// Global variables, which outlive a frame.
#[derive(Default)]
pub struct State {
    globals: HashMap<Rc<str>, Value>,
}

struct Run<'a> {
    state: &'a mut State,
    host: &'a mut dyn Host,
    scopes: Vec<Scope>,
    steps: u32,
}

impl State {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.globals.insert("shaderParam".into(), Value::table());
        s
    }

    pub fn global(&self, name: &str) -> Value {
        self.globals.get(name).cloned().unwrap_or(Value::Nil)
    }

    /// Runs a file, and its `Initialize` if it has one.
    pub fn load(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        file: &dyn Fn(&str) -> Option<String>,
    ) -> Result<()> {
        let text = file(name).ok_or_else(|| format!("no script {name}"))?;
        let mut plain = String::new();
        for line in text.lines() {
            match line.trim().strip_prefix("#include") {
                Some(n) => self.load(host, n.trim().trim_matches('"'), file)?,
                None => plain.push_str(line),
            }
            plain.push('\n');
        }
        self.globals.remove("Initialize");
        self.execute(host, &plain)
            .map_err(|e| format!("{name}: {e}"))?;
        if let Some(f) = self.globals.remove("Initialize") {
            self.run(host, |r| r.invoke(&f, Vec::new()))
                .map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }

    pub fn execute(&mut self, host: &mut dyn Host, text: &str) -> Result<()> {
        let mut p = Parser {
            toks: lex(text)?,
            at: 0,
        };
        let body = p.block()?;
        if p.peek().is_some() {
            return Err(format!("unexpected {:?}", p.peek()));
        }
        self.run(host, |r| r.block(&body)).map(|_| ())
    }

    fn run<T>(&mut self, host: &mut dyn Host, f: impl FnOnce(&mut Run) -> Result<T>) -> Result<T> {
        f(&mut Run {
            state: self,
            host,
            scopes: Vec::new(),
            steps: 0,
        })
    }
}

fn field(t: &Value, k: &Value) -> Result<Value> {
    match (t, k) {
        (Value::Table(t), Value::Text(k)) => {
            Ok(t.borrow().named.get(k).cloned().unwrap_or(Value::Nil))
        }
        (Value::Table(t), Value::Number(n)) => Ok(t
            .borrow()
            .list
            .get(&(*n as i64))
            .cloned()
            .unwrap_or(Value::Nil)),
        (Value::Vector(v, n), Value::Text(k)) => {
            match "xyzw".find(&**k).filter(|i| k.len() == 1 && i < n) {
                Some(i) => Ok(Value::Number(f64::from(v.borrow()[i]))),
                None => Err(format!("no component {k}")),
            }
        }
        (t, k) => Err(format!("cannot index {t:?} with {k:?}")),
    }
}

impl Run<'_> {
    fn block(&mut self, body: &[Stmt]) -> Result<Flow> {
        self.scopes.push(Default::default());
        let mut flow = Ok(Flow::Go);
        for s in body {
            flow = self.statement(s);
            if !matches!(flow, Ok(Flow::Go)) {
                break;
            }
        }
        self.scopes.pop();
        flow
    }

    fn declare(&mut self, name: &Rc<str>, v: Value) {
        self.scopes
            .last()
            .expect("scope")
            .borrow_mut()
            .insert(name.clone(), v);
    }

    fn statement(&mut self, s: &Stmt) -> Result<Flow> {
        self.steps += 1;
        if self.steps > 2_000_000 {
            return Err("script does not end".into());
        }
        match s {
            Stmt::Local(names, values) => {
                let values: Vec<Value> =
                    values.iter().map(|v| self.expr(v)).collect::<Result<_>>()?;
                for (i, n) in names.iter().enumerate() {
                    self.declare(n, values.get(i).cloned().unwrap_or(Value::Nil));
                }
            }
            Stmt::Assign(targets, values) => {
                let values: Vec<Value> =
                    values.iter().map(|v| self.expr(v)).collect::<Result<_>>()?;
                for (i, t) in targets.iter().enumerate() {
                    self.assign(t, values.get(i).cloned().unwrap_or(Value::Nil))?;
                }
            }
            Stmt::Call(e) => {
                self.expr(e)?;
            }
            Stmt::If(arms, last) => {
                for (c, body) in arms {
                    if self.expr(c)?.truth() {
                        return self.block(body);
                    }
                }
                return self.block(last);
            }
            Stmt::While(c, body) => {
                while self.expr(c)?.truth() {
                    match self.block(body)? {
                        Flow::Go => {}
                        Flow::Break => break,
                        f => return Ok(f),
                    }
                }
            }
            Stmt::Repeat(body, c) => loop {
                match self.block(body)? {
                    Flow::Go => {}
                    Flow::Break => break,
                    f => return Ok(f),
                }
                if self.expr(c)?.truth() {
                    break;
                }
            },
            Stmt::For(n, a, b, s, body) => {
                let number = |v: Value| v.number().ok_or("for needs numbers".to_string());
                let (mut i, end) = (number(self.expr(a)?)?, number(self.expr(b)?)?);
                let step = match s {
                    Some(s) => number(self.expr(s)?)?,
                    None => 1.0,
                };
                while if step >= 0.0 { i <= end } else { i >= end } {
                    self.scopes.push(Default::default());
                    self.declare(n, Value::Number(i));
                    let flow = self.block(body);
                    self.scopes.pop();
                    match flow? {
                        Flow::Go => {}
                        Flow::Break => break,
                        f => return Ok(f),
                    }
                    i += step;
                }
            }
            Stmt::Do(body) => return self.block(body),
            Stmt::Return(v) => {
                return Ok(Flow::Return(match v {
                    Some(v) => self.expr(v)?,
                    None => Value::Nil,
                }));
            }
            Stmt::Break => return Ok(Flow::Break),
        }
        Ok(Flow::Go)
    }

    fn assign(&mut self, target: &Expr, v: Value) -> Result<()> {
        match target {
            Expr::Name(n) => {
                for s in self.scopes.iter().rev() {
                    if let Some(old) = s.borrow_mut().get_mut(n) {
                        *old = v;
                        return Ok(());
                    }
                }
                self.state.globals.insert(n.clone(), v);
                Ok(())
            }
            Expr::Index(t, k) => {
                let (t, k) = (self.expr(t)?, self.expr(k)?);
                match (&t, &k) {
                    (Value::Table(t), Value::Text(k)) => {
                        t.borrow_mut().named.insert(k.clone(), v);
                    }
                    (Value::Table(t), Value::Number(n)) => {
                        t.borrow_mut().list.insert(*n as i64, v);
                    }
                    (Value::Vector(x, _), Value::Text(k)) => {
                        let i = "xyzw"
                            .find(&**k)
                            .ok_or_else(|| format!("no component {k}"))?;
                        x.borrow_mut()[i] = v.number().ok_or("components are numbers")? as f32;
                    }
                    _ => return Err(format!("cannot set {k:?} of {t:?}")),
                }
                Ok(())
            }
            e => Err(format!("cannot assign to {e:?}")),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<Value> {
        Ok(match e {
            Expr::Value(v) => v.clone(),
            Expr::Name(n) => {
                for s in self.scopes.iter().rev() {
                    if let Some(v) = s.borrow().get(n) {
                        return Ok(v.clone());
                    }
                }
                if let Some(v) = self.state.globals.get(n) {
                    return Ok(v.clone());
                }
                if let Some(v) = self.host.global(n) {
                    return Ok(v);
                }
                match &**n {
                    "math" | "table" | "AtiVector3" | "AtiVector4" | "AtiMatrix" => {
                        Value::Native(n.clone())
                    }
                    // The original's tweaking panels: built, never shown.
                    n if n.starts_with("UI") => Value::Native(n.into()),
                    "ConnectSignal" | "ErrorLog" | "AtiVector3ToString" | "AtiVector4ToString" => {
                        Value::Native(n.clone())
                    }
                    n if self.host.has(n) => Value::Native(n.into()),
                    _ => Value::Nil,
                }
            }
            Expr::Index(t, k) => {
                let (t, k) = (self.expr(t)?, self.expr(k)?);
                match (&t, &k) {
                    (Value::Native(lib), Value::Text(k)) => {
                        Value::Native(format!("{lib}.{k}").into())
                    }
                    _ => field(&t, &k)?,
                }
            }
            Expr::Call(f, args) => {
                let name = match &**f {
                    Expr::Name(n) => Some(n.clone()),
                    _ => None,
                };
                let f = self.expr(f)?;
                if let (Value::Nil, Some(n)) = (&f, name) {
                    return Err(format!("no function {n}"));
                }
                let args = args.iter().map(|a| self.expr(a)).collect::<Result<_>>()?;
                self.invoke(&f, args)?
            }
            Expr::Method(o, name, args) => {
                let o = self.expr(o)?;
                let args: Vec<Value> = args.iter().map(|a| self.expr(a)).collect::<Result<_>>()?;
                self.method(o, name, args)?
            }
            Expr::Function(code) => Value::Function(Rc::new(Function {
                code: code.clone(),
                scopes: self.scopes.clone(),
            })),
            Expr::Unary(op, e) => {
                let v = self.expr(e)?;
                match (*op, &v) {
                    ("not", v) => Value::Bool(!v.truth()),
                    (_, Value::Vector(x, n)) => Value::vector(x.borrow().map(|c| -c), *n),
                    (_, v) => Value::Number(-v.number().ok_or("cannot negate")?),
                }
            }
            Expr::And(a, b) => {
                let a = self.expr(a)?;
                if a.truth() { self.expr(b)? } else { a }
            }
            Expr::Or(a, b) => {
                let a = self.expr(a)?;
                if a.truth() { a } else { self.expr(b)? }
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                binary(op, a, b)?
            }
            Expr::Table(fields) => {
                let mut t = Table::default();
                let mut next = 1;
                for (k, v) in fields {
                    let k = k.as_ref().map(|k| self.expr(k)).transpose()?;
                    let v = self.expr(v)?;
                    match k {
                        None => {
                            t.list.insert(next, v);
                            next += 1;
                        }
                        Some(Value::Number(n)) => {
                            t.list.insert(n as i64, v);
                        }
                        Some(Value::Text(k)) => {
                            t.named.insert(k, v);
                        }
                        Some(k) => return Err(format!("bad key {k:?}")),
                    }
                }
                Value::Table(Rc::new(RefCell::new(t)))
            }
        })
    }

    fn invoke(&mut self, f: &Value, args: Vec<Value>) -> Result<Value> {
        match f {
            Value::Function(f) => {
                let f = f.clone();
                let scopes = std::mem::replace(&mut self.scopes, f.scopes.clone());
                self.scopes.push(Default::default());
                for (i, p) in f.code.0.iter().enumerate() {
                    self.declare(p, args.get(i).cloned().unwrap_or(Value::Nil));
                }
                let flow = self.block(&f.code.1);
                self.scopes = scopes;
                match flow? {
                    Flow::Return(v) => Ok(v),
                    _ => Ok(Value::Nil),
                }
            }
            Value::Native(n) => {
                let n = n.clone();
                let number = |i: usize| args.get(i).and_then(Value::number);
                match &*n {
                    // Lua 5.0's math library on the C `rand()`.
                    "math.randomseed" => {
                        crt_srand(number(0).unwrap_or(0.0) as i64 as u32);
                        return Ok(Value::Nil);
                    }
                    "math.random" => {
                        const MAX: f64 = 32767.0;
                        let r = f64::from(crt_rand() % 32767) / MAX;
                        // The bounds are C ints (`luaL_checkint`): truncated.
                        let int = |v: f64| v.trunc();
                        return Ok(Value::Number(match (number(0), number(1)) {
                            (None, _) => r,
                            (Some(m), None) => (r * int(m)).floor() + 1.0,
                            (Some(m), Some(n)) => (r * (int(n) - int(m) + 1.0)).floor() + int(m),
                        }));
                    }
                    "ConnectSignal" | "ErrorLog" => return Ok(Value::Nil),
                    "AtiVector3ToString" | "AtiVector4ToString" => {
                        return Ok(Value::Text(
                            args.first().map(Value::show).unwrap_or_default().into(),
                        ));
                    }
                    _ => {}
                }
                native(&n, &args).unwrap_or_else(|| self.host.call(&n, &args))
            }
            f => Err(format!("cannot call {f:?}")),
        }
    }

    fn method(&mut self, o: Value, name: &str, args: Vec<Value>) -> Result<Value> {
        let number = |i: usize| args.get(i).and_then(Value::number).unwrap_or(0.0) as f32;
        match (&o, name) {
            (Value::Native(t), "new") => match &**t {
                "AtiVector3" => Ok(Value::vector([number(0), number(1), number(2), 0.0], 3)),
                "AtiVector4" => Ok(Value::vector(
                    [number(0), number(1), number(2), number(3)],
                    4,
                )),
                "AtiMatrix" => Ok(Value::Matrix(Mat4::IDENTITY)),
                t if t.starts_with("UI") => Ok(Value::Native("UIWidget".into())),
                t => Err(format!("cannot make {t}")),
            },
            (Value::Native(t), m) if &**t == "UIWidget" => Ok(match m.starts_with("Get") {
                true => Value::Number(0.0),
                false => Value::Nil,
            }),
            (Value::Vector(v, n), "Length") => {
                let sum: f32 = v.borrow()[..*n].iter().map(|c| c * c).sum();
                Ok(Value::Number(f64::from(sum.sqrt())))
            }
            (Value::Light(l), m) => self.host.light(*l, m, &args),
            (Value::Table(t), m) => {
                let f = t.borrow().named.get(m).cloned().unwrap_or(Value::Nil);
                let mut all = vec![o.clone()];
                all.extend(args);
                self.invoke(&f, all)
            }
            (o, m) => Err(format!("no method {m} of {o:?}")),
        }
    }
}

fn binary(op: &str, a: Value, b: Value) -> Result<Value> {
    if op == ".." {
        return Ok(Value::Text((a.show() + &b.show()).into()));
    }
    match op {
        "==" => return Ok(Value::Bool(a.equals(&b))),
        "~=" => return Ok(Value::Bool(!a.equals(&b))),
        _ => {}
    }
    if let (Value::Vector(x, n), Value::Vector(y, _)) = (&a, &b) {
        let (x, y) = (*x.borrow(), *y.borrow());
        let f = |f: fn(f32, f32) -> f32| Value::vector(std::array::from_fn(|i| f(x[i], y[i])), *n);
        return match op {
            "+" => Ok(f(|a, b| a + b)),
            "-" => Ok(f(|a, b| a - b)),
            _ => Err(format!("vectors {op}")),
        };
    }
    if let (Value::Vector(x, n), Some(s), "*" | "/") = (&a, b.number(), op) {
        let s = if op == "/" { 1.0 / s } else { s } as f32;
        return Ok(Value::vector(x.borrow().map(|c| c * s), *n));
    }
    let (Some(x), Some(y)) = (a.number(), b.number()) else {
        return Err(format!("{a:?} {op} {b:?}"));
    };
    Ok(match op {
        "+" => Value::Number(x + y),
        "-" => Value::Number(x - y),
        "*" => Value::Number(x * y),
        "/" => Value::Number(x / y),
        "^" => Value::Number(x.powf(y)),
        "<" => Value::Bool(x < y),
        ">" => Value::Bool(x > y),
        "<=" => Value::Bool(x <= y),
        ">=" => Value::Bool(x >= y),
        op => return Err(format!("unknown operator {op}")),
    })
}

/// Lua's own libraries.
fn native(name: &str, args: &[Value]) -> Option<Result<Value>> {
    let x = args.first().and_then(Value::number).unwrap_or(0.0);
    let y = args.get(1).and_then(Value::number).unwrap_or(0.0);
    let number = |v: f64| Some(Ok(Value::Number(v)));
    match name {
        "math.mod" => number(x % y),
        "math.ceil" => number(x.ceil()),
        "math.floor" => number(x.floor()),
        "math.abs" => number(x.abs()),
        "math.sqrt" => number(x.sqrt()),
        "math.sin" => number(x.sin()),
        "math.cos" => number(x.cos()),
        "math.tan" => number(x.tan()),
        "math.rad" => number(x.to_radians()),
        "math.deg" => number(x.to_degrees()),
        "math.min" => number(x.min(y)),
        "math.max" => number(x.max(y)),
        "table.getn" => match args.first() {
            Some(Value::Table(t)) => {
                let t = t.borrow();
                let n = (1..).take_while(|i| t.list.contains_key(i)).count();
                number(n as f64)
            }
            _ => Some(Err("getn of no table".into())),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Calls(Vec<String>);
    impl Host for Calls {
        fn has(&self, name: &str) -> bool {
            name == "Say"
        }
        fn global(&self, name: &str) -> Option<Value> {
            (name == "ONE").then_some(Value::Number(1.0))
        }
        fn call(&mut self, name: &str, args: &[Value]) -> Result<Value> {
            self.0.push(format!(
                "{name} {}",
                args.iter().map(Value::show).collect::<Vec<_>>().join(",")
            ));
            Ok(Value::Nil)
        }
        fn light(&mut self, _: usize, _: &str, _: &[Value]) -> Result<Value> {
            Err("none".into())
        }
    }

    fn said(text: &str) -> Vec<String> {
        let mut c = Calls::default();
        State::new().execute(&mut c, text).unwrap();
        c.0
    }

    #[test]
    fn runs_what_the_scripts_use() {
        let out = said(
            "-- lua\n// c\n/* both */ function f (a, b)\n local t = {[0] = a, [1] = b}\n\
             for i = 0, 3, 1 do Say(\"W\" .. i, t[math.mod(i, 2)]) end\n return a end\n\
             Say(f(5, ONE))\nlocal s = {{frame = 1}, {frame = 9, name = 'x'}}\n\
             if s[2].frame >= 9 and s[3] == nil then Say(table.getn(s), s[2].name) elseif ONE then Say() end\n\
             v = AtiVector3:new(3, 4, 0)\n v.x = v.x * 1 Say((v - AtiVector3:new(0, 0, 0)):Length())\n\
             i = nothing or 7 while true do i = i - 1 if i < 3 then break end end Say(i)",
        );
        assert_eq!(
            out,
            [
                "Say W0,5", "Say W1,1", "Say W2,5", "Say W3,1", "Say 5", "Say 2,x", "Say 5",
                "Say 2"
            ]
        );
    }

    #[test]
    fn errors_are_reported() {
        let mut c = Calls::default();
        assert!(State::new().execute(&mut c, "x = = 1").is_err());
        assert!(State::new().execute(&mut c, "Missing()").is_err());
        assert!(
            State::new()
                .execute(&mut c, "while 1 do x = 1 end")
                .is_err()
        );
    }
}
