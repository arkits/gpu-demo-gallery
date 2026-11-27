//! NV_fragment_program 1.0 (`!!FP1.0`, as Cg 1.1 writes it for the `fp30` profile) to a
//! WGSL fragment function. The demo ships its programs as these listings; they are read
//! and translated when it starts, so none of them is part of this repository.
//!
//! Registers become `vec4<f32>` variables, R and H alike (Cg keeps them apart), X
//! (fixed point) results are kept between -2 and 2, and condition codes are a vector
//! that instructions with a C suffix set and `(GT.x)`-style masks test.

use crate::Error;

/// Where a program's named parameters (`DECLARE`) live among the draw's numbers.
pub const SLOTS: &[(&str, usize)] = &[
    ("timefunc", 20),
    ("scenetime", 21),
    ("mixamount", 22),
    ("base_color", 24),
    ("reflectivity", 25),
    ("ambient_light", 26),
    ("scene_time", 27),
];
/// The size of the rectangle texture, for coordinates in pixels.
pub const RECT_SIZE: usize = 23;

fn slot(name: &str) -> Option<usize> {
    SLOTS.iter().find(|s| s.0 == name).map(|s| s.1)
}

/// Splits `R0.xyz` into the name and the components, as indices.
fn components(s: &str) -> Vec<usize> {
    s.chars()
        .filter_map(|c| match c {
            'x' | 'r' => Some(0),
            'y' | 'g' => Some(1),
            'z' | 'b' => Some(2),
            'w' | 'a' => Some(3),
            _ => None,
        })
        .collect()
}

const AXES: [char; 4] = ['x', 'y', 'z', 'w'];

/// A source operand as a WGSL `vec4<f32>` expression.
fn source(s: &str, vars: &mut Vars) -> Result<String, Error> {
    let s = s.trim();
    let (negate, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    let (abs, s) = match s.strip_prefix('|').and_then(|r| r.strip_suffix('|')) {
        Some(inner) => (true, inner.trim()),
        None => (false, s),
    };
    // Constants: `{1.2}.x`, `{0, 1, 2, 0}.xyzx`.
    let (base, swizzle) = if let Some(rest) = s.strip_prefix('{') {
        let end = rest.find('}').ok_or("unclosed constant")?;
        let values: Vec<f32> = rest[..end]
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<Result<_, _>>()?;
        let mut v = values.clone();
        while v.len() < 4 {
            v.push(*values.last().unwrap_or(&0.0));
        }
        let text = format!("vec4<f32>({:?}, {:?}, {:?}, {:?})", v[0], v[1], v[2], v[3]);
        let swizzle = rest[end + 1..].trim().strip_prefix('.').unwrap_or("");
        (text, swizzle.to_string())
    } else {
        let (name, swizzle) = match s.split_once('.') {
            Some((n, w)) if !n.contains('[') || n.ends_with(']') => (n, w),
            _ => (s, ""),
        };
        let base = if let Some(inner) = name.strip_prefix("f[").and_then(|n| n.strip_suffix(']')) {
            match inner {
                "TEX0" => "i.uv0".to_string(),
                "TEX1" => "i.uv1".to_string(),
                "TEX2" => "i.uv2".to_string(),
                "TEX3" => "i.uv3".to_string(),
                "COL0" => "i.color".to_string(),
                "WPOS" => "wpos".to_string(),
                other => return Err(format!("input {other}").into()),
            }
        } else if let Some(n) = slot(name) {
            format!("params.v[{n}]")
        } else if name.starts_with('R') || name.starts_with('H') {
            vars.reg(name);
            name.to_string()
        } else {
            return Err(format!("operand {name}").into());
        };
        (base, swizzle.to_string())
    };
    let mut e = base;
    let sw = components(&swizzle);
    if !sw.is_empty() {
        let mut sw = sw;
        while sw.len() < 4 {
            sw.push(*sw.last().expect("not empty"));
        }
        let text: String = sw.iter().map(|&c| AXES[c]).collect();
        e = format!("({e}).{text}");
    }
    if abs {
        e = format!("abs({e})");
    }
    if negate {
        e = format!("(-{e})");
    }
    Ok(e)
}

#[derive(Default)]
struct Vars {
    regs: Vec<String>,
}

impl Vars {
    fn reg(&mut self, name: &str) {
        if !self.regs.iter().any(|r| r == name) {
            self.regs.push(name.to_string());
        }
    }
}

/// Splits operands at commas that are not inside braces.
fn operands(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0, String::new());
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur).trim().to_string());
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// One fragment function, `fn NAME(i: V) -> @location(0) vec4<f32>`.
pub fn translate(name: &str, listing: &str) -> Result<String, Error> {
    let mut vars = Vars::default();
    let mut body = String::new();
    let mut text = String::new();
    for line in listing.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with("!!") || line == "END" {
            continue;
        }
        text.push_str(line);
        text.push(' ');
    }
    for stmt in text.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() || stmt.starts_with("DECLARE") {
            continue;
        }
        let (op, rest) = stmt.split_once(char::is_whitespace).unwrap_or((stmt, ""));
        let ops = operands(rest);
        let line = instruction(op, &ops, &mut vars).map_err(|e| format!("{name}: {stmt}: {e}"))?;
        body.push_str(&line);
    }
    let mut out = format!("@fragment\nfn {name}(i: V) -> @location(0) vec4<f32> {{\n");
    // The reflection shows only on the floor (the stencil of 0x4067c0).
    out.push_str("  if params.v[28].x > 0.5 && length(i.uv3.xz) > params.v[28].y { discard; }\n");
    out.push_str("  let wpos = vec4<f32>(i.clip.x, params.v[23].y - i.clip.y, i.clip.z, 1.0);\n");
    out.push_str("  var cc = vec4<f32>(0.0);\n  var colr = vec4<f32>(0.0, 0.0, 0.0, 1.0);\n");
    for r in &vars.regs {
        out.push_str(&format!("  var {r} = vec4<f32>(0.0);\n"));
    }
    out.push_str(&body);
    out.push_str("  return colr;\n}\n");
    Ok(out)
}

