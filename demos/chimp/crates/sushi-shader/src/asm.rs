//! D3D9 shader assembly: vs.1.1, vs.2.0 (without flow control), ps.1.4 and ps.2.0.
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Bank {
    Temp,
    Input,
    Const,
    Texcoord,
    Sampler,
    Address,
    OutPosition,
    OutTexcoord,
    OutColor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operand {
    pub bank: Bank,
    pub index: u32,
    /// `c[a0.x + index]`.
    pub relative: bool,
    /// Write mask or swizzle, as component numbers; empty when absent.
    pub comps: Vec<u8>,
    pub negate: bool,
    /// ps.1.4 `1-r0`.
    pub complement: bool,
    /// ps.1.4 `_dw`: divide x and y by w.
    pub divide_w: bool,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub op: String,
    pub saturate: bool,
    pub scale: f32,
    pub dest: Operand,
    pub src: Vec<Operand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Pixel14,
    Pixel20,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub stage: Stage,
    /// Vertex inputs: semantic, index, register.
    pub inputs: Vec<(String, u32, u32)>,
    pub defs: Vec<(u32, [f32; 4])>,
    /// Samplers declared as cube maps.
    pub cubes: Vec<u32>,
    pub code: Vec<Instruction>,
}

fn component(c: char) -> Result<u8> {
    Ok(match c {
        'x' | 'r' => 0,
        'y' | 'g' => 1,
        'z' | 'b' => 2,
        'w' | 'a' => 3,
        _ => return Err(format!("bad component {c}")),
    })
}

pub fn operand(s: &str) -> Result<Operand> {
    let s = s.trim();
    let (complement, s) = s
        .strip_prefix("1-")
        .map_or((false, s), |r| (true, r.trim()));
    let (negate, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r.trim()));
    let mut o = Operand {
        bank: Bank::Temp,
        index: 0,
        relative: false,
        comps: Vec::new(),
        negate,
        complement,
        divide_w: false,
    };
    let (reg, suffix) = if let Some((head, tail)) = s.split_once(']') {
        // c[a0.x + n]
        let inner = head.split_once('[').ok_or(format!("bad operand {s}"))?.1;
        o.relative = true;
        for part in inner.split('+').map(str::trim) {
            if part != "a0.x" {
                o.index = part.parse().map_err(|_| format!("bad offset {s}"))?;
            }
        }
        ("c", tail.strip_prefix('.').unwrap_or(""))
    } else {
        s.split_once('.').unwrap_or((s, ""))
    };
    let reg = match reg.strip_suffix("_dw") {
        Some(r) => {
            o.divide_w = true;
            r
        }
        None => reg,
    };
    o.comps = suffix.chars().map(component).collect::<Result<_>>()?;
    let number = |p: &str| {
        reg[p.len()..]
            .parse::<u32>()
            .map_err(|_| format!("bad register {s}"))
    };
    if o.relative {
        o.bank = Bank::Const;
        return Ok(o);
    }
    (o.bank, o.index) = match reg {
        "oPos" => (Bank::OutPosition, 0),
        r if r.starts_with("oT") => (Bank::OutTexcoord, number("oT")?),
        r if r.starts_with("oD") => (Bank::OutColor, number("oD")?),
        r if r.starts_with("oC") => (Bank::OutColor, number("oC")?),
        r if r.starts_with('r') => (Bank::Temp, number("r")?),
        r if r.starts_with('v') => (Bank::Input, number("v")?),
        r if r.starts_with('c') => (Bank::Const, number("c")?),
        r if r.starts_with('t') => (Bank::Texcoord, number("t")?),
        r if r.starts_with('s') => (Bank::Sampler, number("s")?),
        r if r.starts_with('a') => (Bank::Address, number("a")?),
        _ => return Err(format!("unknown register {s}")),
    };
    Ok(o)
}

