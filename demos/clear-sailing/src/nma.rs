//! The demo's text scenes (`.nma`): `Scene { lights [ SpotLight name {
//! field value ... } ] // lights ... }`. Every byte must be understood; `//`
//! and `/* */` comments and `,` separators carry no meaning.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
    Ident(String),
    Nums(Vec<f64>),
    Strs(Vec<String>),
    Idents(Vec<String>),
    Nodes(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub kind: String,
    pub name: String,
    pub fields: Vec<(String, Value)>,
}

impl Node {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.0 == field).map(|f| &f.1)
    }
    pub fn num(&self, field: &str, i: usize) -> f64 {
        match self.get(field) {
            Some(Value::Nums(v)) => v.get(i).copied().unwrap_or(0.0),
            Some(Value::Num(v)) => {
                if i == 0 {
                    *v
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }
    pub fn nums(&self, field: &str) -> &[f64] {
        match self.get(field) {
            Some(Value::Nums(v)) => v,
            _ => &[],
        }
    }
    pub fn text(&self, field: &str) -> &str {
        match self.get(field) {
            Some(Value::Str(v)) => v,
            _ => "",
        }
    }
    pub fn ident(&self, field: &str) -> &str {
        match self.get(field) {
            Some(Value::Ident(v)) => v,
            _ => "",
        }
    }
    pub fn strs(&self, field: &str) -> Vec<&str> {
        match self.get(field) {
            Some(Value::Strs(v)) => v.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        }
    }
    pub fn nodes(&self, field: &str) -> &[Node] {
        match self.get(field) {
            Some(Value::Nodes(v)) => v,
            _ => &[],
        }
    }
    pub fn node(&self, field: &str) -> Option<&Node> {
        self.nodes(field).first()
    }
    pub fn map(&self) -> BTreeMap<&str, &Value> {
        self.fields.iter().map(|f| (f.0.as_str(), &f.1)).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Str(String),
    Num(f64),
    Ident(String),
}

fn tokenize(s: &str) -> Result<Vec<Tok>, Error> {
    let b = s.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    let at = |i: usize| *b.get(i).unwrap_or(&0);
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && at(i + 1) == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && at(i + 1) == b'*' {
            i += 2;
            loop {
                if i + 1 >= b.len() {
                    return Err("unterminated comment".into());
                }
                if b[i] == b'*' && b[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else if c == b'{' {
            toks.push(Tok::LBrace);
            i += 1;
        } else if c == b'}' {
            toks.push(Tok::RBrace);
            i += 1;
        } else if c == b'[' {
            toks.push(Tok::LBracket);
            i += 1;
        } else if c == b']' {
            toks.push(Tok::RBracket);
            i += 1;
        } else if c == b',' {
            toks.push(Tok::Comma);
            i += 1;
        } else if c == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            if j >= b.len() {
                return Err("unterminated string".into());
            }
            let t = std::str::from_utf8(&b[i + 1..j])?;
            toks.push(Tok::Str(t.to_owned()));
            i = j + 1;
        } else {
            let mut j = i;
            while j < b.len() && !b[j].is_ascii_whitespace() && !b"{ } [ ] , \"".contains(&b[j]) {
                j += 1;
            }
            let w = std::str::from_utf8(&b[i..j])?;
            if let Ok(n) = w.parse::<f64>() {
                toks.push(Tok::Num(n));
            } else if w
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '+')
                && !w.is_empty()
            {
                // `.875` parses as a number above; anything else is a name.
                toks.push(Tok::Ident(w.to_owned()));
            } else {
                return Err(format!("bad word {w:?}").into());
            }
            i = j;
        }
    }
    Ok(toks)
}

struct Words {
    toks: Vec<Tok>,
    at: usize,
}

impl Words {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn next(&mut self) -> Result<Tok, Error> {
        let t = self.peek().cloned().ok_or("cut short")?;
        self.at += 1;
        Ok(t)
    }
    fn node(&mut self) -> Result<Node, Error> {
        let kind = match self.next()? {
            Tok::Ident(k) => k,
            t => return Err(format!("want kind, have {t:?}").into()),
        };
        let mut name = String::new();
        match self.peek() {
            Some(Tok::Str(_)) | Some(Tok::Ident(_)) => {
                if let Tok::Str(s) | Tok::Ident(s) = self.next()? {
                    name = s;
                }
                if !matches!(self.peek(), Some(Tok::LBrace)) {
                    return Err(format!("want '{{' for {kind} {name}").into());
                }
                self.next()?;
            }
            Some(Tok::LBrace) => {
                self.next()?;
            }
            t => return Err(format!("want name or '{{' for {kind}, have {t:?}").into()),
        }
        let mut fields = Vec::new();
        loop {
            match self.next()? {
                Tok::RBrace => break,
                Tok::Ident(f) => {
                    fields.push((f, self.value()?));
                }
                t => return Err(format!("want field of {kind} {name}, have {t:?}").into()),
            }
        }
        Ok(Node { kind, name, fields })
    }
    fn value(&mut self) -> Result<Value, Error> {
        match self.next()? {
            Tok::Str(s) => Ok(Value::Str(s)),
            Tok::Num(n) => {
                let mut v = vec![n];
                while let Some(Tok::Num(_)) = self.peek() {
                    if let Tok::Num(m) = self.next()? {
                        v.push(m);
                    }
                }
                Ok(if v.len() == 1 {
                    Value::Num(v[0])
                } else {
                    Value::Nums(v)
                })
            }
            Tok::Ident(id) => {
                // An embedded node without a name, or a bare name.
                let is_node = match self.peek() {
                    Some(Tok::LBrace) => true,
                    Some(Tok::Str(_)) | Some(Tok::Ident(_)) => {
                        matches!(self.toks.get(self.at + 1), Some(Tok::LBrace))
                    }
                    _ => false,
                };
                if is_node {
                    self.at -= 1;
                    Ok(Value::Nodes(vec![self.node()?]))
                } else {
                    Ok(Value::Ident(id))
                }
            }
            Tok::LBracket => {
                let mut nums = Vec::new();
                let mut strs = Vec::new();
                let mut idents = Vec::new();
                let mut nodes = Vec::new();
                loop {
                    match self.next()? {
                        Tok::RBracket => break,
                        Tok::Comma => {}
                        Tok::Num(n) => {
                            if !strs.is_empty() || !nodes.is_empty() || !idents.is_empty() {
                                return Err("mixed list".into());
                            }
                            nums.push(n);
                        }
                        Tok::Str(s) => {
                            if !nums.is_empty() || !nodes.is_empty() || !idents.is_empty() {
                                return Err("mixed list".into());
                            }
                            strs.push(s);
                        }
                        Tok::Ident(id) => {
                            // A node, or a bare name.
                            let is_node = match self.peek() {
                                Some(Tok::LBrace) => true,
                                Some(Tok::Str(_)) | Some(Tok::Ident(_)) => {
                                    matches!(self.toks.get(self.at + 1), Some(Tok::LBrace))
                                }
                                _ => false,
                            };
                            if is_node {
                                if !nums.is_empty() || !strs.is_empty() || !idents.is_empty() {
                                    return Err("mixed list".into());
                                }
                                self.at -= 1;
                                nodes.push(self.node()?);
                            } else {
                                if !nums.is_empty() || !strs.is_empty() || !nodes.is_empty() {
                                    return Err("mixed list".into());
                                }
                                idents.push(id);
                            }
                        }
                        t => return Err(format!("bad list item {t:?}").into()),
                    }
                }
                if !nodes.is_empty() {
                    Ok(Value::Nodes(nodes))
                } else if !strs.is_empty() {
                    Ok(Value::Strs(strs))
                } else if !idents.is_empty() {
                    Ok(Value::Idents(idents))
                } else {
                    Ok(Value::Nums(nums))
                }
            }
            t => Err(format!("bad value {t:?}").into()),
        }
    }
}

/// Reads a whole `.nma` file; every byte must be understood.
pub fn read_text(data: &[u8]) -> Result<Node, Error> {
    let text = std::str::from_utf8(data)?;
    let mut w = Words {
        toks: tokenize(text)?,
        at: 0,
    };
    let root = w.node()?;
    if w.at != w.toks.len() {
        return Err("trailing words".into());
    }
    if root.kind != "Scene" {
        return Err(format!("want Scene, have {}", root.kind).into());
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_lists() {
        let s = r#"Scene {
          cameras [
            Camera c {
              near 1
              transform "t"
              position 0 0 0
              order xyz
            }
          ] // cameras
        }"#;
        let r = read_text(s.as_bytes()).unwrap();
        assert_eq!(r.kind, "Scene");
        let c = &r.nodes("cameras")[0];
        assert_eq!((c.kind.as_str(), c.name.as_str()), ("Camera", "c"));
        assert_eq!(c.num("near", 0), 1.0);
        assert_eq!(c.text("transform"), "t");
        assert_eq!(c.nums("position"), &[0.0, 0.0, 0.0]);
        assert_eq!(c.ident("order"), "xyz");
    }

    #[test]
    fn rejects_leftovers() {
        assert!(read_text(b"Scene { } trailing").is_err());
        assert!(read_text(b"Scene { ").is_err());
        assert!(read_text(b"Box { }").is_err());
    }
}
