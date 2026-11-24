//! Everything drawn each frame (`main.cpp` 0x40cf90, `EarlyDemo.cpp`,
//! `VolumeRenderer.cpp`).
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::{BlendFactor as F, Face};

use crate::Error;
use crate::data::{Files, Mesh, Vertex, read_obj};
use crate::gpu::{COLOR, DEPTH, Gpu, SLOT, rgba};
use crate::sim::{COLS, DIM};

const HALF: Vec3 = Vec3::new(0.5, 0.25, 0.25);
/// Smoke is drawn at this size and stretched over the window.
const SMOKE_SIZE: u32 = 128 * 3;
/// Cube faces in +X, -X, +Y, -Y, +Z, -Z order.
const FACES: [&str; 6] = ["right", "left", "top", "bottom", "front", "back"];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_proj: [f32; 16],
    eye: [f32; 3],
    scale: f32,
    forward: [f32; 3],
    refract_on: f32,
    dim: [f32; 3],
    cols: f32,
}

struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

/// What to draw.
pub struct Frame {
    pub view: Mat4,
    pub eye: Vec3,
    pub water: bool,
    pub supersample: bool,
}

struct Pipelines {
    shadow: wgpu::RenderPipeline,
    gold: wgpu::RenderPipeline,
    velvet: wgpu::RenderPipeline,
    glass_back: wgpu::RenderPipeline,
    plate_back: wgpu::RenderPipeline,
    occluder: wgpu::RenderPipeline,
    smoke: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    volume_over: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    // Into the window.
    copy: wgpu::RenderPipeline,
    glass_front: wgpu::RenderPipeline,
    plate_front: wgpu::RenderPipeline,
    gold_front: wgpu::RenderPipeline,
    glow: wgpu::RenderPipeline,
}

struct Targets {
    key: (u32, u32, bool),
    scene: wgpu::TextureView,
    scene_depth: wgpu::TextureView,
    window_depth: wgpu::TextureView,
    smoke: wgpu::TextureView,
    water: wgpu::TextureView,
    glow: [wgpu::TextureView; 2],
    glow_depth: wgpu::TextureView,
}

pub struct Scene {
    pipes: Pipelines,
    image_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    blank: wgpu::TextureView,
    stand: Buffers,
    cushion: Buffers,
    floor: Buffers,
    plate: Buffers,
    /// Five glass walls.
    walls: Buffers,
    cube: Buffers,
    ray_cube: Buffers,
    targets: Option<Targets>,
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, window: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        let mesh = |name: &str| -> Result<Buffers, Error> {
            let text = std::str::from_utf8(files.get(&format!("models/{name}.obj"))?)?;
            Ok(buffers(gpu, &read_obj(text)?))
        };
        let image = |name: &str, side: u32, channels: usize| -> Result<Vec<u8>, Error> {
            let data = files.get(&format!("textures/{name}.raw"))?;
            if data.len() != (side * side) as usize * channels {
                return Err(format!("{name}.raw has an unexpected size").into());
            }
            Ok(rgba(data, channels))
        };
        let plain = |t: wgpu::Texture| t.create_view(&Default::default());
        let velvet = plain(gpu.mipped(256, &[image("velvet", 256, 1)?]));
        let shadow = plain(gpu.mipped(1024, &[image("shadows", 1024, 3)?]));
        let noise = plain(gpu.mipped(1024, &[image("noise", 1024, 3)?]));
        let faces: Result<Vec<_>, _> = FACES.iter().map(|f| image(f, 512, 3)).collect();
        let env = gpu.mipped(512, &faces?);
        let env = env.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let blank = plain(gpu.mipped(1, &[vec![0; 4]]));
        let sampler = |address: wgpu::AddressMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: address,
                address_mode_v: address,
                address_mode_w: address,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let clamp = sampler(wgpu::AddressMode::ClampToEdge);
        let repeat = sampler(wgpu::AddressMode::Repeat);

