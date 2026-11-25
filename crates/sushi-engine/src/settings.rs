//! `Sushi2.ini`: scenes, lens and chapters.

/// One camera path of a chapter and what is drawn along it.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Frame script, and the call that draws a frame.
    pub script: String,
    pub call: String,
    pub path: String,
    /// List of objects, if not the first scene's.
    pub objects: Option<String>,
    /// Scene whose lights shine instead of the current one's.
    pub lights: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub title: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// File names without extension.
    pub scenes: Vec<String>,
    pub quads: String,
    pub near: f32,
    pub far: f32,
    /// Horizontal, in degrees.
    pub fov: f32,
    /// Width of the film in millimeters and largest blur in pixels.
    pub film: f32,
    pub blur: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub overlap: f32,
    pub chapters: Vec<Entry>,
}

/// Words, where quotes and parentheses keep several together. Quoted ones start with `"`.
fn words(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut depth) = (false, 0);
    for c in line.chars() {
        match c {
            '"' if depth == 0 => {
                if quoted {
                    out.push(std::mem::take(&mut word));
                } else {
                    word.push('"');
                }
                quoted = !quoted;
            }
            c if quoted => word.push(c),
            c if c.is_whitespace() && depth == 0 => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c => {
                depth += i32::from(c == '(') - i32::from(c == ')');
                word.push(c);
            }
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// `NORMAL` becomes `Normal`.
fn title(text: &str) -> String {
    let text = text.trim();
    if text.chars().any(char::is_lowercase) {
        return text.into();
    }
    let mut out = String::new();
    for c in text.chars() {
        match out.is_empty() {
            true => out.extend(c.to_uppercase()),
            false => out.extend(c.to_lowercase()),
        }
    }
    out
}

fn stem(file: &str) -> String {
    file.rsplit_once('.').map_or(file, |f| f.0).to_string()
}

/// `Name("value")`.
fn option<'a>(word: &'a str, name: &str) -> Option<&'a str> {
    let rest = word.strip_prefix(name)?.trim().strip_prefix('(')?;
    Some(rest.strip_suffix(')')?.trim().trim_matches('"'))
}

impl Settings {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut plain = String::new();
        let mut rest = text;
        while let Some(i) = rest.find("/*") {
            plain.push_str(&rest[..i]);
            rest = rest[i..].split_once("*/").map_or("", |r| r.1);
        }
        plain.push_str(rest);
        let lines: Vec<(String, String)> = plain
            .lines()
            .filter_map(|l| l.split("//").next()?.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        let get = |key: &str| lines.iter().find(|l| l.0 == key).map(|l| l.1.as_str());
        let number = |key: &str, or: f32| -> Result<f32, String> {
            let Some(v) = get(key) else { return Ok(or) };
            let v = v.split_whitespace().next().unwrap_or("");
            v.parse().map_err(|_| format!("Sushi2.ini: bad {key}"))
        };
        let mut chapters: Vec<Entry> = Vec::new();
        for (k, v) in &lines {
            let first = k == "script";
            if !first && k != "scriptAppend" {
                continue;
            }
            let w = words(v);
            let bad = || format!("Sushi2.ini: bad script line {v:?}");
            let mut part = Part {
                script: w.first().ok_or_else(bad)?.clone(),
                call: w.get(1).ok_or_else(bad)?.trim_start_matches('"').into(),
                path: String::new(),
                objects: None,
                lights: None,
            };
            if !part.call.contains('(') {
                part.call.push_str("()");
            }
            let mut name = String::new();
            for w in &w[2..] {
                if let Some(t) = w.strip_prefix('"') {
                    name = title(t);
                } else if let Some(o) = option(w, "Objects") {
                    part.objects = Some(stem(o));
                } else if let Some(l) = option(w, "Lights") {
                    part.lights = Some(stem(l));
                } else if w.ends_with(".pth") {
                    part.path = w.clone();
                } else if option(w, "POS").is_none() {
                    return Err(bad());
                }
            }
            match chapters.last_mut() {
                Some(c) if !first => c.parts.push(part),
                _ => chapters.push(Entry {
                    title: name,
                    parts: vec![part],
                }),
            }
        }
        if chapters.is_empty() {
            return Err("Sushi2.ini names no scripts".into());
        }
        let scenes: Vec<String> = lines
            .iter()
            .filter(|l| l.0 == "scene")
            .map(|l| stem(&l.1))
            .collect();
        if scenes.is_empty() {
            return Err("Sushi2.ini names no scene".into());
        }
        Ok(Self {
            scenes,
            quads: stem(get("screenQuads").unwrap_or("Quad.ati")),
            near: number("near", 1.0)?,
            far: number("far", 100_000.0)?,
            fov: number("fov", 50.0)?,
            film: number("filmFormat", 35.0)?,
            blur: number("maxDOFBlurRadius", 8.0)?,
            fade_in: number("fadeInTime", 0.0)?,
            fade_out: number("fadeOutTime", 0.0)?,
            overlap: number("audioIntroOverlap", 0.0)?,
            chapters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chapters_of_several_parts() {
        let s = Settings::parse(
            "// far = 1\nnear = 10.0 // x\nscene = A.ati\nscene = B.ati\nscreenQuads = Quad.ati\n\
             script = N.sds \"Draw(TRUE, 1)\" \"EDGE WIDTH\" A.pth POS(0)\n/* script = x\n*/\n\
             scriptAppend = M.sds DrawM B.pth Lights(\"L.ati\") Objects(\"O.ini\")",
        )
        .unwrap();
        assert_eq!((s.near, s.far, s.scenes.len()), (10.0, 100_000.0, 2));
        let c = &s.chapters[0];
        assert_eq!((c.title.as_str(), c.parts.len()), ("Edge width", 2));
        assert_eq!(c.parts[0].call, "Draw(TRUE, 1)");
        assert_eq!(
            (c.parts[1].call.as_str(), c.parts[1].path.as_str()),
            ("DrawM()", "B.pth")
        );
        assert_eq!(c.parts[1].lights.as_deref(), Some("L"));
        assert_eq!(c.parts[1].objects.as_deref(), Some("O"));
        assert!(Settings::parse("near = 1").is_err());
    }
}
