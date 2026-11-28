//! Reads the demo's own files. Needs the user's copy: `ARK_DATA`, or `data/`.
use radeons_ark::ati::World;
use radeons_ark::data::Files;
use radeons_ark::path::Path;
use radeons_ark::proteus::{Animation, Object};

fn files() -> Files {
    let p = std::env::var("ARK_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

/// The world is read to its last byte, as the program reads it.
#[test]
fn world_reads_whole() {
    let w = World::read(files().get("radiation.ati").unwrap()).unwrap();
    assert_eq!(w.rooms.len(), 33);
    assert_eq!(w.mirrors.len(), 21);
    assert!(w.mirrors.iter().all(|m| m.rooms.len() == 33));
    assert_eq!(w.textures.len(), 346);
    assert_eq!(w.textures[4].name, "..\\caustic.bmp");
    assert_eq!(
        w.rooms[0].bounds(),
        ([-250.0, -250.0, -8.0], [250.0, 250.0, 252.0])
    );
    for r in &w.rooms {
        // Every bucket's command list ends.
        for b in 0..radeons_ark::ati::BUCKETS {
            if !r.vertices(b).is_empty() {
                assert!(r.commands(b).contains(&5));
            }
        }
    }
}

/// The flight: 253 points, 29 cuts, so 224 seconds, the music's length.
#[test]
fn flight_path() {
    let p = Path::parse(&String::from_utf8_lossy(files().get("fly.pth").unwrap())).unwrap();
    assert_eq!(p.points.len(), 253);
    assert_eq!(p.length(), 224.0);
    assert!(!p.loops());
    // The spline passes through its points.
    let (at, _) = p.at(0.0).unwrap();
    assert!((at - p.points[0].0).length() < 1e-2, "{at}");
}

#[test]
fn characters_read() {
    let f = files();
    let o = Object::read(f.get("radeon7.obj").unwrap()).unwrap();
    assert_eq!(o.name, "Noelle");
    assert_eq!(o.bones.len(), 49);
    assert_eq!(o.meshes.len(), 69);
    let a = Animation::read(f.get("radeon7.pan").unwrap()).unwrap();
    assert_eq!(
        (a.length, a.root.len(), a.bones.len(), a.events.len()),
        (228.0, 3421, 49, 9)
    );
    // She teleports three times; the jumps in her place all fall while she is gone.
    assert_eq!(a.event(30.0), Some((0, 0.5)));
    assert_eq!(a.event(31.5).map(|e| e.0), Some(2));
    assert_eq!(a.event(33.0), Some((1, 0.5)));
    assert_eq!(a.event(40.0), None);
    let step = a.length / (a.root.len() - 1) as f32;
    for i in 1..a.root.len() {
        if (a.root[i].w_axis - a.root[i - 1].w_axis).length() > 200.0 {
            let mid = (i as f32 - 0.5) * step;
            assert_eq!(a.event(mid).map(|e| e.0), Some(2), "jump at {mid}");
        }
    }
    for name in f.names().into_iter().filter(|n| n.ends_with(".obj")) {
        Object::read(f.get(&name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    for name in f.names().into_iter().filter(|n| n.ends_with(".pan")) {
        Animation::read(f.get(&name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in f.names().iter().filter(|n| !n.starts_with("music.")) {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
