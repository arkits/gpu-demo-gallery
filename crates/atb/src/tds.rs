//! 3D Studio scenes made into ATB objects, as `tdstoats.c` does when an ATB
//! demo loads a `.3ds` (`atsflip.exe` `0x410630`), so they draw like a `.BOF`.
//!
//! Per library material: one with a texture map is lit and modulates its
//! texture (preset 0x5001, or 0x5301 when the picture has alpha), emissive
//! 0.6 and diffuse 1; one with only a reflection map is a sphere map (preset
//! 9); the rest are lit flat colors (0x6000) from the material's ambient and
//! diffuse. Positions become `(-x, z, y)`, texture `v` becomes `1 - v`, and
//! triangles keep their facing by swapping the last two corners.

use glam::Vec3;

use crate::Material;
use crate::bof::{Bof, Group, Image, Node, Object, Shape, Strip, Texture, TriSet};
use crate::glide::Rgba;
use crate::mesh3ds::Scene3ds;

/// The `.3df` name a map file name stands for: same stem, any case.
pub fn texture_name(file: &str) -> String {
    let stem = file.rsplit(['\\', '/']).next().unwrap_or(file);
    let stem = stem.split('.').next().unwrap_or(stem);
    format!("{}.3df", stem.to_ascii_lowercase())
}

fn image(p: &Rgba) -> Image {
    let mut data = Vec::with_capacity(p.rgba.len());
    for px in p.rgba.chunks_exact(4) {
        data.extend([px[3], px[0], px[1], px[2]]);
    }
    Image {
        format: 16,
        width: p.width,
        height: p.height,
        levels: 1,
        file: None,
        data,
        table: None,
    }
}

/// Builds the scene. `picture` gives a `.3df` by name, with whether it has
/// alpha. The scene's root group is the returned id.
pub fn to_bof(scene: &Scene3ds, picture: &dyn Fn(&str) -> Option<(Rgba, bool)>) -> (Bof, u32) {
    let mut objects: Vec<Object> = Vec::new();
    fn add(o: Object, objects: &mut Vec<Object>) -> u32 {
        objects.push(o);
        objects.len() as u32
    }
    // Materials, with their textures.
    let mut materials = Vec::new();
    for m in &scene.materials {
        let map = |file: &Option<String>| file.as_deref().and_then(|f| picture(&texture_name(f)));
        let id = if let Some((p, alpha)) = map(&m.texture) {
            let img = add(Object::Image(image(&p)), &mut objects);
            let tex = add(
                Object::Texture(Texture {
                    handle: 0,
                    images: vec![img],
                }),
                &mut objects,
            );
            let mut mat = Material::preset(if alpha { 0x5301 } else { 0x5001 })
                .with_colors([0.6; 3], [1.0; 3]);
            mat.words[14] = tex;
            add(Object::Material(Box::new(mat)), &mut objects)
        } else if let Some((p, _)) = map(&m.reflection) {
            let img = add(Object::Image(image(&p)), &mut objects);
            let tex = add(
                Object::Texture(Texture {
                    handle: 0,
                    images: vec![img],
                }),
                &mut objects,
            );
            let mut mat = Material::preset(9);
            mat.words[14] = tex;
            add(Object::Material(Box::new(mat)), &mut objects)
        } else {
            let c = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
            let mat = Material::preset(0x6000).with_colors(c(m.ambient), c(m.diffuse));
            add(Object::Material(Box::new(mat)), &mut objects)
        };
        materials.push((m.name.clone(), id));
    }
    let fallback = add(
        Object::Material(Box::new(
            Material::preset(0x6000).with_colors([0.3; 3], [0.7; 3]),
        )),
        &mut objects,
    );

    let mut children = Vec::new();
    for mesh in &scene.meshes {
        if mesh.faces.is_empty() {
            continue;
        }
        let pos: Vec<Vec3> = mesh
            .vertices
            .iter()
            .map(|v| Vec3::new(-v[0], v[2], v[1]))
            .collect();
        let corner = |f: [u16; 3]| [f[0], f[2], f[1]].map(usize::from);
        // Smooth vertex normals from the faces around each vertex.
        let mut normals = vec![Vec3::ZERO; pos.len()];
        for &f in &mesh.faces {
            let [a, b, c] = corner(f);
            if a.max(b).max(c) >= pos.len() {
                continue;
            }
            // Outward, from the file's own corner order.
            let n = (pos[c] - pos[a]).cross(pos[b] - pos[a]);
            for i in [a, b, c] {
                normals[i] += n;
            }
        }
        let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
        let mut covered = vec![false; mesh.faces.len()];
        for (name, faces) in &mesh.groups {
            let id = materials
                .iter()
                .find(|(n, _)| n == name)
                .map_or(fallback, |&(_, id)| id);
            for &f in faces {
                covered[f as usize] = true;
            }
            groups.push((id, faces.iter().map(|&f| f as usize).collect()));
        }
        let rest: Vec<usize> = (0..mesh.faces.len()).filter(|&i| !covered[i]).collect();
        if !rest.is_empty() {
            groups.push((fallback, rest));
        }
        let mut parts = Vec::new();
        for (material, faces) in groups {
            let mut strips = Vec::new();
            let mut strip = Strip {
                flags: 0,
                vertices: Vec::new(),
                triangles: Vec::new(),
                images: Vec::new(),
            };
            let mut index = std::collections::HashMap::new();
            for f in faces {
                let c = corner(mesh.faces[f]);
                if c.iter().any(|&i| i >= pos.len()) {
                    continue;
                }
                // Strips hold at most 62 vertices, as ATB's do.
                if strip.vertices.len() + 3 > 62 {
                    strips.push(std::mem::replace(
                        &mut strip,
                        Strip {
                            flags: 0,
                            vertices: Vec::new(),
                            triangles: Vec::new(),
                            images: Vec::new(),
                        },
                    ));
                    index.clear();
                }
                let t = c.map(|i| {
                    *index.entry(i).or_insert_with(|| {
                        let p = pos[i];
                        let n = normals[i].normalize_or_zero();
                        let uv = mesh.texcoords.get(i).copied().unwrap_or([0.0; 2]);
                        let mut v = [0.0; 16];
                        v[..6].copy_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z]);
                        v[6] = uv[0];
                        v[7] = 1.0 - uv[1];
                        strip.vertices.push(v);
                        (strip.vertices.len() - 1) as u8
                    })
                });
                strip.triangles.push(t);
            }
            if !strip.triangles.is_empty() {
                strips.push(strip);
            }
            let geometry = add(
                Object::TriSet(TriSet {
                    header: [0; 6],
                    strips,
                }),
                &mut objects,
            );
            parts.push((material, geometry));
        }
        let shape = add(
            Object::Shape(Shape {
                node: Node {
                    flags: [0, u32::MAX],
                    name: Some(mesh.name.clone()),
                    sphere: [0.0; 4],
                },
                parts,
            }),
            &mut objects,
        );
        children.push(shape);
    }
    let root = add(
        Object::Group(Group {
            node: Node {
                flags: [0, u32::MAX],
                name: None,
                sphere: [0.0; 4],
            },
            children,
        }),
        &mut objects,
    );
    (
        Bof {
            objects,
            dict: Vec::new(),
        },
        root,
    )
}