pub fn parse(src: &str) -> Result<Program> {
    let mut p: Option<Program> = None;
    // `;` also starts a comment in D3D assembly.
    let code = src
        .lines()
        .map(|l| l.split(';').next().unwrap_or("").trim());
    for line in code.filter(|l| !l.is_empty()) {
        let (head, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let args: Vec<&str> = rest
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .collect();
        let Some(p) = p.as_mut() else {
            let stage = match head {
                // The vertex shaders 2.0 seen here use only 1.1's instructions and mova.
                "vs.1.1" | "vs.2.0" => Stage::Vertex,
                "ps.1.4" => Stage::Pixel14,
                "ps.2.0" => Stage::Pixel20,
                _ => return Err(format!("unsupported version {head}")),
            };
            p = Some(Program {
                stage,
                inputs: Vec::new(),
                defs: Vec::new(),
                cubes: Vec::new(),
                code: Vec::new(),
            });
            continue;
        };
        one(p, head, &args).map_err(|e| format!("{line:?}: {e}"))?;
    }
    p.ok_or("empty shader".into())
}

fn one(p: &mut Program, head: &str, args: &[&str]) -> Result<()> {
    let arg = |i: usize| args.get(i).copied().ok_or("missing operand".to_string());
    if head == "def" {
        let mut v = [0.0; 4];
        for (i, v) in v.iter_mut().enumerate() {
            let text = arg(i + 1)?;
            let text = text.strip_suffix('f').unwrap_or(text);
            *v = text.parse().map_err(|_| "bad def value")?;
        }
        p.defs.push((operand(arg(0)?)?.index, v));
        return Ok(());
    }
    if let Some(sem) = head.strip_prefix("dcl") {
        let reg = operand(arg(0)?)?;
        match (p.stage, reg.bank, sem) {
            (_, Bank::Sampler, "_cube") => p.cubes.push(reg.index),
            (_, Bank::Sampler, _) => {}
            (Stage::Vertex, _, sem) => {
                let sem = sem.trim_start_matches('_');
                let name = sem.trim_end_matches(|c: char| c.is_ascii_digit());
                let index = sem[name.len()..].parse().unwrap_or(0);
                p.inputs.push((name.into(), index, reg.index));
            }
            _ => {}
        }
        return Ok(());
    }
    if head == "phase" {
        return Ok(());
    }
    // Coissue only affects scheduling.
    let mut parts = head.trim_start_matches('+').split('_');
    let mut i = Instruction {
        op: parts.next().unwrap_or("").into(),
        saturate: false,
        scale: 1.0,
        dest: operand(arg(0)?)?,
        src: args[1..]
            .iter()
            .map(|a| operand(a))
            .collect::<Result<_>>()?,
    };
    for m in parts {
        match m {
            "sat" => i.saturate = true,
            "x2" => i.scale = 2.0,
            "x4" => i.scale = 4.0,
            "x8" => i.scale = 8.0,
            "d2" => i.scale = 0.5,
            _ => return Err(format!("unknown modifier {m}")),
        }
    }
    p.code.push(i);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operands() {
        let o = operand("-r1.xyzz").unwrap();
        assert!(o.negate && o.bank == Bank::Temp && o.comps == [0, 1, 2, 2]);
        let o = operand("c[a0.x+2]").unwrap();
        assert!(o.relative && o.index == 2);
        let o = operand("t2_dw.xyw").unwrap();
        assert!(o.divide_w && o.bank == Bank::Texcoord && o.comps == [0, 1, 3]);
        assert!(operand("1-r0.a").unwrap().complement);
        assert!(operand("q0").is_err());
    }

    #[test]
    fn modifiers_and_coissue() {
        let p = parse("ps.1.4\nphase\ndp3_x4 r0.y, r1, r1\n+mov_sat r0.a, c0.a").unwrap();
        assert_eq!(p.code[0].scale, 4.0);
        assert!(p.code[1].saturate && p.code[1].op == "mov");
    }
}
