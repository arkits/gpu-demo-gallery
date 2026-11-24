//! Frame scripts (`Scripts/*.sds`): parsed as the engine does (0x404xxx).

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    CreateColor {
        name: String,
        width: u32,
        height: u32,
    },
    /// Color buffer name; `None` is the back buffer.
    Target(Option<String>),
    /// Left, bottom, right, top as fractions, bottom up.
    Viewport([f32; 4]),
    Scissor([f32; 4]),
    Clear {
        depth: bool,
        color: Option<[f32; 4]>,
    },
    Objects {
        property: String,
        opaque: bool,
        transparent: bool,
        receive: bool,
        other: bool,
    },
    Scene {
        property: String,
        opaque: bool,
        transparent: bool,
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
            let size = |i: usize| a.get(i).and_then(|v| v.parse().ok());
            let (Some(width), Some(height)) = (size(0), size(1)) else {
                return Err(format!("{name} needs a size"));
            };
            // Depth buffers come with their color buffers here.
            if name == "CreateColorBuffer" {
                out.push(Command::CreateColor {
                    name: buffer,
                    width,
                    height,
                });
            }
            continue;
        }
        if name == "Clear" {
            let (mut depth, mut color) = (false, None);
            while let Some(Tok::Word(w)) =
                t.next_if(|t| matches!(t, Tok::Word(w) if w == "Z" || w == "Color"))
            {
                let a = arguments(&mut t)?;
                if w == "Z" {
                    depth = true;
                } else {
                    color = Some(floats("Clear Color", &a)?);
                }
            }
            out.push(Command::Clear { depth, color });
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
                let target = a.get(1).ok_or("SetRenderTarget needs a buffer")?;
                Command::Target((target != "backBuffer").then(|| target.clone()))
            }
            "SetViewport" => Command::Viewport(floats(&name, &a)?),
            "SetScissor" => Command::Scissor(floats(&name, &a)?),
            // The projection always follows the window here.
            "SetProjectionBasedOnViewport" => continue,
            "DrawObjects" => Command::Objects {
                property: first()?,
                opaque: has("Opaque"),
                transparent: has("Trans"),
                receive: has("Receive"),
                other: has("NotReceive"),
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
                width: 512,
                height: 384
            }
        );
        assert_eq!(s[1], Command::Target(Some("c".into())));
        assert_eq!(
            s[2],
            Command::Clear {
                depth: true,
                color: Some([0.0, 0.5, 0.0, 1.0])
            }
        );
        assert!(matches!(
            &s[3],
            Command::Objects {
                opaque: true,
                transparent: false,
                receive: true,
                ..
            }
        ));
        assert_eq!(s[4], Command::Target(None));
        assert!(
            matches!(&s[5], Command::Text { text, background: true, .. } if text == " a \n b ")
        );
        assert!(parse("Nonsense(1)").is_err());
    }
}
