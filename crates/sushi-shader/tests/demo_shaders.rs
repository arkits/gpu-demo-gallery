//! Parses and translates every shader in the user's own copy of the demo.
use sushi_formats::{archive::Archive, object, scene, shader_header::ShaderHeader};
use sushi_shader::{parse, wgsl};

fn demo() -> Archive {
    let p = std::env::var("SUSHI_ATI").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Sushi.ati"
        )
        .into()
    });
    Archive::open(&std::fs::read(&p).unwrap_or_else(|e| panic!("need Sushi.ati at {p}: {e}")))
        .unwrap()
}

fn load(a: &Archive, name: &str) -> sushi_shader::ShaderFile {
    let path = format!("shaders/{}", name.replace('\\', "/"));
    let dir = path.rsplit_once('/').unwrap().0.to_string();
    let text = |n: &str| {
        a.get(n)
            .map(|b| b.iter().map(|&c| c as char).collect::<String>())
    };
    let include = |n: &str| text(&format!("{dir}/{n}"));
    let src = text(&path).unwrap_or_else(|| panic!("no shader {path}"));
    parse(&src, &include).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn element(h: &ShaderHeader, sem: &str, index: u32) -> Option<wgsl::ElementType> {
    h.element(sem, index).map(|e| match e.ty() {
        0 => wgsl::ElementType::Float1,
        1 => wgsl::ElementType::Float2,
        2 => wgsl::ElementType::Float3,
        3 => wgsl::ElementType::Float4,
        4 => wgsl::ElementType::Color,
        5 => wgsl::ElementType::Ubyte4,
        t => panic!("element type {t}"),
    })
}

fn validate(name: &str, src: &str) {
    let module = naga::front::wgsl::parse_str(src)
        .unwrap_or_else(|e| panic!("{name}: {}\n{src}", e.emit_to_string(src)));
    let mut v = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    );
    v.validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}\n{src}"));
}

#[test]
fn every_file_parses() {
    let a = demo();
    let names: Vec<_> = a
        .names()
        .filter(|n| n.ends_with(".ssh"))
        .map(String::from)
        .collect();
    assert_eq!(names.len(), 35);
    for n in names {
        load(&a, n.strip_prefix("shaders/").unwrap());
    }
}

#[test]
fn every_drawable_pass_translates_to_valid_wgsl() {
    let a = demo();
    let mut uses: Vec<(String, ShaderHeader)> = Vec::new();
    for n in a.names().filter(|n| n.ends_with(".so2")) {
        for m in object::read_object(a.get(n).unwrap()).unwrap().meshes {
            if !uses.iter().any(|u| u.0 == m.shader) {
                uses.push((m.shader.clone(), m.header));
            }
        }
    }
    let s = scene::read_scene(a.get("Scenes/Animusic.ati").unwrap()).unwrap();
    for sh in s.shaders {
        let name = sushi_formats::reader::cstr(&sh.raw);
        if !uses.iter().any(|u| u.0 == name) {
            uses.push((name, sh.header));
        }
    }
    assert!(uses.len() > 20);
    let mut passes = 0;
    for (name, header) in &uses {
        let file = load(&a, name);
        for shader in &file.shaders {
            if !(shader.has("Normal") || shader.has("Shadow")) {
                continue;
            }
            for (i, pass) in shader.passes.iter().enumerate() {
                let cube = |st: u32| {
                    pass.samplers.get(&st).is_some_and(|s| {
                        file.textures.iter().any(|t| t.name == s.texture && t.cube)
                    })
                };
                let m = wgsl::translate(pass, &|s, i| element(header, s, i), &cube)
                    .unwrap_or_else(|e| panic!("{name} {} pass {i}: {e}", shader.name));
                validate(&format!("{name} {} pass {i}", shader.name), &m.source);
                passes += 1;
            }
        }
    }
    println!("{passes} passes");
    assert!(passes > 60, "{passes}");
}
