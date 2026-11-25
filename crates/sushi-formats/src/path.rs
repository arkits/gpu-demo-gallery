use crate::reader::{Error, Result};

/// Camera fly path (`.pth`, text).
#[derive(Debug, Clone)]
pub struct FlyPath {
    pub step: f32,
    pub frames: Vec<PathFrame>,
}

#[derive(Debug, Clone, Copy)]
pub struct PathFrame {
    pub pos: [f32; 3],
    /// -1 throughout the demo: use the default field of view (inferred).
    pub fov: f32,
    /// 1 on the last sample of a shot; the next sample starts another.
    pub flag: i32,
    /// Distance in focus, f-stop, and focal length in millimeters.
    pub focus: [f32; 3],
    pub quat: [f32; 4],
}

pub fn read_path(text: &str) -> Result<FlyPath> {
    let eof = || Error::Bad {
        at: 0,
        what: "truncated path".into(),
    };
    if text.lines().next().map(str::trim) != Some("v1.0") {
        return Err(Error::Bad {
            at: 1,
            what: "expected v1.0".into(),
        });
    }
    let body = text.split_once('\n').ok_or_else(eof)?.1;
    let mut rows = body
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let mut row = |want: &[usize]| -> Result<Option<Vec<f32>>> {
        let Some((i, l)) = rows.next() else {
            return Ok(None);
        };
        let words = l.split([' ', '\t', ',']).filter(|w| !w.is_empty());
        let v: std::result::Result<Vec<f32>, _> = words.map(str::parse).collect();
        match v {
            Ok(v) if want.contains(&v.len()) => Ok(Some(v)),
            _ => Err(Error::Bad {
                at: i + 2,
                what: format!("expected {want:?} numbers: {l:?}"),
            }),
        }
    };
    let step = row(&[1])?.ok_or_else(eof)?[0];
    let mut frames = Vec::new();
    while let Some(a) = row(&[5, 8])? {
        let q = row(&[4, 5])?.ok_or_else(eof)?;
        frames.push(PathFrame {
            pos: [a[0], a[1], a[2]],
            fov: a[3],
            flag: if a[4] != 0.0 {
                a[4] as i32
            } else {
                q.get(4).copied().unwrap_or_default() as i32
            },
            focus: [5, 6, 7].map(|i| a.get(i).copied().unwrap_or(0.0)),
            quat: [q[0], q[1], q[2], q[3]],
        });
    }
    Ok(FlyPath { step, frames })
}

impl FlyPath {
    /// Samples flagged as the last of their shot take no time.
    pub fn length(&self) -> f32 {
        self.frames.iter().filter(|f| f.flag == 0).count() as f32 * self.step
    }

    /// The sample shown at a moment: the first of a shot rather than the last before it.
    pub fn sample(&self, moment: usize) -> usize {
        let mut seen = 0;
        for (i, f) in self.frames.iter().enumerate() {
            if f.flag != 0 {
                continue;
            }
            if seen == moment {
                return i;
            }
            seen += 1;
        }
        self.frames.len() - 1
    }
}