        let texture = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let sampled = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        use wgpu::TextureViewDimension::{Cube, D2};
        let size = wgpu::BufferSize::new(size_of::<Uniform>() as u64);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: size,
                    },
                    count: None,
                },
                sampled(1),
                sampled(2),
                texture(3, Cube),
                texture(4, D2),
                texture(5, D2),
                texture(6, D2),
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[texture(0, D2), texture(1, D2)],
        });
        let uniforms = gpu.uniforms(2);
        let view = wgpu::BindingResource::TextureView;
        let resources = [
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniforms,
                offset: 0,
                size,
            }),
            wgpu::BindingResource::Sampler(&clamp),
            wgpu::BindingResource::Sampler(&repeat),
            view(&env),
            view(&velvet),
            view(&shadow),
            view(&noise),
        ];
        let entries: Vec<_> = (0..)
            .zip(resources)
            .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &entries,
        });
        let pipes = Pipelines::new(device, &layout, &image_layout, window);
        Ok(Self {
            pipes,
            image_layout,
            uniforms,
            group,
            blank,
            stand: mesh("stand")?,
            cushion: mesh("cushion")?,
            floor: mesh("shadow")?,
            plate: mesh("glass")?,
            walls: buffers(gpu, &cube(true, false)),
            cube: buffers(gpu, &cube(false, false)),
            ray_cube: buffers(gpu, &cube(false, true)),
            targets: None,
        })
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        density: &wgpu::TextureView,
        window: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) {
        let key = (width, height, frame.supersample);
        if self.targets.as_ref().is_none_or(|t| t.key != key) {
            self.targets = Some(Targets::new(gpu, key));
        }
        let t = self.targets.as_ref().expect("just made");
        let aspect = width as f32 / height as f32;
        let proj =
            glam::camera::rh::proj::directx::perspective(45f32.to_radians(), aspect, 0.1, 100.0);
        let scale = if frame.supersample { 2.0 } else { 1.0 };
        for (slot, (scale, refract_on)) in [(1.0, 0.0), (scale, 1.0)].into_iter().enumerate() {
            let u = Uniform {
                view_proj: (proj * frame.view).to_cols_array(),
                eye: frame.eye.to_array(),
                scale,
                forward: (-frame.eye.normalize()).to_array(),
                refract_on,
                dim: DIM.as_vec3().to_array(),
                cols: COLS as f32,
            };
            gpu.queue
                .write_buffer(&self.uniforms, slot as u64 * SLOT, bytemuck::bytes_of(&u));
        }
        let p = &self.pipes;
        let images = |image: &wgpu::TextureView, atlas: &wgpu::TextureView| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(image),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(atlas),
                    },
                ],
            })
        };
        let none = images(&self.blank, &self.blank);
        let scene = images(&t.scene, &self.blank);
        let mut pass =
            |target, depth, clear: Option<wgpu::Color>, slot: u64, images: &wgpu::BindGroup| {
                let mut rp = start(enc, target, depth, clear).forget_lifetime();
                rp.set_bind_group(0, &self.group, &[(slot * SLOT) as u32]);
                rp.set_bind_group(1, images, &[]);
                rp
            };

        let empty = Some(wgpu::Color::TRANSPARENT);
        // The scene without the volume.
        let mut rp = pass(&t.scene, Some(&t.scene_depth), empty, 0, &none);
        draw(&mut rp, &p.shadow, &self.floor);
        draw(&mut rp, &p.gold, &self.stand);
        draw(&mut rp, &p.glass_back, &self.walls);
        draw(&mut rp, &p.plate_back, &self.plate);
        if !frame.water {
            draw(&mut rp, &p.velvet, &self.cushion);
        }
        drop(rp);

        // The volume, then over the scene.
        let (volume, pipe) = match frame.water {
            true => (&t.water, &p.water),
            false => (&t.smoke, &p.smoke),
        };
        let mut rp = pass(volume, None, empty, 0, &images(&t.scene, density));
        draw(&mut rp, pipe, &self.ray_cube);
        drop(rp);
        let mut rp = pass(&t.scene, None, None, 0, &images(volume, &self.blank));
        quad(&mut rp, &p.volume_over);
        drop(rp);

        // Glow of the gold outside the box.
        let mut rp = pass(&t.glow[0], Some(&t.glow_depth), empty, 0, &none);
        draw(&mut rp, &p.occluder, &self.cube);
        draw(&mut rp, &p.gold, &self.stand);
        drop(rp);
        for i in 0..4 {
            let (from, to) = (&t.glow[i % 2], &t.glow[(i + 1) % 2]);
            let mut rp = pass(to, None, empty, 0, &images(from, &self.blank));
            quad(&mut rp, if i % 2 == 0 { &p.blur_h } else { &p.blur_v });
        }

        // The window.
        let mut rp = pass(
            window,
            Some(&t.window_depth),
            Some(wgpu::Color::BLACK),
            1,
            &scene,
        );
        quad(&mut rp, &p.copy);
        draw(&mut rp, &p.glass_front, &self.walls);
        draw(&mut rp, &p.plate_front, &self.plate);
        draw(&mut rp, &p.gold_front, &self.stand);
        rp.set_bind_group(1, &images(&t.glow[0], &self.blank), &[]);
        quad(&mut rp, &p.glow);
    }
}

