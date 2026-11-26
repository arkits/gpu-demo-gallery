//! Turns `!!VP1.0` vertex programs into WGSL. A program reads attributes `v[0..16]` and
//! constants `c[0..96]`, and writes the clip position, two colors, fog and four
//! texture coordinates.
use std::fmt::Write;

use crate::Error;

/// Attribute names of the language and their registers.
const INPUTS: [(&str, usize); 13] = [
    ("OPOS", 0),
    ("WGHT", 1),
    ("NRML", 2),
    ("COL0", 3),
    ("COL1", 4),
    ("FOGC", 5),
    ("TEX0", 8),
    ("TEX1", 9),
    ("TEX2", 10),
    ("TEX3", 11),
    ("TEX4", 12),
    ("TEX5", 13),
    ("TEX6", 14),
];

/// Outputs that matter here. Point sizes and back colors go nowhere.
fn output(name: &str) -> Option<&'static str> {
    Some(match name {
        "HPOS" => "hpos",
        "COL0" => "col0",
        "COL1" => "col1",
        "FOGC" => "fogc",
        "TEX0" => "tex0",
        "TEX1" => "tex1",
        "TEX2" => "tex2",
        "TEX3" => "tex3",
        _ => return None,
    })
}

fn bracketed<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)?
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')
}

/// A register's value, without swizzle.
fn register(s: &str) -> Result<String, Error> {
    let bad = || format!("unsupported register {s}");
    if let Some(n) = s.strip_prefix('R') {
        let n: usize = n.parse().map_err(|_| bad())?;
        return if n < 12 {
            Ok(format!("r{n}"))
        } else {
            Err(bad().into())
        };
    }
    if let Some(n) = bracketed(s, "v") {
        let i = match n.parse::<usize>() {
            Ok(i) => i,
            Err(_) => INPUTS.iter().find(|x| x.0 == n).ok_or_else(bad)?.1,
        };
        return Ok(format!("v{i}"));
    }
    let n = bracketed(s, "c").ok_or_else(bad)?.replace(' ', "");
    if let Some(rest) = n.strip_prefix("A0.x") {
        let offset: i32 = match rest {
            "" => 0,
            r => r.parse().map_err(|_| bad())?,
        };
        return Ok(format!("draw.c[clamp(a0 + {offset}, 0, 95)]"));
    }
    let i: usize = n.parse().map_err(|_| bad())?;
    if i < 96 {
        Ok(format!("draw.c[{i}]"))
    } else {
        Err(bad().into())
    }
}

/// An operand: an optional sign, a register and a swizzle of one or four letters.
fn operand(s: &str) -> Result<String, Error> {
    let s = s.trim();
    let (sign, s) = match s.strip_prefix('-') {
        Some(rest) => ("-", rest.trim()),
        None => ("", s),
    };
    // The address register has a dot of its own inside the brackets.
    let dot = s.rfind('.').filter(|&i| !s[i..].contains(']'));
    let (base, swizzle) = match dot {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, "xyzw"),
    };
    let swizzle = match swizzle.len() {
        1 => swizzle.repeat(4),
        4 => swizzle.to_owned(),
        _ => return Err(format!("unsupported swizzle {swizzle}").into()),
    };
    if swizzle.bytes().any(|b| !b"xyzw".contains(&b)) {
        return Err(format!("unsupported swizzle {swizzle}").into());
    }
    let base = register(base.trim())?;
    Ok(match swizzle.as_str() {
        "xyzw" => format!("{sign}{base}"),
        w => format!("{sign}{base}.{w}"),
    })
}

