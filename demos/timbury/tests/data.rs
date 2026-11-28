//! Validates the assets when the extracted demo is available.
use timbury::{data::Files, demo, model::Geometry, tree};

fn files() -> Option<Files> {
    let path = std::env::var_os(demo::DATA_ENV)?;
    Some(Files::from_dir(std::path::Path::new(&path)).unwrap())
}

fn binary(files: &Files, name: &str) -> tree::Node {
    tree::read_binary(files.get(name).unwrap())
        .map_err(|e| format!("{name}: {e}"))
        .unwrap()
}

fn text(files: &Files, name: &str) -> tree::Node {
    let data = files.get(name).unwrap();
    tree::read_text(&String::from_utf8_lossy(data))
        .map_err(|e| format!("{name}: {e}"))
        .unwrap()
}

#[test]
fn assets_parse_and_round_trip() {
    let Some(files) = files() else { return };
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for path in files.needed().unwrap() {
        assert_eq!(
            files.get(&path).unwrap(),
            packed.get(&path).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn binaries_read_to_the_last_byte() {
    let Some(files) = files() else { return };
    let timbury = binary(&files, demo::TIMBURY);
    let counts: std::collections::HashMap<_, _> = [
        ("lights", 1),
        ("cameras", 5),
        ("shaders", 70),
        ("textures", 168),
        ("renderParams", 10),
        ("renderPasses", 6),
        ("transforms", 21),
        ("shapes", 6),
        ("geometries", 201),
    ]
    .into_iter()
    .collect();
    for (field, count) in counts {
        assert_eq!(timbury.nodes(field).len(), count, "{field}");
    }
    let animation = binary(&files, demo::ANIMATION);
    assert_eq!(animation.nodes("animations").len(), 1);
    let channels = &animation.nodes("animations")[0].nodes("channels");
    assert_eq!(channels.len(), 350);
}

#[test]
fn texts_read_to_the_last_word() {
    let Some(files) = files() else { return };
    for name in [
        demo::HAIR,
        demo::GRASS,
        demo::BLUR,
        demo::BACKGROUND,
        demo::MENU,
        demo::SWARMBUG,
        demo::SWARMBUG_ANIM,
        demo::GRASS_TERRAIN,
    ] {
        let root = text(&files, name);
        assert_eq!(root.kind, "Scene", "{name}");
    }
    // The swarm bug model has two wings and a combined body.
    let swarm = text(&files, demo::SWARMBUG);
    assert_eq!(swarm.nodes("shapes")[0].nodes("children").len(), 3);
}

#[test]
fn the_timbury_rests_as_modelled_and_bends() {
    let Some(files) = files() else { return };
    let root = binary(&files, demo::TIMBURY);
    let mut timbury = timbury::model::Model::new(&root, &files).unwrap();
    let anim = binary(&files, demo::ANIMATION);
    let anim = &anim.nodes("animations")[0];
    timbury.add_animation(timbury::anim::Animation::read(anim).unwrap());
    assert_eq!(timbury.shapes.len(), 404);
    let main = timbury.shapes_in(Some("nvPrimaryRenderPass_group"));
    assert_eq!(main.len(), 200);
    let rest = timbury.pose(None, &[]);
    for &k in &main {
        let shape = &timbury.shapes[k];
        let Geometry::Mesh(mesh) = &shape.geometry else {
            continue;
        };
        for (bone, (_, bends)) in timbury.bones(shape, &rest).iter().zip(&shape.bones) {
            let off = *bone - glam::Mat4::IDENTITY;
            let most = off
                .to_cols_array()
                .iter()
                .fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!(!bends || most < 1e-3, "{} {most}", shape.name);
        }
        for f in &mesh.fixed {
            let sum: f32 = f.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-3, "{}", shape.name);
        }
    }
    // The character has morph targets driven by the animation.
    let morphs: usize = timbury
        .shapes
        .iter()
        .map(|s| match &s.geometry {
            Geometry::Mesh(m) => m.morphs.len(),
            _ => 0,
        })
        .sum();
    assert!(morphs > 200, "{morphs}");
    let dance = &timbury.animations[0];
    assert!(!dance.numbers.is_empty());
    for i in 0..2000 {
        for m in timbury.pose(Some((dance, f64::from(i) * 0.0125)), &[]) {
            assert!(m.is_finite(), "{i}");
        }
    }
    assert!(dance.numbers.iter().all(|n| n.curve.at(5.0).is_finite()));
}

#[test]
fn the_hair_is_as_dense_as_asked() {
    let Some(files) = files() else { return };
    let root = text(&files, demo::HAIR);
    let hair = timbury::model::Model::new(&root, &files).unwrap();
    let strands = hair.shapes.iter().find_map(|s| match &s.geometry {
        Geometry::Hair(h) => Some(h),
        _ => None,
    });
    let strands = strands.unwrap();
    assert_eq!(strands.per_hair, 6);
    assert!(!strands.points.is_empty());
    assert_eq!(strands.points.len() % strands.per_hair, 0);
}

#[test]
fn the_swarm_flaps() {
    let Some(files) = files() else { return };
    let root = text(&files, demo::SWARMBUG);
    let swarm = timbury::model::Model::new(&root, &files).unwrap();
    assert_eq!(swarm.shapes.len(), 3);
    let anim = text(&files, demo::SWARMBUG_ANIM);
    let anim = &anim.nodes("animations")[0];
    let anim = timbury::anim::Animation::read(anim).unwrap();
    assert_eq!(anim.channels.len(), 2);
    for c in &anim.channels {
        for curve in c.curves.iter().flatten() {
            // The wingbeat loops every second.
            assert!((curve.at(0.0) - curve.at(2.0)).abs() < 1e-3);
            assert!(curve.at(0.5).is_finite());
        }
    }
}

#[test]
fn the_sky_and_cubes_decode() {
    let Some(files) = files() else { return };
    for (name, w, h) in [
        ("textures/bkg_xpos.exr", 512, 512),
        ("textures/bkg_yneg.exr", 512, 512),
        ("textures/envdiffusecubemap.exr", 384, 512),
        ("textures/envspecular_128_cubemap.exr", 192, 256),
    ] {
        let (fw, fh, px) = timbury::exr::read(files.get(name).unwrap()).unwrap();
        assert_eq!((fw, fh), (w, h), "{name}");
        assert!(px.iter().any(|&v| v != 0));
    }
}

#[test]
fn text_and_binary_agree() {
    let scene = tree::read_text(
        "Scene { transforms [ MayaTransform a { scalePivot 0 0 0 scalePivotTranslate 0 0 0 scale 1 1 1 rotationPivot 0 0 0 rotationPivotTranslate 0 0 0 rotationOrientation 0 0 0 rotation 0 0 0 translation 1 2 3 rotationOrder xyz } ] }",
    )
    .unwrap();
    let t = &scene.nodes("transforms")[0];
    assert_eq!((t.name.as_str(), t.kind), ("a", "MayaTransform"));
    assert_eq!(t.vec::<3>("translation"), Some([1.0, 2.0, 3.0]));
}

/// The signed `_norm` bump maps keep x, y, z in file byte order: z (byte 2)
/// is about +1 everywhere, so a BGR swap would tilt every normal sideways.
#[test]
fn signed_maps_face_out_in_file_order() {
    let Some(files) = files() else { return };
    let timbury = binary(&files, demo::TIMBURY);
    let mut checked = 0;
    for t in timbury.nodes("textures") {
        let name = t.text("filename");
        if t.kind != "2DTexture" || t.int("format") == 24 || !name.ends_with(".tga") {
            continue;
        }
        let data = files.get(&timbury::data::asset_name(name)).unwrap();
        let (_, _, px) = timbury::tga::read_raw(data).unwrap();
        let z: f64 = px
            .chunks_exact(4)
            .step_by(97)
            .map(|p| f64::from(p[2] as i8))
            .sum::<f64>()
            / (px.len() / 4 / 97 + 1) as f64;
        assert!(z > 100.0, "{name}: mean z {z}");
        checked += 1;
    }
    assert!(checked > 20, "only {checked} signed maps");
}

/// Every shadow caster (and lens copy) of a shape sits exactly where the
/// visible shape does: rigid copies that name only `modelViewProj` (map 9)
/// carry their transform rather than bend through the inverse bind pose.
#[test]
fn pass_copies_coincide_with_the_visible_shapes() {
    let Some(files) = files() else { return };
    let model = timbury::model::Model::new(&binary(&files, demo::TIMBURY), &files).unwrap();
    let anim = binary(&files, demo::ANIMATION);
    let dance = timbury::anim::Animation::read(&anim.nodes("animations")[0]).unwrap();
    let world = model.pose(Some((&dance, 25.0)), &[]);
    let index: std::collections::HashMap<_, _> = model
        .shapes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();
    let mut compared = 0;
    for pass in ["ShadowRenderTargetProxy", "LensRenderTargetProxy"] {
        for &k in &model.group(&format!("{pass}_group")).unwrap().1 {
            let copy = &model.shapes[k];
            let twin = copy.name.replace(pass, "nvPrimaryRenderPass");
            let Some(&t) = index.get(twin.as_str()) else {
                continue;
            };
            let (a, b) = (
                model.bones(copy, &world),
                model.bones(&model.shapes[t], &world),
            );
            assert_eq!(a.len(), b.len(), "{}", copy.name);
            for (x, y) in a.iter().zip(&b) {
                assert!(x.abs_diff_eq(*y, 1e-3), "{} strays from {twin}", copy.name);
            }
            compared += 1;
        }
    }
    assert!(compared > 100, "only {compared} copies compared");
}

/// Face culling follows the filed cullParams, not the shape's own `cull`
/// (view culling): garments and leaves filed _CullNoCull show both sides.
#[test]
fn double_sided_shapes_draw_both_sides() {
    let Some(files) = files() else { return };
    let model = timbury::model::Model::new(&binary(&files, demo::TIMBURY), &files).unwrap();
    let both: Vec<_> = model
        .shapes
        .iter()
        .filter(|s| !s.cull)
        .map(|s| s.name.as_str())
        .collect();
    assert!(both.len() >= 150, "only {} double-sided", both.len());
    for name in [
        "BL_SHORTSShapeShape_nvPrimaryRenderPass_2",
        "BL_SHIRTShapeShape_nvPrimaryRenderPass_5",
    ] {
        assert!(both.contains(&name), "{name} culls its back");
    }
}
