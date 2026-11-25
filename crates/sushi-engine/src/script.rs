//! Frame scripts (`Scripts/*.sds`): run by 0x404810 in the engine.

/// What a drawing lands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surface {
    BackBuffer,
    Named(String),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// No size: as large as the back buffer.
    CreateColor {
        name: String,
        size: Option<[u32; 2]>,
        /// 16 bits per channel instead of 8.
        deep: bool,
    },
    CreateDepth {
        name: String,
        size: Option<[u32; 2]>,
    },
    /// Number, color buffer, and for number 0 a depth buffer.
    Target(usize, Surface, Surface),
    /// Left, bottom, right, top as fractions, bottom up.
    Viewport([f32; 4]),
    Scissor([f32; 4]),
    /// The shape of the picture follows the viewport.
    Projection,
    Clear {
        depth: Option<f32>,
        stencil: Option<u32>,
        color: Option<[f32; 4]>,
    },
    Objects {
        property: String,
        opaque: bool,
        transparent: bool,
        receive: bool,
        other: bool,
        /// Objects that cast shadows, each with its strongest light.
        casting: bool,
    },
    Scene {
        property: String,
        opaque: bool,
        transparent: bool,
    },
    /// A rectangle of the viewport, and of the textures shown in it.
    Quad {
        name: String,
        property: String,
        position: [f32; 4],
        texture: [f32; 4],
    },
    Reflections(String),
    Outline([f32; 3]),
    TextSize(f32),
    TextColor([f32; 4]),
    /// Center, baseline and text, over a dark background if set.
    Text {
        x: f32,
        y: f32,
        text: String,
        background: bool,
    },
}

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Text(String),
    Open,
    Close,
}

fn strip_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('/') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with("//") {
            rest = rest.find('\n').map_or("", |j| &rest[j..]);
        } else if rest.starts_with("/*") {
            rest = rest.find("*/").map_or("", |j| &rest[j + 2..]);
            out.push(' ');
        } else {
            out.push('/');
            rest = &rest[1..];
        }
    }
    out + rest
}

fn tokens(text: &str) -> Result<Vec<Tok>, String> {
    let text = strip_comments(text);
    let mut out = Vec::new();
    let mut c = text.chars().peekable();
    while let Some(&ch) = c.peek() {
        match ch {
            '(' => out.push(Tok::Open),
            ')' => out.push(Tok::Close),
            '"' => {
                c.next();
                let s: String = c.by_ref().take_while(|&ch| ch != '"').collect();
                out.push(Tok::Text(s.replace("\\n", "\n")));
                continue;
            }
            ch if ch.is_whitespace() || ch == ',' => {}
            _ => {
                let mut w = String::new();
                while let Some(&ch) = c.peek() {
                    if ch.is_whitespace() || "(),\"".contains(ch) {
                        break;
                    }
                    w.push(ch);
                    c.next();
                }
                out.push(Tok::Word(w));
                continue;
            }
        }
        c.next();
    }
    Ok(out)
}

/// The arguments of one `name(...)`, as plain strings.
fn arguments(t: &mut std::iter::Peekable<std::vec::IntoIter<Tok>>) -> Result<Vec<String>, String> {
    if t.next_if_eq(&Tok::Open).is_none() {
        return Ok(Vec::new());
    }
    let mut args = Vec::new();
    loop {
        match t.next() {
            Some(Tok::Close) => return Ok(args),
            Some(Tok::Word(w) | Tok::Text(w)) => args.push(w),
            other => return Err(format!("unexpected {other:?}")),
        }
    }
}

fn floats<const N: usize>(name: &str, args: &[String]) -> Result<[f32; N], String> {
    let v: Vec<f32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    <[f32; N]>::try_from(v).map_err(|_| format!("{name} needs {N} numbers, has {args:?}"))
}