/// Splits operands at commas, which never occur inside brackets.
fn statement(line: &str) -> Result<String, Error> {
    let (op, rest) = line
        .split_once(char::is_whitespace)
        .ok_or("instruction without operands")?;
    let mut parts = rest.split(',').map(str::trim);
    let target = parts.next().unwrap_or("");
    let x = parts.map(operand).collect::<Result<Vec<_>, _>>()?;
    let two = || match x.as_slice() {
        [a, b] => Ok((a, b)),
        _ => Err(Error::from(format!("{op} wants two operands"))),
    };
    let one = || match x.as_slice() {
        [a] => Ok(a),
        _ => Err(Error::from(format!("{op} wants one operand"))),
    };
    if op == "ARL" {
        return Ok(format!("a0 = i32(floor(({}).x));", one()?));
    }
    // Infinities are kept finite: 0 times anything is 0 in the language.
    let value = match op {
        "MOV" => one()?.clone(),
        "MUL" => two().map(|(a, b)| format!("({a}) * ({b})"))?,
        "ADD" => two().map(|(a, b)| format!("({a}) + ({b})"))?,
        "SUB" => two().map(|(a, b)| format!("({a}) - ({b})"))?,
        "MIN" => two().map(|(a, b)| format!("min({a}, {b})"))?,
        "MAX" => two().map(|(a, b)| format!("max({a}, {b})"))?,
        "SLT" => two().map(|(a, b)| format!("select(vec4f(0.0), vec4f(1.0), ({a}) < ({b}))"))?,
        "SGE" => two().map(|(a, b)| format!("select(vec4f(0.0), vec4f(1.0), ({a}) >= ({b}))"))?,
        "DP3" => two().map(|(a, b)| format!("vec4f(dot(({a}).xyz, ({b}).xyz))"))?,
        "DP4" => two().map(|(a, b)| format!("vec4f(dot({a}, {b}))"))?,
        "DPH" => two().map(|(a, b)| format!("vec4f(dot(vec4f(({a}).xyz, 1.0), {b}))"))?,
        "DST" => two().map(|(a, b)| format!("vec4f(1.0, ({a}).y * ({b}).y, ({a}).z, ({b}).w)"))?,
        "MAD" => match x.as_slice() {
            [a, b, c] => format!("({a}) * ({b}) + ({c})"),
            _ => return Err("MAD wants three operands".into()),
        },
        "RCP" => format!("vec4f(rcp(({}).x))", one()?),
        "RSQ" => format!("vec4f(rsq(({}).x))", one()?),
        "LIT" => format!("lit({})", one()?),
        "EXP" => format!("expp(({}).x)", one()?),
        "LOG" => format!("logp(({}).x)", one()?),
        _ => return Err(format!("unsupported instruction {op}").into()),
    };
    let (base, mask) = match target.rfind('.').filter(|&i| !target[i..].contains(']')) {
        Some(i) => (&target[..i], &target[i + 1..]),
        None => (target, "xyzw"),
    };
    if mask.is_empty() || mask.bytes().any(|b| !b"xyzw".contains(&b)) {
        return Err(format!("unsupported mask {mask}").into());
    }
    let name = match bracketed(base.trim(), "o") {
        Some(o) => match output(o) {
            Some(name) => name.to_owned(),
            None => return Ok(String::new()),
        },
        None => register(base.trim())?,
    };
    if mask == "xyzw" {
        return Ok(format!("{name} = {value};"));
    }
    // WGSL assigns to one component at a time.
    let mut s = format!("{{ let t = {value};");
    for k in mask.chars() {
        let _ = write!(s, " {name}.{k} = t.{k};");
    }
    s.push_str(" }");
    Ok(s)
}

/// The WGSL body of a program. It expects `v0..v15`, `r0..r11`, `a0` and the outputs.
pub fn translate(source: &str) -> Result<String, Error> {
    let plain: String = source
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .collect::<Vec<_>>()
        .join(" ");
    let plain = plain.trim();
    let body = ["!!VP1.0", "!!VP1.1"]
        .iter()
        .find_map(|h| plain.strip_prefix(h))
        .ok_or("not a vertex program")?;
    let end = body.find("END").ok_or("vertex program without END")?;
    let mut out = String::new();
    for line in body[..end]
        .split(';')
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        let s = statement(line).map_err(|e| format!("{line}: {e}"))?;
        let _ = writeln!(out, "  {s}");
    }
    Ok(out)
}

/// Functions the translated programs call.
pub const PRELUDE: &str = "
fn rcp(x: f32) -> f32 { return select(1.0 / x, 1e18, x == 0.0); }
fn rsq(x: f32) -> f32 { return select(inverseSqrt(abs(x)), 1e18, x == 0.0); }
fn lit(a: vec4f) -> vec4f {
    let s = select(0.0, pow(max(a.y, 1e-9), clamp(a.w, -128.0, 128.0)), a.x > 0.0 && a.y > 0.0);
    return vec4f(1.0, max(a.x, 0.0), s, 1.0);
}
fn expp(x: f32) -> vec4f { return vec4f(exp2(floor(x)), x - floor(x), exp2(x), 1.0); }
fn logp(x: f32) -> vec4f {
    let l = log2(max(abs(x), 1e-30));
    return vec4f(floor(l), exp2(l - floor(l)), l, 1.0);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions() {
        let s = translate(
            "!!VP1.0 # a comment; with a semicolon\nARL A0.x, v[6].y;\n\
             DP4 R0.x, c[A0.x + 8], v[OPOS]; MAD R4, R3.yzxw, R2.zxyw,\n -R4;\n\
             RSQ R2.w, R2.w; MOV o[COL0].xyz, R4; MOV o[PSIZ].x, R4.x;\nEND trailing",
        )
        .unwrap();
        assert!(s.contains("a0 = i32(floor((v6.yyyy).x));"));
        assert!(s.contains("draw.c[clamp(a0 + 8, 0, 95)]"));
        assert!(s.contains("r4 = (r3.yzxw) * (r2.zxyw) + (-r4);"));
        assert!(s.contains("r2.w = t.w;") && s.contains("col0.z = t.z;"));
        assert!(!s.contains("PSIZ") && !s.contains("col0.w"));
    }

    #[test]
    fn rejects_what_it_does_not_know() {
        for bad in [
            "XYZ R0, R1;",
            "MOV R12, R0;",
            "MOV R0, c[96];",
            "ADD R0, R1;",
        ] {
            assert!(translate(&format!("!!VP1.0 {bad} END")).is_err(), "{bad}");
        }
        assert!(translate("MOV R0, R1; END").is_err());
    }
}
