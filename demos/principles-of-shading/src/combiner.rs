//! Turns combiner settings into WGSL. These describe the register combiners of the
//! GeForce: signed registers from -1 to 1, textures and colors from 0 to 1.
use std::fmt::Write;

use crate::Error;
use crate::tree::Node;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Input {
    pub source: u32,
    pub map: u32,
    pub alpha: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Expr {
    pub operator: u32,
    pub scale: u32,
    pub bias: u32,
    pub dest: [u32; 3],
    pub inputs: Vec<Input>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combiner {
    pub color: Expr,
    pub alpha: Expr,
}

/// Where a texture's coordinates come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coords {
    /// The mesh.
    Static,
    /// The view direction reflected at the surface, in the camera's space.
    EyeReflect,
}

/// What a fragment program depends on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Program {
    pub combiners: Vec<Combiner>,
    /// Coordinates, and whether the texture is a cube map.
    pub textures: Vec<(Coords, bool)>,
}

impl Expr {
    fn read(n: &Node) -> Self {
        let input = |i: &Node| Input {
            source: i.int("source"),
            map: i.int("map"),
            alpha: i.int("isAlpha") != 0,
        };
        Self {
            operator: n.int("operator"),
            scale: n.int("outputScale"),
            bias: n.int("outputBias"),
            dest: ["destAB", "destCD", "destABCD"].map(|d| n.int(d)),
            inputs: n.nodes("inputs").iter().map(input).collect(),
        }
    }
}

impl Combiner {
    pub fn read(n: &Node) -> Result<Self, Error> {
        Ok(Self {
            color: Expr::read(n.node("colorExpr").ok_or("combiner without color")?),
            alpha: Expr::read(n.node("alphaExpr").ok_or("combiner without alpha")?),
        })
    }
}

const DISCARD: u32 = 8;

/// One operand: a color, or an alpha spread over a color.
fn operand(i: &Input, for_alpha: bool) -> Result<String, Error> {
    let name = match i.source {
        0 => "vec4f(0.0)".to_owned(),
        1 => "m.color0".to_owned(),
        2 => "m.color1".to_owned(),
        4 => "primary".to_owned(),
        n @ 6..=7 => format!("tex{}", n - 6),
        n @ 14..=15 => format!("r{}", n - 14),
        n => return Err(format!("unsupported combiner source {n}").into()),
    };
    let value = match (for_alpha, i.alpha) {
        (false, false) => format!("{name}.rgb"),
        (false, true) => format!("vec3f({name}.a)"),
        (true, true) => format!("{name}.a"),
        // A color used for alpha gives its blue.
        (true, false) => format!("{name}.b"),
    };
    let zero = if for_alpha { "0.0" } else { "vec3f(0.0)" };
    Ok(match i.map {
        0 => format!("max({value}, {zero})"),
        1 => format!("(1.0 - saturate({value}))"),
        2 => format!("(2.0 * max({value}, {zero}) - 1.0)"),
        3 => format!("(1.0 - 2.0 * max({value}, {zero}))"),
        4 => format!("(max({value}, {zero}) - 0.5)"),
        5 => format!("(0.5 - max({value}, {zero}))"),
        6 => value,
        7 => format!("(-{value})"),
        n => return Err(format!("unsupported combiner map {n}").into()),
    })
}

fn expression(e: &Expr, for_alpha: bool) -> Result<Option<String>, Error> {
    if e.dest[0] == DISCARD {
        return Ok(None);
    }
    if e.dest[0] > 1 || e.dest[1] != DISCARD || e.dest[2] != DISCARD {
        return Err("unsupported combiner destination".into());
    }
    let x: Vec<String> = e
        .inputs
        .iter()
        .map(|i| operand(i, for_alpha))
        .collect::<Result<_, _>>()?;
    let value = match (e.operator, x.as_slice()) {
        (1, [a]) => a.clone(),
        (2, [a]) => format!("({a} * {a})"),
        (3, [a, b]) => format!("({a} * {b})"),
        (4, [a, b]) if !for_alpha => format!("vec3f(dot({a}, {b}))"),
        (5, [a, b]) => format!("({a} + {b})"),
        (9, [a, b, c]) => format!("({a} * {b} + (1.0 - {a}) * {c})"),
        (op, x) => return Err(format!("unsupported operator {op} of {} inputs", x.len()).into()),
    };
    let bias = if e.bias == 1 { " - 0.5" } else { "" };
    let scale = match e.scale {
        0 => "1.0",
        1 => "2.0",
        2 => "4.0",
        3 => "0.5",
        n => return Err(format!("unsupported output scale {n}").into()),
    };
    Ok(Some(format!(
        "clamp(({value}{bias}) * {scale}, -{0}, {0})",
        if for_alpha { "1.0" } else { "vec3f(1.0)" }
    )))
}

impl Program {
    /// The fragment program: `shade` computes the color from `primary`, the
    /// interpolated lighting, and the textures.
    pub fn wgsl(&self) -> Result<String, Error> {
        let mut s = String::new();
        for (i, (_, cube)) in self.textures.iter().enumerate() {
            let ty = if *cube {
                "texture_cube<f32>"
            } else {
                "texture_2d<f32>"
            };
            let _ = writeln!(s, "@group(1) @binding({i}) var t{i}: {ty};");
        }
        s.push_str("fn shade(v: V, primary: vec4f) -> vec4f {\n");
        for (i, (coords, cube)) in self.textures.iter().enumerate() {
            let at = match (coords, cube) {
                (Coords::Static, false) => "v.uv",
                (Coords::EyeReflect, true) => "reflect(normalize(v.eye), normalize(v.normal))",
                other => return Err(format!("unsupported texture coordinates {other:?}").into()),
            };
            let _ = writeln!(s, "  let tex{i} = textureSample(t{i}, samp, {at});");
        }
        let start = if self.textures.is_empty() {
            "1.0"
        } else {
            "tex0.a"
        };
        let _ = writeln!(
            s,
            "  var r0 = vec4f(0.0, 0.0, 0.0, {start});\n  var r1 = vec4f(0.0);"
        );
        for c in &self.combiners {
            // Both halves read the registers as they were.
            s.push_str("  {\n");
            let color = expression(&c.color, false)?;
            let alpha = expression(&c.alpha, true)?;
            if let Some(e) = &color {
                let _ = writeln!(s, "    let color = {e};");
            }
            if let Some(e) = &alpha {
                let _ = writeln!(s, "    let alpha = {e};");
            }
            if color.is_some() {
                let _ = writeln!(s, "    r{0} = vec4f(color, r{0}.a);", c.color.dest[0]);
            }
            if alpha.is_some() {
                let _ = writeln!(s, "    r{}.a = alpha;", c.alpha.dest[0]);
            }
            s.push_str("  }\n");
        }
        s.push_str("  return saturate(r0);\n}\n");
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_are_read_before_writing() {
        let input = |source, alpha| Input {
            source,
            map: 0,
            alpha,
        };
        let e = |operator, inputs| Expr {
            operator,
            scale: 0,
            bias: 0,
            dest: [0, 8, 8],
            inputs,
        };
        let c = Combiner {
            color: e(9, vec![input(6, true), input(7, false), input(14, false)]),
            alpha: e(1, vec![input(14, true)]),
        };
        let p = Program {
            combiners: vec![c],
            textures: vec![(Coords::Static, false), (Coords::EyeReflect, true)],
        };
        let w = p.wgsl().unwrap();
        assert!(w.contains("vec3f(tex0.a)") && w.contains("texture_cube"));
        assert!(w.find("let alpha").unwrap() < w.find("r0 = vec4f(color").unwrap());
    }
}
