//! Dumps .X files: `xdump FILE...` (frames, meshes, key times).
use parhelia_reef::xfile::Model;

fn main() {
    for f in std::env::args().skip(1) {
        let d = std::fs::read(&f).expect("read");
        match Model::read(&d) {
            Ok(m) => {
                println!(
                    "{f}: {} frames, {} meshes, {} tracks",
                    m.frames.len(),
                    m.meshes.len(),
                    m.tracks.len()
                );
                for x in &m.meshes {
                    println!(
                        "  mesh {} in {}: {} v {} t {} uv {} n {} mats {} skins",
                        x.name,
                        x.frame,
                        x.positions.len(),
                        x.triangles.len(),
                        x.uvs.len(),
                        x.normals.len(),
                        x.materials.len(),
                        x.skins.len()
                    );
                }
                for t in m.tracks.iter().take(4) {
                    let times = |v: Vec<f32>| {
                        v.first()
                            .zip(v.last())
                            .map(|(a, b)| format!("{a}..{b} ({})", v.len()))
                            .unwrap_or_default()
                    };
                    println!(
                        "  track {}: rot {} pos {} scale {} mat {}",
                        t.frame,
                        times(t.rotation.iter().map(|k| k.0).collect()),
                        times(t.position.iter().map(|k| k.0).collect()),
                        times(t.scale.iter().map(|k| k.0).collect()),
                        times(t.matrix.iter().map(|k| k.0).collect())
                    );
                }
            }
            Err(e) => println!("{f}: ERROR {e}"),
        }
    }
}