pub fn parse(text: &str) -> Result<Vec<Command>, String> {
    let mut t = tokens(text)?.into_iter().peekable();
    let mut out = Vec::new();
    while let Some(tok) = t.next() {
        let Tok::Word(name) = tok else {
            return Err(format!("expected a command, found {tok:?}"));
        };
        if name == "CreateColorBuffer" || name == "CreateZBuffer" {
            let Some(Tok::Word(buffer)) = t.next() else {
                return Err(format!("{name} needs a name"));
            };
            let a = arguments(&mut t)?;
            let [w, h, format] = a.as_slice() else {
                return Err(format!("{name} needs a size and a format"));
            };
            let size = match (w.parse(), h.parse()) {
                (Ok(w), Ok(h)) => Some([w, h]),
                _ if w == "backBuffer" && h == "backBuffer" => None,
                _ => return Err(format!("{name}: bad size {a:?}")),
            };
            let deep = match (name.as_str(), format.as_str()) {
                ("CreateColorBuffer", "16161616") => true,
                ("CreateColorBuffer", "RGBA") | ("CreateZBuffer", "24") => false,
                _ => return Err(format!("{name}: unknown format {format}")),
            };
            out.push(match name.as_str() {
                "CreateColorBuffer" => Command::CreateColor {
                    name: buffer,
                    size,
                    deep,
                },
                _ => Command::CreateDepth { name: buffer, size },
            });
            continue;
        }
        if name == "Clear" {
            let (mut depth, mut stencil, mut color) = (None, None, None);
            while let Some(Tok::Word(w)) = t.next_if(
                |t| matches!(t, Tok::Word(w) if ["Z", "Color", "Stencil"].contains(&w.as_str())),
            ) {
                let a = arguments(&mut t)?;
                match w.as_str() {
                    "Z" => depth = Some(floats::<1>("Clear Z", &a)?[0]),
                    "Stencil" => stencil = Some(floats::<1>("Clear Stencil", &a)?[0] as u32),
                    _ => color = Some(floats("Clear Color", &a)?),
                }
            }
            out.push(Command::Clear {
                depth,
                stencil,
                color,
            });
            continue;
        }
        if name == "DrawQuad" {
            let a = arguments(&mut t)?;
            let [quad, property] = a.as_slice() else {
                return Err(format!("DrawQuad: bad arguments {a:?}"));
            };
            let (mut position, mut texture) = ([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0]);
            while let Some(Tok::Word(w)) = t.next_if(
                |t| matches!(t, Tok::Word(w) if w == "Position" || w.starts_with("TexCoord")),
            ) {
                let v = floats(&w, &arguments(&mut t)?)?;
                match w.as_str() {
                    "Position" => position = v,
                    _ => texture = v,
                }
            }
            out.push(Command::Quad {
                name: quad.clone(),
                property: property.clone(),
                position,
                texture,
            });
            continue;
        }
        let a = arguments(&mut t)?;
        let has = |w: &str| a.iter().any(|x| x == w);
        let first = || {
            a.first()
                .cloned()
                .ok_or(format!("{name} needs an argument"))
        };
        out.push(match name.as_str() {
            "SetRenderTarget" => {
                let surface = |i: usize| match a.get(i).map(String::as_str) {
                    Some("backBuffer") => Surface::BackBuffer,
                    Some("None") | None => Surface::None,
                    Some(n) => Surface::Named(n.into()),
                };
                let number = first()?.parse().ok().filter(|n| *n < 4);
                let number = number.ok_or("SetRenderTarget needs a number")?;
                Command::Target(number, surface(1), surface(2))
            }
            "SetViewport" => Command::Viewport(floats(&name, &a)?),
            "SetScissor" => Command::Scissor(floats(&name, &a)?),
            "SetProjectionBasedOnViewport" => Command::Projection,
            "DrawObjects" => Command::Objects {
                property: first()?,
                opaque: has("Opaque"),
                transparent: has("Trans"),
                receive: has("Receive"),
                other: has("NotReceive"),
                casting: has("Stencil"),
            },
            "DrawScene" => Command::Scene {
                property: first()?,
                opaque: has("Opaque"),
                transparent: has("Trans"),
            },
            "DrawReflections" => Command::Reflections(first()?),
            "DrawOutlineAroundViewport" => Command::Outline(floats(&name, &a)?),
            "SetTextSize" => Command::TextSize(floats::<1>(&name, &a)?[0]),
            "SetTextColor" => Command::TextColor(floats(&name, &a)?),
            "DrawTextCenteredX" => {
                let [x, y] = floats(&name, &a[..a.len().min(2)])?;
                let text = a.get(2).cloned().ok_or("text missing")?;
                let background = has("background");
                Command::Text {
                    x,
                    y,
                    text,
                    background,
                }
            }
            other => return Err(format!("unknown command {other}")),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        let s = parse(
            "CreateColorBuffer c(512, 384, RGBA)\nCreateZBuffer z(512, 384, 24)\n\
             SetRenderTarget (0, c, z) // note\n/* DrawQuad(\"x\") */\nClear Z(1.0) Color(0.0, 0.5, 0.0, 1.0)\n\
             DrawObjects (\"Normal\", Opaque, Receive, NotReceive)\nSetRenderTarget(0, backBuffer, backBuffer)\n\
             DrawTextCenteredX (0.25, 0.95, \" a \\n b \", background)",
        )
        .unwrap();
        assert_eq!(
            s[0],
            Command::CreateColor {
                name: "c".into(),
                size: Some([512, 384]),
                deep: false
            }
        );
        let z = Surface::Named("z".into());
        assert_eq!(s[2], Command::Target(0, Surface::Named("c".into()), z));
        assert_eq!(
            s[3],
            Command::Clear {
                depth: Some(1.0),
                stencil: None,
                color: Some([0.0, 0.5, 0.0, 1.0])
            }
        );
        assert!(matches!(
            &s[4],
            Command::Objects {
                opaque: true,
                transparent: false,
                receive: true,
                casting: false,
                ..
            }
        ));
        let back = Surface::BackBuffer;
        assert_eq!(s[5], Command::Target(0, back.clone(), back));
        assert!(
            matches!(&s[6], Command::Text { text, background: true, .. } if text == " a \n b ")
        );
        let q =
            parse("DrawQuad(\"E\", \"P\") Position(0.5, 0.0, 1.0, 1.0)\nSetRenderTarget (2, None)")
                .unwrap();
        assert!(matches!(&q[0], Command::Quad { position, texture, .. }
            if *position == [0.5, 0.0, 1.0, 1.0] && *texture == [0.0, 0.0, 1.0, 1.0]));
        assert_eq!(q[1], Command::Target(2, Surface::None, Surface::None));
        assert!(parse("Nonsense(1)").is_err());
    }
}