fn instruction(op: &str, ops: &[String], vars: &mut Vars) -> Result<String, Error> {
    // OP, precision R/H/X, C (sets condition codes), _SAT.
    let (op, sat) = match op.strip_suffix("_SAT") {
        Some(o) => (o, true),
        None => (op, false),
    };
    let (base, precision, sets_cc) = split_op(op)?;
    let dst_text = ops.first().ok_or("no destination")?;
    let mut s =
        |k: usize| -> Result<String, Error> { source(ops.get(k).ok_or("missing operand")?, vars) };
    let value = match base {
        "MOV" => s(1)?,
        "MUL" => format!("{} * {}", s(1)?, s(2)?),
        "ADD" => format!("{} + {}", s(1)?, s(2)?),
        "MAD" => format!("{} * {} + {}", s(1)?, s(2)?, s(3)?),
        "DP3" => format!("vec4<f32>(dot(({}).xyz, ({}).xyz))", s(1)?, s(2)?),
        "DP4" => format!("vec4<f32>(dot({}, {}))", s(1)?, s(2)?),
        "POW" => format!("vec4<f32>(pow(({}).x, ({}).x))", s(1)?, s(2)?),
        "SIN" => format!("vec4<f32>(sin(({}).x))", s(1)?),
        "COS" => format!("vec4<f32>(cos(({}).x))", s(1)?),
        "RSQ" => format!("vec4<f32>(inverseSqrt(abs(({}).x)))", s(1)?),
        "RCP" => format!("vec4<f32>(1.0 / ({}).x)", s(1)?),
        "EX2" => format!("vec4<f32>(exp2(({}).x))", s(1)?),
        "LG2" => format!("vec4<f32>(log2(abs(({}).x)))", s(1)?),
        "FLR" => format!("floor({})", s(1)?),
        "FRC" => format!("fract({})", s(1)?),
        "MIN" => format!("min({}, {})", s(1)?, s(2)?),
        "MAX" => format!("max({}, {})", s(1)?, s(2)?),
        "LRP" => {
            let (a, b, c) = (s(1)?, s(2)?, s(3)?);
            format!("{a} * {b} + (vec4<f32>(1.0) - {a}) * {c}")
        }
        k @ ("SEQ" | "SNE" | "SLT" | "SLE" | "SGT" | "SGE") => {
            let cmp = match k {
                "SEQ" => "==",
                "SNE" => "!=",
                "SLT" => "<",
                "SLE" => "<=",
                "SGT" => ">",
                _ => ">=",
            };
            format!(
                "select(vec4<f32>(0.0), vec4<f32>(1.0), {} {cmp} {})",
                s(1)?,
                s(2)?
            )
        }
        "TEX" => {
            let coord = s(1)?;
            let unit = ops.get(2).ok_or("no texture unit")?.trim();
            let target = ops.get(3).ok_or("no texture target")?.trim();
            match (unit, target) {
                (_, "CUBE") => format!("textureSample(cube, cube_s, ({coord}).xyz)"),
                ("TEX0", "2D") => format!("textureSample(t0, s0, ({coord}).xy)"),
                (_, "RECT") => format!(
                    "textureSample(t1, s1, vec2<f32>(({coord}).x / params.v[23].x, 1.0 - ({coord}).y / params.v[23].y))"
                ),
                ("TEX1", "2D") => format!("textureSample(t1, s1, ({coord}).xy)"),
                other => return Err(format!("texture {other:?}").into()),
            }
        }
        other => return Err(format!("instruction {other}").into()),
    };
    let mut value = format!("({value})");
    if precision == 'X' {
        value = format!("clamp({value}, vec4<f32>(-2.0), vec4<f32>(2.0))");
    }
    if sat {
        value = format!("clamp({value}, vec4<f32>(0.0), vec4<f32>(1.0))");
    }
    // Destination: `R1.w(GT.x)`, `o[COLR].xyz`, `HC.x`.
    let (dst, condition) = match dst_text.split_once('(') {
        Some((d, c)) => (d.trim(), Some(c.trim_end_matches(')').trim())),
        None => (dst_text.trim(), None),
    };
    let (name, mask) = match dst.split_once('.') {
        Some((n, m)) if !n.ends_with('[') => (n, components(m)),
        _ => (dst, vec![0, 1, 2, 3]),
    };
    let target = match name {
        "o[COLR]" | "o[COLH]" => Some("colr".to_string()),
        "HC" | "RC" => None,
        r if r.starts_with('R') || r.starts_with('H') => {
            vars.reg(r);
            Some(r.to_string())
        }
        other => return Err(format!("destination {other}").into()),
    };
    let mut out = format!("  {{\n    let t = {value};\n");
    for &c in &mask {
        let axis = AXES[c];
        let test = match condition {
            None => "true".to_string(),
            Some(cond) => {
                let (code, sw) = cond.split_once('.').unwrap_or((cond, "xyzw"));
                let sw = components(sw);
                let k = AXES[*sw.get(c).or(sw.last()).unwrap_or(&0)];
                match code {
                    "GT" => format!("cc.{k} > 0.0"),
                    "GE" => format!("cc.{k} >= 0.0"),
                    "LT" => format!("cc.{k} < 0.0"),
                    "LE" => format!("cc.{k} <= 0.0"),
                    "EQ" => format!("cc.{k} == 0.0"),
                    "NE" => format!("cc.{k} != 0.0"),
                    "TR" => "true".to_string(),
                    "FL" => "false".to_string(),
                    other => return Err(format!("condition {other}").into()),
                }
            }
        };
        if let Some(t) = &target {
            out.push_str(&format!("    if ({test}) {{ {t}.{axis} = t.{axis}; }}\n"));
        }
        if sets_cc {
            out.push_str(&format!("    if ({test}) {{ cc.{axis} = t.{axis}; }}\n"));
        }
    }
    out.push_str("  }\n");
    Ok(out)
}

