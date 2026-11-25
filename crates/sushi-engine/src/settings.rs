//! `Sushi2.ini`: the scene, the lens and the chapters.

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Frame script, menu text and camera path.
    pub script: String,
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// File name without its extension.
    pub scene: String,
    pub near: f32,
    pub far: f32,
    /// Horizontal, in degrees.
    pub fov: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    /// Seconds by which the music's loop starts before the intro ends.
    pub overlap: f32,
    pub chapters: Vec<Entry>,
}

/// Words, where quotes keep several together.
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

/// `NORMAL` becomes `Normal`.
fn title(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        match out.is_empty() {
            true => out.extend(c.to_uppercase()),
            false => out.extend(c.to_lowercase()),
        }
    }
    out
}

impl Settings {
    pub fn parse(text: &str) -> Result<Self, String> {
        let lines: Vec<(String, String)> = text
            .lines()
            .filter_map(|l| l.split("//").next()?.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        let get = |key: &str| lines.iter().find(|l| l.0 == key).map(|l| l.1.as_str());
        let number = |key: &str| -> Result<f32, String> {
            let v = get(key).ok_or_else(|| format!("Sushi2.ini lacks {key}"))?;
            v.parse().map_err(|_| format!("Sushi2.ini: bad {key}"))
        };
        let stem = |file: &str| file.rsplit_once('.').map_or(file, |f| f.0).to_string();
        let mut chapters = Vec::new();
        for (_, v) in lines.iter().filter(|l| l.0 == "script") {
            let w = words(v);
            let [script, name, path] = w.as_slice() else {
                return Err(format!("Sushi2.ini: bad script line {v:?}"));
            };
            chapters.push(Entry {
                script: script.clone(),
                title: title(name),
                path: path.clone(),
            });
        }
        if chapters.is_empty() {
            return Err("Sushi2.ini names no scripts".into());
        }
        Ok(Self {
            scene: stem(get("scene").ok_or("Sushi2.ini names no scene")?),
            near: number("near")?,
            far: number("far")?,
            fov: number("fov")?,
            fade_in: number("fadeInTime")?,
            fade_out: number("fadeOutTime")?,
            overlap: number("audioIntroOverlap").unwrap_or(0.0),
            chapters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lines() {
        let s = Settings::parse(
            "// far = 1\nnear = 60\nfar = 4000.0 // far\nfov = 50.0\nfadeInTime = 1.0\n\
             fadeOutTime = 2.0\nscene = A.ati\nscript = N.sds  \"EDGE WIDTH\"  A.pth\n//script = x y z",
        )
        .unwrap();
        assert_eq!((s.near, s.far, s.fade_out), (60.0, 4000.0, 2.0));
        assert_eq!((s.scene.as_str(), s.chapters.len()), ("A", 1));
        assert_eq!(s.chapters[0].title, "Edge width");
        assert!(Settings::parse("near = 1").is_err());
    }
}
