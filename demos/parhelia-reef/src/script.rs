//! The demo's two scripts, `ReefDemoScene.s` (shaders) and `.w` (world). Each byte is stored
//! plus a byte of a repeating key; the keys are messages the program also prints. Decoded,
//! they are trees of `Name { value }` and `Name { children }`.
use crate::Error;

/// The world's key and the shaders' key.
pub const WORLD_KEY: &[u8] = b"*** TDRG_Audio::Mix() - Software Mix ENABLED";
pub const SHADER_KEY: &[u8] = b"*** TDRG_Input::QueryDevice() - m_poInputDevice FAILED";

pub fn decode(d: &[u8], key: &[u8]) -> String {
    d.iter()
        .enumerate()
        .map(|(i, &b)| b.wrapping_sub(key[i % key.len()]) as char)
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct Item {
    pub name: String,
    /// The words of `Name { words }`, if it is a value.
    pub value: Option<Vec<String>>,
    pub children: Vec<Item>,
}

impl Item {
    pub fn get(&self, name: &str) -> Option<&Item> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Item> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// The words of a value child, joined; "" if missing.
    pub fn text(&self, name: &str) -> String {
        self.get(name)
            .and_then(|c| c.value.as_ref())
            .map(|w| w.join(" "))
            .unwrap_or_default()
    }
    pub fn nums(&self, name: &str) -> Vec<f32> {
        self.get(name)
            .and_then(|c| c.value.as_ref())
            .map(|w| w.iter().filter_map(|x| x.parse().ok()).collect())
            .unwrap_or_default()
    }
    pub fn num(&self, name: &str) -> f32 {
        self.nums(name).first().copied().unwrap_or(0.0)
    }
}

/// Parses `head { ... }` from decoded text: the first line is the version.
pub fn parse(text: &str) -> Result<Item, Error> {
    let mut lines = text.lines();
    let version = lines.next().unwrap_or("").trim().to_string();
    let mut stack: Vec<Item> = vec![Item {
        name: version,
        ..Default::default()
    }];
    let mut pending: Option<String> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "{" {
            let name = pending.take().unwrap_or_default();
            stack.push(Item {
                name,
                ..Default::default()
            });
        } else if line == "}" {
            let done = stack.pop().ok_or("unbalanced }")?;
            match stack.last_mut() {
                Some(top) => top.children.push(done),
                None => return Ok(done),
            }
        } else if let Some((name, rest)) = line.split_once('{') {
            let Some(inner) = rest.rsplit_once('}') else {
                // A line such as "Name {" opening children.
                stack.push(Item {
                    name: name.trim().to_string(),
                    ..Default::default()
                });
                continue;
            };
            let words: Vec<String> = inner.0.split_whitespace().map(String::from).collect();
            let top = stack.last_mut().ok_or("no parent")?;
            top.children.push(Item {
                name: name.trim().to_string(),
                value: Some(words),
                ..Default::default()
            });
        } else {
            if let Some(p) = pending.take() {
                // A bare word on its own line followed by another: keep the first as empty.
                stack.last_mut().ok_or("no parent")?.children.push(Item {
                    name: p,
                    ..Default::default()
                });
            }
            pending = Some(line.to_string());
        }
        if stack.len() == 1 && stack[0].children.len() == 1 && stack[0].children[0].name.is_empty()
        {
            break;
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().ok_or("unbalanced")?;
        stack.last_mut().ok_or("unbalanced")?.children.push(done);
    }
    let mut root = stack.pop().ok_or("empty")?;
    // The tree is the one unnamed child of the version line.
    if root.children.len() == 1 && root.children[0].name.is_empty() {
        let inner = root.children.remove(0);
        root.children = inner.children;
    }
    Ok(root)
}
