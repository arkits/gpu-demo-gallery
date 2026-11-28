//! `bin/args.txt` (`0x4083b0`): window width and height, colour/depth/stencil bits,
//! samples, fullscreen, then the scene files; the first is the scene, the rest merge in.
//! Tokens part at white space and commas; `//` starts a comment to the end of the line
//! (`0x468420`).

pub struct Args {
    pub window: (u32, u32),
    pub scenes: Vec<String>,
}

pub fn tokens(s: &str) -> Vec<String> {
    let mut out = vec![];
    for line in s.lines() {
        let line = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        for t in line.split(|c: char| c.is_whitespace() || c == ',') {
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
    }
    out
}

pub fn parse(s: &str) -> Option<Args> {
    let t = tokens(s);
    let w = t.first()?.parse().ok()?;
    let h = t.get(1)?.parse().ok()?;
    // Bits (3), samples, fullscreen.
    let scenes = t.iter().skip(7).cloned().collect();
    Some(Args { window: (w, h), scenes })
}