fn draw(rp: &mut wgpu::RenderPass, pipe: &wgpu::RenderPipeline, b: &Buffers) {
    rp.set_pipeline(pipe);
    rp.set_vertex_buffer(0, b.vertices.slice(..));
    rp.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
    rp.draw_indexed(0..b.count, 0, 0..1);
}

fn quad(rp: &mut wgpu::RenderPass, pipe: &wgpu::RenderPipeline) {
    rp.set_pipeline(pipe);
    rp.draw(0..3, 0..1);
}

fn start<'e>(
    enc: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    depth: Option<&wgpu::TextureView>,
    clear: Option<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    let load = clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    })
}

impl Targets {
    fn new(gpu: &Gpu, key: (u32, u32, bool)) -> Self {
        let (w, h, supersample) = key;
        let max = gpu.device.limits().max_texture_dimension_2d;
        let k = if supersample { 2 } else { 1 };
        let (sw, sh) = ((w * k).min(max), (h * k).min(max));
        let (gw, gh) = ((sw / 4).max(1), (sh / 4).max(1));
        let make = |w, h, f| gpu.target(w, h, f).create_view(&Default::default());
        Self {
            key,
            scene: make(sw, sh, COLOR),
            scene_depth: make(sw, sh, DEPTH),
            window_depth: make(w, h, DEPTH),
            smoke: make(SMOKE_SIZE, SMOKE_SIZE, COLOR),
            water: make(sw, sh, COLOR),
            glow: [make(gw, gh, COLOR), make(gw, gh, COLOR)],
            glow_depth: make(gw, gh, DEPTH),
        }
    }
}

struct Kind {
    vs: &'static str,
    fs: &'static str,
    format: wgpu::TextureFormat,
    cull: Option<Face>,
    blend: Option<(F, F)>,
    /// None: no depth buffer. Otherwise whether to test and whether to write.
    depth: Option<(bool, bool)>,
    write: wgpu::ColorWrites,
}

