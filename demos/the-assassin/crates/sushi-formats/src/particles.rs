//! `.spg` particle systems (reader 0x492d70): text, then for each system a shader header,
//! its textures, and the curves that its particles follow.
use crate::reader::*;
use crate::shader_header::*;
use crate::texture::*;

/// A line's words, and the lines up to its `End`.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub words: Vec<String>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|c| c.words.first().is_some_and(|w| w == name))
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children
            .iter()
            .filter(move |c| c.words.first().is_some_and(|w| w == name))
    }
    /// Numbers among the words.
    pub fn numbers(&self) -> Vec<f32> {
        self.words.iter().filter_map(|w| w.parse().ok()).collect()
    }
    pub fn number(&self, name: &str) -> Option<f32> {
        self.child(name)?.numbers().first().copied()
    }
    pub fn vector(&self, name: &str) -> Option<[f32; 3]> {
        let n = self.child(name)?.numbers();
        Some([*n.first()?, *n.get(1)?, *n.get(2)?])
    }
    pub fn is(&self, name: &str) -> bool {
        self.child(name)
            .is_some_and(|c| c.words.get(1).is_some_and(|w| w == "TRUE"))
    }
}

#[derive(Debug, Clone)]
pub struct System {
    pub text: Node,
    pub header: ShaderHeader,
    pub textures: Vec<Texture>,
    /// `curves[row][step]`, four numbers each.
    pub curves: Vec<Vec<[f32; 4]>>,
}

fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, part) in line.split('"').enumerate() {
        match i % 2 {
            0 => out.extend(part.split_whitespace().map(String::from)),
            _ => out.push(part.to_string()),
        }
    }
    out
}

fn nodes(text: &str) -> Vec<Node> {
    let lines: Vec<Vec<String>> = text
        .lines()
        .map(words)
        .filter(|w| !w.is_empty() && w[0] != "\0")
        .collect();
    fn block(lines: &[Vec<String>], at: &mut usize) -> Vec<Node> {
        let mut out = Vec::new();
        while *at < lines.len() {
            let w = &lines[*at];
            *at += 1;
            if w[0].starts_with("End") {
                return out;
            }
            let ahead = |end: &str| lines[*at..].iter().any(|l| l[0] == end);
            let opens = match w[0].as_str() {
                "ParticleSystem" | "Emitter" | "Action" | "Transform" | "Shape"
                | "KeyFrameChannel" | "ShaderParameters" | "EditableParam" => {
                    // Parameters that do not change take one line.
                    w[0] != "EditableParam" || w.last().is_some_and(|l| l == "KeyFrame")
                }
                // Curves of their own have no numbers on the first line.
                _ => w.get(1).is_some_and(|p| p == "Param") && w.len() == 3 && ahead("EndParam"),
            };
            let children = if opens { block(lines, at) } else { Vec::new() };
            out.push(Node {
                words: w.clone(),
                children,
            });
        }
        out
    }
    block(&lines, &mut 0)
}

pub fn read_particles(data: &[u8]) -> Result<Vec<System>> {
    let mut r = Reader::new(data);
    r.magic(0x17, "SushiParticles01.03.00")?;
    let n = r.count()?;
    let text: String = r.bytes(n)?.iter().map(|&c| c as char).collect();
    let mut systems = Vec::new();
    for text in nodes(&text) {
        let header = read_shader_header(&mut r)?;
        systems.push(System {
            text,
            header,
            textures: Vec::new(),
            curves: Vec::new(),
        });
    }
    for s in &mut systems {
        for _ in 0..s.header.textures.len() {
            let raw = r.bytes(TEXTURE_RECORD)?;
            s.textures.push(read_texels(&mut r, raw)?);
        }
    }
    for s in &mut systems {
        let size = s
            .text
            .child("KeyFrameTexSize")
            .map(Node::numbers)
            .unwrap_or_default();
        let [w, h] = size.as_slice() else {
            return r.bad("no KeyFrameTexSize");
        };
        for _ in 0..*h as usize {
            let row = r.f32s(*w as usize * 4)?;
            s.curves.push(
                row.chunks_exact(4)
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect(),
            );
        }
    }
    r.finish()?;
    Ok(systems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_becomes_a_tree() {
        let n = nodes(
            "ParticleSystem Billboard \"A b\"\n Enabled TRUE\n Emitter Shape \"E\"\n  EmissionRate Param KeyFrame \n\
             KeyFrameChannel\n 0.0 1.5\n 1.0 2.0\n EndKeyFrameChannel\n EndParam\n Size Param KeyFrame 1 0.0 1.0\n\
             EndEmitter\nEndParticleSystem\n",
        );
        let e = n[0].child("Emitter").unwrap();
        assert!(n[0].is("Enabled") && n[0].words[2] == "A b");
        let keys = &e.child("EmissionRate").unwrap().children[0].children;
        assert_eq!(keys[0].numbers(), [0.0, 1.5]);
        assert_eq!(e.child("Size").unwrap().numbers(), [1.0, 0.0, 1.0]);
    }
}
