//! Parses every shader file in the user's own copy of the demo and translates every pass.
use sushi_formats::archive::Archive;
use sushi_shader::{parse, wgsl};

fn demo() -> Option<Archive> {
    let provided = std::env::var_os("SUSHI_ATI");
    let default = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/Sushi.ati");
    match std::fs::read(provided.as_deref().unwrap_or(default.as_ref())) {
        Ok(data) => Some(Archive::open(&data).expect("open Sushi.ati")),
        Err(e) if provided.is_none() && e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("read Sushi.ati: {e}"),
    }
}

#[test]
fn every_pass_becomes_valid_wgsl() {
    let Some(a) = demo() else { return };
    let text = |n: &str| {
        a.get(n)
            .map(|b| b.iter().map(|&c| c as char).collect::<String>())
    };
    let (mut files, mut passes, mut bad) = (0, 0, Vec::new());
    for name in a
        .names()
        .filter(|n| n.ends_with(".ssh") && !n.contains("/global/"))
    {
        let dir = name.rsplit_once('/').unwrap().0;
        let include = |n: &str| {
            text(&format!("{dir}/{n}")).or_else(|| text(&format!("shaders/includes/{n}")))
        };
        let f = match parse(&text(name).unwrap(), &include) {
            Ok(f) => f,
            Err(e) => {
                bad.push(format!("{name}: {e}"));
                continue;
            }
        };
        files += 1;
        for s in &f.shaders {
            for (k, p) in s.passes.iter().enumerate() {
                let element = |s: &str, _: u32| {
                    Some(match s {
                        "blendindices" => wgsl::ElementType::Ubyte4,
                        "color" => wgsl::ElementType::Color,
                        "texcoord" => wgsl::ElementType::Float2,
                        "blendweight" => wgsl::ElementType::Float4,
                        _ => wgsl::ElementType::Float3,
                    })
                };
                let setting = wgsl::Setting {
                    element: &element,
                    unfiltered: &|_| false,
                    sampler: &|n| f.samplers.get(n).cloned(),
                    depth_only: false,
                };
                passes += 1;
                let at = format!("{name} {} pass {k}", s.name);
                let m = match wgsl::translate(p, &setting) {
                    Ok(m) => m,
                    Err(e) => {
                        bad.push(format!("{at}: {e}"));
                        continue;
                    }
                };
                let module = match naga::front::wgsl::parse_str(&m.source) {
                    Ok(x) => x,
                    Err(e) => {
                        bad.push(format!("{at}: {}", e.emit_to_string(&m.source)));
                        if std::env::var("DUMP").is_ok() {
                            println!("{}", m.source);
                        }
                        continue;
                    }
                };
                let mut v = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                if let Err(e) = v.validate(&module) {
                    bad.push(format!("{at}: {}", e.emit_to_string(&m.source)));
                    if std::env::var("DUMP").is_ok() {
                        println!("{}", m.source);
                    }
                }
            }
        }
    }
    println!("{files} files, {passes} passes, {} bad", bad.len());
    let mut kinds = std::collections::BTreeMap::new();
    for b in &bad {
        let k = b.splitn(2, ": ").nth(1).unwrap_or("").to_string();
        kinds.entry(k).or_insert_with(Vec::new).push(b.clone());
    }
    for (k, v) in &kinds {
        println!(
            "{}x {}\n   e.g. {}",
            v.len(),
            k,
            v[0].lines().next().unwrap()
        );
    }
    assert!(files > 0 && passes > 0);
    // The legacy Radeon 9700 wireframe variants contain fixed-function passes.
    assert!(
        bad.iter()
            .all(|pass| pass.starts_with("shaders/ruby3/gwireframe.ssh ")
                && pass.ends_with(": no vertex shader"))
    );
}
