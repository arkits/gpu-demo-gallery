//! Reads the demo's own files. Needs the user's copy: `TOON_DATA`, or `data/`.
use kyro_toon::data::Files;
use kyro_toon::demo::FILES;
use kyro_toon::kyro::{self, Exe};

fn files() -> Files {
    let p = std::env::var("TOON_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn scene_tables_read() {
    let exe = Exe::new(files().get("toon.exe").unwrap().to_vec()).unwrap();
    let meshes = kyro::meshes(&exe, 0x521ab0, 58);
    assert_eq!(meshes[0].name, "Bone16");
    assert_eq!(meshes[57].name, "SHADOW_FLAT_Cylinder08");
    for m in &meshes {
        assert_eq!(m.positions.len(), m.normals.len(), "{}", m.name);
        let n = m.positions.len() as u16;
        assert!(m.faces.iter().flatten().all(|&i| i < n), "{}", m.name);
    }
    let mats = kyro::materials(&exe, 0x49b640, 17);
    assert_eq!(mats[16].name, "ball");
    assert_eq!(mats[7].maps[6].as_deref(), Some("logo-foot.bmp"));
    let anims = kyro::mesh_anims(&exe, 0x5d0218, 58, 700);
    assert_eq!(anims[35].visibility.as_ref().unwrap().values.len(), 2);
    let cams = kyro::cameras(&exe, 0x49bc80, 0x5d8110, 1, 700);
    assert_eq!(cams[0].0.name, "Camera01");
    assert_eq!(cams[0].1.position.as_ref().unwrap().values.len(), 701);
}

#[test]
fn every_picture_reads() {
    let exe = Exe::new(files().get("toon.exe").unwrap().to_vec()).unwrap();
    let res = exe.resources();
    for name in [
        "toontone.bmp",
        "toonsilhouette.bmp",
        "logo-foot.bmp",
        "credits.bmp",
        "kyro2.bmp",
    ] {
        let i = kyro::picture(&res, name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{name}");
    }
    let ini = String::from_utf8_lossy(res[&("INIFILE".into(), "TOON.INI".into())]).into_owned();
    let fades = kyro::fades(&ini);
    assert_eq!(fades.len(), 3);
    assert_eq!(
        (fades[1].frame, fades[1].frames, fades[1].from, fades[1].to),
        (650, 15, 0.0, 1.0)
    );
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
