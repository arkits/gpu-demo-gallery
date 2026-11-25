//! Turns the scenes' pixel shaders into WGSL: the texture shader stages and register
//! combiners of the GeForce 3 and 4. Registers hold -1 to 1, textures and colors 0 to 1.
//! The numbers are the viewers' own; `docs/ENGINE.md` says where they come from.
use std::fmt::Write;

use crate::Error;
use crate::pass::COLORS;
use crate::tree::Node;

#[derive(Debug, Clone)]
struct Input {
    source: u32,
    map: u32,
    other: bool,
}

#[derive(Debug, Clone)]
struct Expr {
    operator: u32,
    scale: u32,
    bias: u32,
    dest: [u32; 2],
    inputs: Vec<Input>,
}

#[derive(Debug, Clone)]
struct Combiner {
    color: Expr,
    alpha: Expr,
    constants: [[f32; 4]; 2],
}

/// What one texture unit does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Samples at the interpolated coordinates.
    Plain,
    PassThrough,
    /// A dot product of the coordinates and an earlier texture.
    Dot {
        input: usize,
        expand: bool,
    },
    /// Samples a 2D texture at the last two dot products.
    Dot2D {
        input: usize,
        expand: bool,
    },
    /// Samples a 1D texture at this dot product.
    Dot1D {
        input: usize,
        expand: bool,
    },
    /// Puts the quotient of the last two dot products into the depth buffer.
    DotDepth {
        input: usize,
        expand: bool,
    },
    /// Samples a cube map at the last three dot products.
    DotCube {
        input: usize,
        expand: bool,
    },
    /// The same, at the eye vector reflected there. The eye is in the coordinates' w.
    DotReflect {
        input: usize,
        expand: bool,
    },
}

/// How a pass treats fog: the color, where it starts and ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    pub color: [f32; 4],
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Shader {
    stages: Vec<Stage>,
    combiners: Vec<Combiner>,
    /// The program sets the constant colors, see `pass::COLORS`.
    dynamic: bool,
}

const DISCARD: u32 = 12;

impl Expr {
    fn read(n: &Node) -> Self {
        let input = |i: &Node| Input {
            source: i.int("source"),
            map: i.int("map"),
            other: i.bool("otherComponent", false),
        };
        Self {
            operator: n.int("operator"),
            scale: n.int("outputScale"),
            bias: n.int("outputBias"),
            dest: [n.int("dest0"), n.int("dest1")],
            inputs: n.nodes("inputs").iter().map(input).collect(),
        }
    }
}

impl Shader {
    pub fn read(n: &Node, dynamic: bool) -> Result<Self, Error> {
        let mut stages = Vec::new();
        for e in n.nodes("textureExpr") {
            let (input, expand) = (e.int("input") as usize, e.bool("expandRGBA", false));
            let dot = Stage::Dot { input, expand };
            match e.int("expr") {
                0 => stages.push(Stage::Plain),
                1 => stages.push(Stage::PassThrough),
                8 => stages.extend([dot, Stage::DotDepth { input, expand }]),
                9 => stages.extend([dot, Stage::Dot2D { input, expand }]),
                11 => stages.extend([dot, dot, Stage::DotCube { input, expand }]),
                12 => stages.extend([dot, dot, Stage::DotReflect { input, expand }]),
                16 => stages.push(Stage::Dot1D { input, expand }),
                x => return Err(format!("unsupported texture expression {x}").into()),
            }
        }
        if stages.len() > 4 {
            return Err("too many texture stages".into());
        }
        let combiner = |c: &Node| -> Result<Combiner, Error> {
            let constant = |f| c.vec::<4>(f).unwrap_or([0.0; 4]);
            Ok(Combiner {
                color: Expr::read(c.node("colorExpr").ok_or("combiner without color")?),
                alpha: Expr::read(c.node("alphaExpr").ok_or("combiner without alpha")?),
                constants: [constant("constColor0"), constant("constColor1")],
            })
        };
        Ok(Self {
            stages,
            combiners: n
                .nodes("combiners")
                .iter()
                .map(combiner)
                .collect::<Result<_, _>>()?,
            dynamic,
        })
    }