/// `MADR` -> (MAD, R, no codes); `MOVXC` -> (MOV, X, codes); `TEX` -> (TEX, R, no).
fn split_op(op: &str) -> Result<(&str, char, bool), Error> {
    const NAMES: &[&str] = &[
        "MOV", "MUL", "ADD", "MAD", "DP3", "DP4", "POW", "SIN", "COS", "RSQ", "RCP", "EX2", "LG2",
        "FLR", "FRC", "MIN", "MAX", "LRP", "SEQ", "SNE", "SLT", "SLE", "SGT", "SGE", "TEX",
    ];
    let base = NAMES
        .iter()
        .find(|n| op.starts_with(**n))
        .ok_or_else(|| format!("instruction {op}"))?;
    let rest = &op[base.len()..];
    let mut precision = 'R';
    let mut cc = false;
    for c in rest.chars() {
        match c {
            'R' | 'H' | 'X' => precision = c,
            'C' => cc = true,
            _ => return Err(format!("instruction {op}").into()),
        }
    }
    Ok((base, precision, cc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_a_small_program() {
        let p = "!!FP1.0\nDECLARE mixamount;\nTEX R0, -f[TEX1].xyzx, TEX0, CUBE;\nMULR R1.xyz, f[TEX0].xyzx, {1.2}.x;\nSLTR H0.x, R1.w, {0}.x;\nMOVXC HC.x, H0.x;\nMOVR R1.w(GT.x), {0}.x;\nMOVR o[COLR].xyz, R1;\nMOVR o[COLR].w, mixamount.x;\nEND\n";
        let w = translate("look", p).unwrap();
        assert!(w.contains("textureSample(cube"), "{w}");
        assert!(w.contains("if (cc.x > 0.0) { R1.w"), "{w}");
        assert!(w.contains("params.v[22]"), "{w}");
    }

    #[test]
    fn splits_instruction_names() {
        assert_eq!(split_op("MOVXC").unwrap(), ("MOV", 'X', true));
        assert_eq!(split_op("DP3R").unwrap(), ("DP3", 'R', false));
        assert_eq!(split_op("TEX").unwrap(), ("TEX", 'R', false));
    }
}