/// Centre and radius of everything under `root`, as `atsGetBoundingSphere`
/// gives the demos their camera distance: the box's centre and the farthest
/// vertex from it.
pub fn bounds(bof: &Bof, items: &[crate::Item]) -> (Vec3, f32) {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    let mut points = Vec::new();
    for item in items {
        if let Some(Object::TriSet(t)) = bof.get(item.geometry) {
            for s in &t.strips {
                for v in &s.vertices {
                    let p = item.matrix.transform_point3(Vec3::new(v[0], v[1], v[2]));
                    lo = lo.min(p);
                    hi = hi.max(p);
                    points.push(p);
                }
            }
        }
    }
    if points.is_empty() {
        return (Vec3::ZERO, 1.0);
    }
    let c = (lo + hi) * 0.5;
    let r = points.iter().map(|p| p.distance(c)).fold(0.0, f32::max);
    (c, r.max(1e-3))
}

/// A `.vq` model (`vqtoats.c`, `atsflip.exe` `0x40d510`): text lines
/// `v index x y z nx ny nz` and quads `q a b c d`, after a
/// `# total polys =` line. It is drawn chrome: `ra.3df` as a sphere map
/// (`0x40d940`, preset 9, clamped).
pub fn vq_to_bof(text: &str, chrome: Rgba) -> Result<(Bof, u32), crate::Error> {
    let mut lines = text.lines();
    if !lines
        .next()
        .is_some_and(|l| l.starts_with("# total polys ="))
    {
        return Err("not a VQ model".into());
    }
    let mut verts: Vec<[f32; 6]> = Vec::new();
    let mut quads = Vec::new();
    for line in lines {
        let mut f = line.split_whitespace();
        match f.next() {
            Some("v") => {
                let i: usize = f.next().ok_or("VQ vertex index")?.parse()?;
                let mut v = [0.0; 6];
                for x in &mut v {
                    *x = f.next().ok_or("VQ vertex cut short")?.parse()?;
                }
                if i >= verts.len() {
                    verts.resize(i + 1, [0.0; 6]);
                }
                verts[i] = v;
            }
            Some("q") => {
                let mut q = [0usize; 4];
                for x in &mut q {
                    *x = f.next().ok_or("VQ quad cut short")?.parse()?;
                }
                quads.push(q);
            }
            _ => {}
        }
    }
    if quads.iter().flatten().any(|&i| i >= verts.len()) {
        return Err("VQ quad names a missing vertex".into());
    }
    let mut objects = vec![
        Object::Image(image(&chrome)),
        Object::Texture(Texture {
            handle: 0,
            images: vec![1],
        }),
    ];
    let mut mat = Material::preset(9);
    mat.words[14] = 2;
    mat.words[47] = 1;
    mat.words[49] = 1;
    objects.push(Object::Material(Box::new(mat)));
    let mut strips = Vec::new();
    let mut strip = Strip {
        flags: 0,
        vertices: Vec::new(),
        triangles: Vec::new(),
        images: Vec::new(),
    };
    for [a, b, c, d] in quads {
        if strip.vertices.len() + 4 > 62 {
            strips.push(std::mem::replace(
                &mut strip,
                Strip {
                    flags: 0,
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                    images: Vec::new(),
                },
            ));
        }
        let base = strip.vertices.len() as u8;
        for i in [a, b, c, d] {
            let mut v = [0.0; 16];
            v[..6].copy_from_slice(&verts[i]);
            strip.vertices.push(v);
        }
        strip.triangles.push([base, base + 1, base + 2]);
        strip.triangles.push([base, base + 2, base + 3]);
    }
    if !strip.triangles.is_empty() {
        strips.push(strip);
    }
    objects.push(Object::TriSet(TriSet {
        header: [0; 6],
        strips,
    }));
    objects.push(Object::Shape(Shape {
        node: Node {
            flags: [0, u32::MAX],
            name: None,
            sphere: [0.0; 4],
        },
        parts: vec![(3, 4)],
    }));
    Ok((
        Bof {
            objects,
            dict: Vec::new(),
        },
        5,
    ))
}