    pub fn writes_depth(&self) -> bool {
        self.stages
            .iter()
            .any(|s| matches!(s, Stage::DotDepth { .. }))
    }

    fn stage(&self, i: usize) -> Stage {
        self.stages.get(i).copied().unwrap_or(Stage::Plain)
    }

    /// `fn shade(s: Varyings) -> vec4f`. `units` says which units have a texture and
    /// whether it is a cube map.
    pub fn wgsl(
        &self,
        units: [Option<bool>; 4],
        shadows: [bool; 4],
        fog: Option<Fog>,
    ) -> Result<String, Error> {
        let mut s = String::from("var<private> depth: f32;\nfn shade(s: Varyings) -> vec4f {\n");
        let coords = ["s.tex0", "s.tex1", "s.tex2", "s.tex3"];
        for (i, at) in coords.iter().enumerate() {
            let sample = |uv: String| format!("textureSample(map{i}, sampler{i}, {uv})");
            let plane = |uv: &str| match units[i] {
                Some(false) => sample(uv.to_owned()),
                _ => "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned(),
            };
            let cube = |dir: &str| match units[i] {
                Some(true) => sample(dir.to_owned()),
                _ => "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned(),
            };
            let dot = |input: usize, expand: bool| {
                let source = match expand {
                    true => format!("(2.0 * tex{input}.rgb - 1.0)"),
                    false => format!("tex{input}.rgb"),
                };
                format!("let d{i} = dot({at}.xyz, {source});")
            };
            let early = |input: usize, n: usize| match input < i && i >= n {
                true => Ok(()),
                false => Err(Error::from("texture stage depends on a later one")),
            };
            let value = match self.stage(i) {
                Stage::Plain if shadows[i] => format!("lit_by(map{i}, {at})"),
                Stage::Plain => match units[i] {
                    Some(true) => cube(&format!("{at}.xyz")),
                    _ => plane(&format!("project({at})")),
                },
                Stage::PassThrough => format!("saturate({at})"),
                Stage::Dot { input, expand } => {
                    early(input, 0)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned()
                }
                Stage::Dot2D { input, expand } => {
                    early(input, 1)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    plane(&format!("vec2f(d{}, d{i})", i - 1))
                }
                Stage::DotDepth { input, expand } => {
                    early(input, 1)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    let _ = writeln!(s, "  depth = saturate(d{} / d{i});", i - 1);
                    "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned()
                }
                Stage::Dot1D { input, expand } => {
                    early(input, 0)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    plane(&format!("vec2f(d{i}, 0.5)"))
                }
                Stage::DotCube { input, expand } => {
                    early(input, 2)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    cube(&format!("vec3f(d{}, d{}, d{i})", i - 2, i - 1))
                }
                Stage::DotReflect { input, expand } => {
                    early(input, 2)?;
                    let _ = writeln!(s, "  {}", dot(input, expand));
                    let _ = writeln!(
                        s,
                        "  let n{i} = vec3f(d{0}, d{1}, d{i});\n  \
                         let e{i} = vec3f({2}.w, {3}.w, {at}.w);\n  \
                         let l{i} = 2.0 * dot(n{i}, e{i}) / max(dot(n{i}, n{i}), 1e-6) * n{i} - e{i};",
                        i - 2,
                        i - 1,
                        coords[i - 2],
                        coords[i - 1],
                    );
                    cube(&format!("l{i}"))
                }
            };
            let _ = writeln!(s, "  var tex{i} = {value};");
        }
        s.push_str("  var primary = saturate(s.col0);\n  var secondary = saturate(s.col1);\n");
        s.push_str("  var spare0 = vec4f(0.0, 0.0, 0.0, tex0.a);\n  var spare1 = vec4f(0.0);\n");
        match fog {
            Some(f) => {
                let [r, g, b, _] = f.color;
                let range = f.end - f.start;
                let _ = writeln!(
                    s,
                    "  let fog = vec4f({r:?}, {g:?}, {b:?}, saturate(({:?} - s.fog) / {:?}));",
                    f.end,
                    if range == 0.0 { 1.0 } else { range },
                );
            }
            None => s.push_str("  let fog = vec4f(0.0, 0.0, 0.0, 1.0);\n"),
        }
        for (k, c) in self.combiners.iter().enumerate() {
            let lit =
                |v: [f32; 4]| format!("vec4f({:?}, {:?}, {:?}, {:?})", v[0], v[1], v[2], v[3]);
            let _ = writeln!(s, "  // combiner {k}\n  {{");
            let [c0, c1] = c.constants;
            let [c0, c1] = match self.dynamic {
                true => [COLORS, COLORS + 1].map(|i| format!("draw.c[{i}]")),
                false => [lit(c0), lit(c1)],
            };
            let _ = writeln!(s, "    let const0 = {c0};\n    let const1 = {c1};");
            // Both halves read the registers as they were.
            let color = products(&c.color, false).map_err(|e| format!("combiner {k}: {e}"))?;
            let alpha = products(&c.alpha, true).map_err(|e| format!("combiner {k}: {e}"))?;
            for (i, (value, _)) in color.iter().enumerate() {
                let _ = writeln!(s, "    let color{i} = {value};");
            }
            for (i, (value, _)) in alpha.iter().enumerate() {
                let _ = writeln!(s, "    let alpha{i} = {value};");
            }
            for (i, (_, dest)) in color.iter().enumerate() {
                let d = destination(*dest)?;
                let _ = writeln!(s, "    {d} = vec4f(color{i}, {d}.a);");
            }
            for (i, (_, dest)) in alpha.iter().enumerate() {
                let _ = writeln!(s, "    {}.a = alpha{i};", destination(*dest)?);
            }
            s.push_str("  }\n");
        }
        s.push_str("  return saturate(spare0);\n}\n");
        Ok(s)
    }
}

