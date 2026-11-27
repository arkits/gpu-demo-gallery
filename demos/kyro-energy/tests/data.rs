//! Reads the demo's own files. Needs the user's copy: `ENERGY_DATA`, or `data/`.
use kyro_energy::data::Files;
use kyro_energy::demo::FILES;
use kyro_energy::droplets::{Rand, Sprite};
use kyro_energy::kyro::{self, Exe};

fn files() -> Files {
    let p = std::env::var("ENERGY_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn scene_tables_read() {
    let exe = Exe::new(files().get("energy.exe").unwrap().to_vec()).unwrap();
    let meshes = kyro::meshes(&exe, 0x4e6ba0, 12);
    assert_eq!(meshes[0].name, "Line01");
    assert_eq!(meshes[3].name, "STENCIL_table");
    assert_eq!(meshes[3].colors.as_ref().unwrap()[0], 0x7ff0_f0f0);
    let mats = kyro::materials(&exe, 0x49c770, 5);
    assert_eq!(mats[0].maps[8].as_deref(), Some("proc.bmp"));
    let lights = kyro::lights(&exe, 0x49c900, 3);
    assert_eq!(lights[2].name, "ANIM_Direct02");
}

#[test]
fn drop_pictures_read() {
    let exe = Exe::new(files().get("energy.exe").unwrap().to_vec()).unwrap();
    let res = exe.resources();
    for i in 1..=4 {
        let d = Sprite::height(&kyro::picture(&res, &format!("drop{i}_16x16.bmp")).unwrap());
        assert_eq!((d.w, d.h), (16, 16));
    }
    let s = kyro::picture(&res, "staticDrop.bmp").unwrap();
    assert_eq!((s.width, s.height), (8, 8));
}

#[test]
fn rand_is_visual_cpp() {
    let mut r = Rand::default();
    assert_eq!([r.rand(), r.rand(), r.rand()], [41, 18467, 6334]);
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
