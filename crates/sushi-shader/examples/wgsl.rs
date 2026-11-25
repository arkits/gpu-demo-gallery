//! Prints the translation of a shader: `wgsl <Sushi.ati> <file> <shader>`.
use sushi_formats::archive::Archive;
use sushi_shader::{parse, wgsl};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, name, shader] = args.as_slice() else {
        return Err("usage: wgsl <Sushi.ati> <file> <shader>".into());
    };
    let archive = Archive::open(&std::fs::read(data)?)?;
    let text = |n: &str| {
        let bytes = archive.get(n)?;
        Some(bytes.iter().map(|&c| c as char).collect::<String>())
    };
    let dir = name.rsplit_once('/').ok_or("the file needs its folder")?.0;
    let include =
        |n: &str| text(&format!("{dir}/{n}")).or_else(|| text(&format!("shaders/includes/{n}")));
    let file = parse(&text(name).ok_or("no such file")?, &include)?;
    for s in file.shaders.iter().filter(|s| s.name == *shader) {
        for pass in &s.passes {
            let setting = wgsl::Setting {
                element: &|s, _| {
                    Some(match s {
                        "blendindices" => wgsl::ElementType::Ubyte4,
                        "color" => wgsl::ElementType::Color,
                        "texcoord" => wgsl::ElementType::Float2,
                        "blendweight" => wgsl::ElementType::Float4,
                        _ => wgsl::ElementType::Float3,
                    })
                },
                unfiltered: &|_| false,
                sampler: &|n| file.samplers.get(n).cloned(),
                depth_only: false,
            };
            let module = wgsl::translate(pass, &setting)?;
            println!("{}", module.source);
        }
    }
    Ok(())
}