fn destination(n: u32) -> Result<&'static str, Error> {
    Ok(match n {
        0 => "primary",
        1 => "secondary",
        2 => "tex0",
        3 => "tex1",
        4 => "tex2",
        5 => "tex3",
        10 => "spare0",
        11 => "spare1",
        n => return Err(format!("unsupported combiner destination {n}").into()),
    })
}

/// One operand: a color, or an alpha spread over a color.
fn operand(i: &Input, for_alpha: bool) -> Result<String, Error> {
    let name = match i.source {
        0 => "vec4f(0.0)",
        1 => "const0",
        2 => "const1",
        3 => "fog",
        4 => "primary",
        5 => "secondary",
        6 => "tex0",
        7 => "tex1",
        8 => "tex2",
        9 => "tex3",
        14 => "spare0",
        15 => "spare1",
        n => return Err(format!("unsupported source {n}").into()),
    };
    let value = match (for_alpha, i.other) {
        (false, false) => format!("{name}.rgb"),
        (false, true) => format!("vec3f({name}.a)"),
        (true, false) => format!("{name}.a"),
        // Where an alpha is wanted, a color gives its blue.
        (true, true) => format!("{name}.b"),
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
        n => return Err(format!("unsupported input mapping {n}").into()),
    })
}

/// The values an expression writes, with their destinations.
fn products(e: &Expr, for_alpha: bool) -> Result<Vec<(String, u32)>, Error> {
    let x = e
        .inputs
        .iter()
        .map(|i| operand(i, for_alpha))
        .collect::<Result<Vec<_>, _>>()?;
    let dot = |a: &str, b: &str| format!("vec3f(dot({a}, {b}))");
    let [first, second] = e.dest;
    // Some list more inputs than they use.
    let used = match e.operator {
        0 => 0,
        1 => 1,
        2..=4 => 2,
        5..=7 => 3,
        _ => 4,
    };
    let x: Vec<&str> = x.iter().take(used).map(String::as_str).collect();
    let values = match (e.operator, x.as_slice()) {
        (0, _) => vec![(
            if for_alpha { "0.0" } else { "vec3f(0.0)" }.to_owned(),
            first,
        )],
        (1, [a]) => vec![(format!("({a})"), first)],
        (2, [a, b]) => vec![(format!("({a} * {b})"), first)],
        (3, [a, b]) if !for_alpha => vec![(dot(a, b), first)],
        (4, [a, b]) => vec![(format!("({a} + {b})"), first)],
        (5, [a, b, c]) => vec![(format!("({a} * {b} + {c})"), first)],
        (6, [a, b, c]) => vec![(format!("({a} * {b} + {a} * {c})"), first)],
        // A blend by the first operand.
        (7, [a, b, c]) => vec![(format!("({a} * {b} + (1.0 - {a}) * {c})"), first)],
        (8, [a, b, c, d]) => vec![(format!("({a} * {b} + {c} * {d})"), first)],
        (9, [a, b, c, d]) => {
            let pick = format!("select({a} * {b}, {c} * {d}, spare0.a >= 0.5)");
            vec![(pick, first)]
        }
        (10, [a, b, c, d]) => {
            vec![
                (format!("({a} * {b})"), first),
                (format!("({c} * {d})"), second),
            ]
        }
        (11, [a, b, c, d]) if !for_alpha => vec![(dot(a, b), first), (dot(c, d), second)],
        (12, [a, b, c, d]) if !for_alpha => {
            vec![(dot(a, b), first), (format!("({c} * {d})"), second)]
        }
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
    let one = if for_alpha { "1.0" } else { "vec3f(1.0)" };
    Ok(values
        .into_iter()
        .filter(|(_, d)| *d != DISCARD)
        .map(|(v, d)| (format!("clamp(({v}{bias}) * {scale}, -{one}, {one})"), d))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source: u32, map: u32, other: bool) -> Input {
        Input { source, map, other }
    }

    fn expr(operator: u32, dest: [u32; 2], inputs: Vec<Input>) -> Expr {
        Expr {
            operator,
            scale: 0,
            bias: 0,
            dest,
            inputs,
        }
    }

    #[test]
    fn registers_are_read_before_writing() {
        let c = Combiner {
            color: expr(
                8,
                [10, 12],
                vec![
                    input(6, 0, false),
                    input(14, 0, false),
                    input(9, 2, false),
                    input(14, 0, true),
                ],
            ),
            alpha: expr(1, [10, 12], vec![input(14, 0, true)]),
            constants: [[0.0; 4]; 2],
        };
        let shader = Shader {
            stages: vec![Stage::Plain],
            combiners: vec![c],
            dynamic: false,
        };
        let w = shader
            .wgsl([Some(false), None, None, Some(true)], [false; 4], None)
            .unwrap();
        assert!(w.contains("vec3f(spare0.a)") && w.contains("max(spare0.b, 0.0)"));
        assert!(w.find("let alpha0").unwrap() < w.find("spare0 = vec4f(color0").unwrap());
    }

    #[test]
    fn two_products_have_two_destinations() {
        let x = || input(6, 0, false);
        let p = products(&expr(10, [10, 11], vec![x(), x(), x(), x()]), false).unwrap();
        assert_eq!(p.iter().map(|p| p.1).collect::<Vec<_>>(), [10, 11]);
        let p = products(&expr(1, [12, 12], vec![x()]), true).unwrap();
        assert!(p.is_empty());
        assert!(products(&expr(3, [10, 12], vec![x(), x()]), true).is_err());
    }
}
