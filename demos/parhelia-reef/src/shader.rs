//! Direct3D 8 vertex shaders (vs.1.1) and pixel shaders (ps.1.0 to ps.1.3), as the demo ships
//! them in `Data/Shaders`, translated to WGSL. Registers become variables; constants are an
//! array of 96 vectors for the vertex shader and 8 for the pixel shader.
use crate::Error;

/// Vertex inputs the vertex declaration provides, by register: position, blend weights,
/// blend indices, normal, diffuse, specular, texture coordinates 0 and 1, tangent, binormal,
/// then texture coordinates 2 and 3 (one number each in the demo's FVFs).
pub const INPUTS: [usize; 12] = [0, 1, 2, 3, 5, 6, 7, 8, 14, 15, 9, 10];

#[derive(Debug)]
struct Operand {
    neg: bool,
    reg: String,
    swizzle: String,
    modifier: String,
}

fn operand(s: &str) -> Operand {
    let mut s = s.trim();
    let mut neg = false;
    if let Some(r) = s.strip_prefix('-') {
        neg = true;
        s = r.trim();
    }
    let mut invert = false;
    if let Some(r) = s.strip_prefix("1-") {
        invert = true;
        s = r.trim();
    }
    let (body, swizzle) = match s.rfind('.') {
        Some(i) if !s[i + 1..].contains(']') => (&s[..i], s[i + 1..].to_string()),
        _ => (s, String::new()),
    };
    let (reg, modifier) = match body.split_once('_') {
        Some((r, m)) => (r.to_string(), m.to_string()),
        None => (body.to_string(), String::new()),
    };
    Operand {
        neg,
        reg,
        swizzle,
        modifier: if invert { "invert".into() } else { modifier },
    }
}

/// A source as a vec4 expression.
fn source(o: &Operand, ps: bool) -> String {
    let reg = o.reg.as_str();
    let base = if let Some(idx) = reg.strip_prefix("c[").and_then(|r| r.strip_suffix(']')) {
        // c[a0.x + n] or c[a0.x]
        let n = idx
            .split('+')
            .nth(1)
            .map_or(0, |n| n.trim().parse::<i32>().unwrap_or(0));
        format!("c[clamp(a0 + {n}, 0, 95)]")
    } else if let Some(n) = reg.strip_prefix('c') {
        format!("c[{n}]")
    } else if let Some(n) = reg.strip_prefix('v') {
        if ps {
            format!("vd{n}")
        } else {
            format!("v{n}")
        }
    } else {
        reg.to_string()
    };
    let mut e = if o.swizzle.is_empty() {
        base
    } else {
        let sw: String = o
            .swizzle
            .chars()
            .map(|c| match c {
                'r' => 'x',
                'g' => 'y',
                'b' => 'z',
                'a' => 'w',
                c => c,
            })
            .collect();
        let full: String = match sw.len() {
            1 => sw.repeat(4),
            n if n < 4 => {
                let last = sw.chars().last().unwrap_or('x');
                format!("{sw}{}", last.to_string().repeat(4 - n))
            }
            _ => sw,
        };
        format!("({base}).{full}")
    };
    e = match o.modifier.as_str() {
        "bx2" => format!("({e} * 2.0 - vec4<f32>(1.0))"),
        "bias" => format!("({e} - vec4<f32>(0.5))"),
        "x2" => format!("({e} * 2.0)"),
        "invert" => format!("(vec4<f32>(1.0) - {e})"),
        _ => e,
    };
    if o.neg {
        e = format!("(-{e})");
    }
    e
}

fn mask(dst: &str) -> (String, String) {
    match dst.split_once('.') {
        Some((r, m)) => {
            let m: String = m
                .chars()
                .map(|c| match c {
                    'r' => 'x',
                    'g' => 'y',
                    'b' => 'z',
                    'a' => 'w',
                    c => c,
                })
                .collect();
            (r.to_string(), m)
        }
        None => (dst.to_string(), "xyzw".into()),
    }
}

fn dest_name(r: &str, ps: bool) -> String {
    match r {
        "oPos" => "o_pos".into(),
        "oFog" => "o_fog".into(),
        "oPts" => "o_pts".into(),
        r if r.starts_with("oD") => format!("o_d{}", &r[2..]),
        r if r.starts_with("oT") => format!("o_t{}", &r[2..]),
        r if ps && r.starts_with('v') => format!("vd{}", &r[1..]),
        r => r.to_string(),
    }
}

