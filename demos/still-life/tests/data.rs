//! Reads the demo's own files. Needs the user's copy: `STILL_DATA`, or `data/`.
use still_life::data::Files;
use still_life::demo::FILES;
use still_life::kyro::{self, Exe};

fn files() -> Files {
    let p = std::env::var("STILL_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn scene_tables_read() {
    let exe = Exe::new(files().get("still.exe").unwrap().to_vec()).unwrap();
    let meshes = kyro::meshes(&exe, 0x508f80, 34);
    assert_eq!(meshes[0].name, "Orange");
    assert_eq!(meshes[28].name, "NO_SILHOUETTE_table");
    for m in &meshes {
        let n = m.positions.len() as u16;
        assert!(m.faces.iter().flatten().all(|&i| i < n), "{}", m.name);
    }
    let mats = kyro::materials(&exe, 0x49b650, 14);
    assert_eq!(mats[13].name, "Banana");
    let lights = kyro::lights(&exe, 0x49bab0, 2);
    assert_eq!((lights[0].name.as_str(), lights[0].kind), ("Fspot01", 1));
    let anims = kyro::light_anims(&exe, 0x49f3c8, 2, 600);
    assert_eq!(anims[0].position.as_ref().unwrap().values.len(), 600);
    assert!(anims[1].position.is_none());
}

#[test]
fn hatching_pictures_have_their_mipmaps() {
    let exe = Exe::new(files().get("still.exe").unwrap().to_vec()).unwrap();
    let res = exe.resources();
    for tone in [250, 220, 190, 160, 130, 100] {
        for size in [512, 256, 128, 64, 32] {
            let name = format!("Tam_{tone}_{size}.bmp");
            let i = kyro::picture(&res, &name).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!((i.width, i.height), (size, size), "{name}");
        }
    }
    for name in [
        "overlay.bmp",
        "toonsilhouette.bmp",
        "Travertn.tga",
        "Tutash.jpg",
        "Galvplat.jpg",
    ] {
        kyro::picture(&res, name).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