impl Pipelines {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        images: &wgpu::BindGroupLayout,
        window: wgpu::TextureFormat,
    ) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("../shaders/scene.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(layout), Some(images)],
            immediate_size: 0,
        });
        let mesh_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
        };
        let make = |k: Kind| {
            let component = k
                .blend
                .map(|(src_factor, dst_factor)| wgpu::BlendComponent {
                    src_factor,
                    dst_factor,
                    operation: wgpu::BlendOperation::Add,
                });
            let mesh = [Some(mesh_layout.clone())];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(k.fs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(k.vs),
                    compilation_options: Default::default(),
                    buffers: if k.vs == "quad_vs" { &[] } else { &mesh },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(k.fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: k.format,
                        blend: component.map(|c| wgpu::BlendState { color: c, alpha: c }),
                        write_mask: k.write,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: k.cull,
                    ..Default::default()
                },
                depth_stencil: k.depth.map(|(test, write)| wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(match test {
                        true => wgpu::CompareFunction::LessEqual,
                        false => wgpu::CompareFunction::Always,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let solid = Some((true, true));
        let over = Some((F::SrcAlpha, F::OneMinusSrcAlpha));
        let mesh = |fs, cull, blend, depth| Kind {
            vs: "mesh_vs",
            fs,
            format: COLOR,
            cull,
            blend,
            depth,
            write: wgpu::ColorWrites::ALL,
        };
        let full = |fs, blend| Kind {
            vs: "quad_vs",
            ..mesh(fs, None, blend, None)
        };
        // Browsers show the page through a canvas that is not opaque.
        let on_window = |k: Kind| Kind {
            format: window,
            write: wgpu::ColorWrites::COLOR,
            ..k
        };
        Self {
            shadow: make(mesh("shadow_fs", None, None, solid)),
            gold: make(mesh("gold_fs", Some(Face::Back), None, solid)),
            velvet: make(mesh("velvet_fs", Some(Face::Back), None, solid)),
            glass_back: make(mesh(
                "glass_fs",
                Some(Face::Front),
                over,
                Some((true, false)),
            )),
            plate_back: make(mesh(
                "refract_fs",
                Some(Face::Front),
                over,
                Some((true, false)),
            )),
            occluder: make(Kind {
                write: wgpu::ColorWrites::empty(),
                ..mesh("occlude_fs", Some(Face::Back), None, solid)
            }),
            smoke: make(Kind {
                vs: "ray_vs",
                ..mesh("slicecast_fs", Some(Face::Back), None, None)
            }),
            water: make(Kind {
                vs: "ray_vs",
                ..mesh("water_fs", Some(Face::Back), None, None)
            }),
            volume_over: make(full("copy_fs", over)),
            blur_h: make(full("blurh_fs", None)),
            blur_v: make(full("blurv_fs", None)),
            copy: make(on_window(Kind {
                depth: Some((false, false)),
                ..full("copy_fs", None)
            })),
            glass_front: make(on_window(mesh("glass_fs", Some(Face::Back), over, solid))),
            plate_front: make(on_window(mesh("refract_fs", Some(Face::Back), None, solid))),
            gold_front: make(on_window(mesh("gold_fs", Some(Face::Back), None, solid))),
            glow: make(on_window(Kind {
                depth: Some((false, false)),
                ..full("glow_fs", Some((F::One, F::One)))
            })),
        }
    }
}

fn buffers(gpu: &Gpu, m: &Mesh) -> Buffers {
    let vertices = bytemuck::cast_slice(&m.vertices);
    let indices = bytemuck::cast_slice(&m.indices);
    Buffers {
        vertices: gpu.buffer(vertices, wgpu::BufferUsages::VERTEX),
        indices: gpu.buffer(indices, wgpu::BufferUsages::INDEX),
        count: m.indices.len() as u32,
    }
}

/// The box. `walls` leaves out the back, which is the glass plate model. `cells`
/// puts grid coordinates in the normals, where ray casting starts.
fn cube(walls: bool, cells: bool) -> Mesh {
    let mut m = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for (axis, sign) in [
        (0, 1.0),
        (0, -1.0),
        (1, 1.0),
        (1, -1.0),
        (2, 1.0),
        (2, -1.0f32),
    ] {
        if walls && axis == 2 && sign < 0.0 {
            continue;
        }
        let mut n = Vec3::ZERO;
        n[axis] = sign;
        // u x v = n, so corners run counter-clockwise seen from outside.
        let u = if axis == 1 { Vec3::X } else { Vec3::Y };
        let v = n.cross(u);
        let base = m.vertices.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let pos = (n + u * a + v * b) * HALF;
            let mut start = pos * 128.0 + DIM.as_vec3() * 0.5;
            if axis == 0 && sign > 0.0 {
                start.x -= 2.0;
            }
            m.vertices.push(Vertex {
                pos: pos.to_array(),
                normal: if cells { start } else { n }.to_array(),
                uv: [0.0; 2],
            });
        }
        m.indices.extend([0, 1, 2, 0, 2, 3].map(|i: u32| base + i));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_faces_outwards() {
        let m = cube(false, false);
        assert_eq!(m.indices.len(), 36);
        assert_eq!(cube(true, false).indices.len(), 30);
        for t in m.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.vertices[i as usize]);
            let (p, q, r) = (Vec3::from(a.pos), Vec3::from(b.pos), Vec3::from(c.pos));
            assert!((q - p).cross(r - p).dot(Vec3::from(a.normal)) > 0.0);
        }
    }
}