/// Assigns a vec4 expression to the masked components of a register.
fn assign(out: &mut String, dst: &str, value: &str, ps: bool, scale: &str, sat: bool) {
    let (r, m) = mask(dst);
    let r = dest_name(&r, ps);
    let mut v = format!("({value})");
    if !scale.is_empty() {
        v = format!("({v} * {scale})");
    }
    if sat {
        v = format!("clamp({v}, vec4<f32>(0.0), vec4<f32>(1.0))");
    } else if ps {
        v = format!("clamp({v}, vec4<f32>(-1.0), vec4<f32>(1.0))");
    }
    out.push_str(&format!("    {{ let t = {v};\n"));
    for c in m.chars() {
        out.push_str(&format!("      {r}.{c} = t.{c};\n"));
    }
    out.push_str("    }\n");
}

fn lines(src: &str) -> Vec<String> {
    src.lines()
        .map(|l| {
            l.split(';')
                .next()
                .unwrap_or("")
                .split("//")
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn split_args(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for ch in rest.chars() {
        match ch {
            '[' => {
                depth += 1;
                cur.push(ch);
            }
            ']' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The body of a vertex shader function: reads `vN`, writes `o_pos`, `o_dN`, `o_tN`.
pub fn vertex(src: &str) -> Result<String, Error> {
    let mut out = String::new();
    for line in lines(src) {
        let (op, rest) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
        let op = op.to_ascii_lowercase();
        if op.starts_with("vs.") || op == "dcl" {
            continue;
        }
        let a = split_args(rest);
        let s = |i: usize| -> String {
            a.get(i)
                .map(|x| source(&operand(x), false))
                .unwrap_or_default()
        };
        let d = a.first().cloned().unwrap_or_default();
        let value = match op.as_str() {
            "mov" if d.starts_with("a0") => {
                out.push_str(&format!("    a0 = i32(floor(({}).x + 0.5));\n", s(1)));
                continue;
            }
            "mov" => s(1),
            "add" => format!("{} + {}", s(1), s(2)),
            "sub" => format!("{} - {}", s(1), s(2)),
            "mul" => format!("{} * {}", s(1), s(2)),
            "mad" => format!("{} * {} + {}", s(1), s(2), s(3)),
            "dp3" => format!("vec4<f32>(dot(({}).xyz, ({}).xyz))", s(1), s(2)),
            "dp4" => format!("vec4<f32>(dot({}, {}))", s(1), s(2)),
            "min" => format!("min({}, {})", s(1), s(2)),
            "max" => format!("max({}, {})", s(1), s(2)),
            "slt" => format!(
                "select(vec4<f32>(0.0), vec4<f32>(1.0), {} < {})",
                s(1),
                s(2)
            ),
            "sge" => format!(
                "select(vec4<f32>(0.0), vec4<f32>(1.0), {} >= {})",
                s(1),
                s(2)
            ),
            "rcp" => format!("vec4<f32>(rcp_({}.x))", s(1)),
            "rsq" => format!("vec4<f32>(rsq_({}.x))", s(1)),
            "exp" | "expp" => format!("vec4<f32>(exp2({}.x))", s(1)),
            "log" | "logp" => format!("vec4<f32>(log2(max(abs({}.x), 1e-30)))", s(1)),
            "frc" => format!("fract({})", s(1)),
            "lit" => format!("lit_({})", s(1)),
            "dst" => format!(
                "vec4<f32>(1.0, ({0}).y * ({1}).y, ({0}).z, ({1}).w)",
                s(1),
                s(2)
            ),
            "m4x4" | "m4x3" | "m3x4" | "m3x3" | "m3x2" => {
                let rows: usize = op[3..4].parse().unwrap_or(4);
                let cols: usize = op[1..2].parse().unwrap_or(4);
                let src = s(1);
                let base = a.get(2).cloned().unwrap_or_default();
                let reg = |k: usize| -> String {
                    if let Some(idx) = base.strip_prefix("c[").and_then(|r| r.strip_suffix(']')) {
                        let n = idx
                            .split('+')
                            .nth(1)
                            .map_or(0, |n| n.trim().parse::<i32>().unwrap_or(0));
                        format!("c[clamp(a0 + {}, 0, 95)]", n + k as i32)
                    } else {
                        let n: usize = base.trim_start_matches('c').parse().unwrap_or(0);
                        format!("c[{}]", n + k)
                    }
                };
                let comps: Vec<String> = (0..rows)
                    .map(|k| match cols {
                        4 => format!("dot({src}, {})", reg(k)),
                        _ => format!("dot(({src}).xyz, ({}).xyz)", reg(k)),
                    })
                    .collect();
                let mut c = comps.clone();
                while c.len() < 4 {
                    c.push("0.0".into());
                }
                // Only the written components count: the destination mask is xyz, xy, ...
                let (r, m) = mask(&d);
                let m = if m == "xyzw" {
                    "xyzw"[..rows].to_string()
                } else {
                    m
                };
                let fixed = format!("{r}.{m}");
                let v = format!("vec4<f32>({}, {}, {}, {})", c[0], c[1], c[2], c[3]);
                assign(&mut out, &fixed, &v, false, "", false);
                continue;
            }
            "nop" => continue,
            o => return Err(format!("vertex shader: {o}").into()),
        };
        assign(&mut out, &d, &value, false, "", false);
    }
    Ok(out)
}

/// The body of a pixel shader function: reads `vd0`, `vd1`, texture coordinates `tcN` and
/// samples with `sample(N, uv)`; writes `r0`.
pub fn pixel(src: &str) -> Result<String, Error> {
    let mut out = String::new();
    for line in lines(src) {
        let (coissue, line) = match line.strip_prefix('+') {
            Some(l) => (true, l.trim().to_string()),
            None => (false, line),
        };
        let _ = coissue;
        let (op, rest) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
        let op = op.to_ascii_lowercase();
        if op.starts_with("ps.") {
            continue;
        }
        let mut parts = op.split('_');
        let base = parts.next().unwrap_or("").to_string();
        let mut scale = String::new();
        let mut sat = false;
        for p in parts {
            match p {
                "x2" => scale = "2.0".into(),
                "x4" => scale = "4.0".into(),
                "d2" => scale = "0.5".into(),
                "sat" => sat = true,
                _ => {}
            }
        }
        let a = split_args(rest);
        let s = |i: usize| -> String {
            a.get(i)
                .map(|x| source(&operand(x), true))
                .unwrap_or_default()
        };
        let d = a.first().cloned().unwrap_or_default();
        let value = match base.as_str() {
            "def" => {
                let n: usize = d.trim_start_matches('c').parse().unwrap_or(0);
                let v: Vec<&str> = a[1..].iter().map(String::as_str).collect();
                out.push_str(&format!("    c[{n}] = vec4<f32>({});\n", v.join(", ")));
                continue;
            }
            "tex" => {
                let n: usize = d.trim_start_matches('t').parse().unwrap_or(0);
                out.push_str(&format!("    t{n} = sample_{n}(tc{n});\n"));
                continue;
            }
            "texcoord" => {
                let n: usize = d.trim_start_matches('t').parse().unwrap_or(0);
                out.push_str(&format!(
                    "    t{n} = clamp(tc{n}, vec4<f32>(0.0), vec4<f32>(1.0));\n"
                ));
                continue;
            }
            "texbem" | "texbeml" => {
                let n: usize = d.trim_start_matches('t').parse().unwrap_or(0);
                let src = s(1);
                out.push_str(&format!(
                    "    {{ let m = u.bump[{n}]; let o = vec2<f32>(m.x * ({src}).x + m.z * ({src}).y, m.y * ({src}).x + m.w * ({src}).y);\n      t{n} = sample_{n}(vec4<f32>(tc{n}.xy + o, tc{n}.zw));\n"
                ));
                if base == "texbeml" {
                    out.push_str(&format!(
                        "      t{n} = vec4<f32>(t{n}.rgb * clamp(({src}).z * u.bumpl[{n}].x + u.bumpl[{n}].y, 0.0, 1.0), t{n}.a);\n"
                    ));
                }
                out.push_str("    }\n");
                continue;
            }
            "texdp3tex" => {
                let n: usize = d.trim_start_matches('t').parse().unwrap_or(0);
                out.push_str(&format!(
                    "    t{n} = sample_{n}(vec4<f32>(dot(tc{n}.xyz, ({}).xyz), 0.0, 0.0, 1.0));\n",
                    s(1)
                ));
                continue;
            }
            "texdp3" => {
                let n: usize = d.trim_start_matches('t').parse().unwrap_or(0);
                out.push_str(&format!(
                    "    t{n} = vec4<f32>(dot(tc{n}.xyz, ({}).xyz));\n",
                    s(1)
                ));
                continue;
            }
            "mov" => s(1),
            "add" => format!("{} + {}", s(1), s(2)),
            "sub" => format!("{} - {}", s(1), s(2)),
            "mul" => format!("{} * {}", s(1), s(2)),
            "mad" => format!("{} * {} + {}", s(1), s(2), s(3)),
            "lrp" => format!("mix({2}, {1}, {0})", s(1), s(2), s(3)),
            "dp3" => format!("vec4<f32>(dot(({}).xyz, ({}).xyz))", s(1), s(2)),
            "dp4" => format!("vec4<f32>(dot({}, {}))", s(1), s(2)),
            "cnd" => format!(
                "select({2}, {1}, vec4<f32>(({0}).w) > vec4<f32>(0.5))",
                s(1),
                s(2),
                s(3)
            ),
            "cmp" => format!("select({2}, {1}, {0} >= vec4<f32>(0.0))", s(1), s(2), s(3)),
            "nop" => continue,
            o => return Err(format!("pixel shader: {o}").into()),
        };
        assign(&mut out, &d, &value, true, &scale, sat);
    }
    Ok(out)
}

/// A whole program: the vertex shader, the pixel shader, and which stages sample volumes.
pub fn program(vs: &str, ps: &str, volumes: [bool; 4]) -> Result<String, Error> {
    let vbody = vertex(vs)?.replace("c[", "u.c[");
    let pbody = pixel(ps)?;
    let mut w = String::new();
    w.push_str("@vertex\nfn vs(i: In) -> Out {\n");
    for r in 0..16 {
        let have = INPUTS.contains(&r);
        if have {
            w.push_str(&format!("    let v{r} = i.v{r};\n"));
        } else {
            w.push_str(&format!("    let v{r} = vec4<f32>(0.0);\n"));
        }
    }
    w.push_str("    var a0: i32 = 0;\n");
    for r in 0..12 {
        w.push_str(&format!("    var r{r} = vec4<f32>(0.0);\n"));
    }
    w.push_str("    var o_pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);\n    var o_d0 = vec4<f32>(0.0);\n    var o_d1 = vec4<f32>(0.0);\n    var o_fog = vec4<f32>(0.0);\n    var o_pts = vec4<f32>(0.0);\n");
    for t in 0..4 {
        w.push_str(&format!("    var o_t{t} = vec4<f32>(0.0);\n"));
    }
    w.push_str(&vbody);
    w.push_str("    var o: Out;\n    o.pos = o_pos;\n    o.d0 = clamp(o_d0, vec4<f32>(0.0), vec4<f32>(1.0));\n    o.d1 = clamp(o_d1, vec4<f32>(0.0), vec4<f32>(1.0));\n    o.t0 = o_t0;\n    o.t1 = o_t1;\n    o.t2 = o_t2;\n    o.t3 = o_t3;\n    return o;\n}\n");
    for (n, vol) in volumes.iter().enumerate() {
        if *vol {
            w.push_str(&format!("fn sample_{n}(tc: vec4<f32>) -> vec4<f32> {{ return textureSample(t3d{n}, smp{n}, tc.xyz); }}\n"));
        } else {
            w.push_str(&format!("fn sample_{n}(tc: vec4<f32>) -> vec4<f32> {{ return textureSample(t2d{n}, smp{n}, tc.xy); }}\n"));
        }
    }
    w.push_str("@fragment\nfn fs(i: Out) -> @location(0) vec4<f32> {\n    let vd0 = i.d0;\n    let vd1 = i.d1;\n    let tc0 = i.t0;\n    let tc1 = i.t1;\n    let tc2 = i.t2;\n    let tc3 = i.t3;\n");
    w.push_str("    var t0 = vec4<f32>(0.0);\n    var t1 = vec4<f32>(0.0);\n    var t2 = vec4<f32>(0.0);\n    var t3 = vec4<f32>(0.0);\n    var r0 = vec4<f32>(0.0);\n    var r1 = vec4<f32>(0.0);\n    var c = u.pc;\n");
    w.push_str(&pbody);
    w.push_str("    if !passes_alpha(r0.a) { discard; }\n    return r0;\n}\n");
    Ok(w)
}
