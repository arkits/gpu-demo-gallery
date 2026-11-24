//! Draws water, creature, fish, plankton and laser.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::BlendFactor as F;

use crate::Error;
use crate::data::{CAUSTICS, FACES, Files};
use crate::dds::Image;
use crate::gpu::{DEPTH, Gpu, SAMPLES};
use crate::model::{Animation, Kind, Model, Shape, Vertex};
use crate::show::{FAR, Frame, MOST_FISH};
use crate::tga;

/// 0.7 radians, vertically; the planes are 1 and 1.4 times the fog's end (0x4016c0).
const FOV: f32 = 0.7;
const NEAR: f32 = 1.0;
/// Pieces of the laser beam.
const BEAMS: usize = 10;
const CELLS: usize = 27;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_proj: [f32; 16],
    view: [f32; 16],
    inverse: [f32; 16],
    light: [f32; 4],
    fog: [f32; 4],
    misc: [f32; 4],
    drift: [f32; 4],
    eye: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    place: [f32; 16],
    diffuse: [f32; 4],
    ambient: [f32; 4],
}

const INSTANCE: u64 = size_of::<Instance>() as u64;

struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    kind: Kind,
    textures: wgpu::BindGroup,
    node: usize,
    diffuse: Vec4,
    ambient: Vec4,
}

/// Width and height.
type Side = (u32, u32);

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

pub struct Scene {
    format: wgpu::TextureFormat,
    model: Model,
    animation: Animation,
    mask: usize,
    radius: f32,
    creature: Vec<Mesh>,
    fish: Mesh,
    beam: Mesh,
    lit: wgpu::RenderPipeline,
    textured: wgpu::RenderPipeline,
    bump: wgpu::RenderPipeline,
    laser: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    specks: wgpu::RenderPipeline,
    image: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    plankton: wgpu::Buffer,
    cells: wgpu::Buffer,
    badge: wgpu::BindGroup,
    quad: wgpu::Buffer,
    targets: Option<Targets>,
}

const MESH: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
const PLACE: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
];
const SPECK: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x4];
const CELL: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![1 => Float32x4];
const IMAGE: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

fn buffers<'a>(
    stride: u64,
    step_mode: wgpu::VertexStepMode,
    attributes: &'a [wgpu::VertexAttribute],
) -> Option<wgpu::VertexBufferLayout<'a>> {
    Some(wgpu::VertexBufferLayout {
        array_stride: stride,
        step_mode,
        attributes,
    })
}

struct Maker<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipelines: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    textures: HashMap<String, wgpu::TextureView>,
}

struct Pipe<'a> {
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    strip: bool,
    blend: bool,
    test: bool,
    write: bool,
}

impl Maker<'_> {
    fn pipeline(&self, p: Pipe) -> wgpu::RenderPipeline {
        let c = wgpu::BlendComponent {
            src_factor: F::SrcAlpha,
            dst_factor: F::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        self.gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(p.fs),
                layout: Some(&self.pipelines),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some(p.vs),
                    compilation_options: Default::default(),
                    buffers: p.buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(p.fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.format,
                        blend: p.blend.then_some(wgpu::BlendState { color: c, alpha: c }),
                        // Browsers show the page through a canvas that is not opaque.
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: match p.strip {
                        true => wgpu::PrimitiveTopology::TriangleStrip,
                        false => wgpu::PrimitiveTopology::TriangleList,
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(p.write),
                    depth_compare: Some(match p.test {
                        true => wgpu::CompareFunction::LessEqual,
                        false => wgpu::CompareFunction::Always,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
    }

    fn texture(&mut self, name: &str) -> Result<wgpu::TextureView, Error> {
        if let Some(t) = self.textures.get(name) {
            return Ok(t.clone());
        }
        let data = self.files.get(name)?;
        let t = if name.ends_with(".tga") {
            let (w, h, px) = tga::read(data).map_err(|e| format!("{name}: {e}"))?;
            self.gpu.upload_rgba((w as u32, h as u32), &[px], true)
        } else {
            let i = Image::read(data).map_err(|e| format!("{name}: {e}"))?;
            self.gpu.upload(&[i]).map_err(|e| format!("{name}: {e}"))?
        };
        let v = t.create_view(&Default::default());
        self.textures.insert(name.to_owned(), v.clone());
        Ok(v)
    }

    fn group(&mut self, names: &[String]) -> Result<wgpu::BindGroup, Error> {
        // Lit shapes have no texture; they get the badge, unused.
        let first = names.first().map_or("nvidialogo.dds", String::as_str);
        let views = [
            self.texture(first)?,
            self.texture(names.last().map_or(first, String::as_str))?,
        ];
        let entries = [0, 1].map(|i| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(&views[i]),
        });
        Ok(self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &entries,
            }))
    }

    fn mesh(&mut self, s: &Shape) -> Result<Mesh, Error> {
        let g = self.gpu;
        Ok(Mesh {
            vertices: g.buffer(
                bytemuck::cast_slice(&s.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: g.buffer(bytemuck::cast_slice(&s.indices), wgpu::BufferUsages::INDEX),
            count: s.indices.len() as u32,
            kind: s.kind,
            textures: self.group(&s.textures)?,
            node: s.node,
            diffuse: s.diffuse,
            ambient: s.ambient,
        })
    }

    /// Images of one size from TGA files.
    fn layers(&self, names: &[String]) -> Result<(Side, Vec<Vec<u8>>), Error> {
        let (mut side, mut out) = (None, Vec::new());
        for n in names {
            let (w, h, px) = tga::read(self.files.get(n)?).map_err(|e| format!("{n}: {e}"))?;
            if *side.get_or_insert((w as u32, h as u32)) != (w as u32, h as u32) {
                return Err(format!("{n} differs in size").into());
            }
            out.push(px);
        }
        Ok((side.ok_or("no images")?, out))
    }
}

fn texture_entry(
    binding: u32,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn one(m: &Model) -> Result<&Shape, Error> {
    match m.shapes.as_slice() {
        [s] => Ok(s),
        _ => Err("expected one shape".into()),
    }
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        use wgpu::TextureViewDimension as D;
        let device = &gpu.device;
        let size = wgpu::BufferSize::new(size_of::<Uniform>() as u64);
        let constants = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: size,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(2, D::D2Array),
                texture_entry(3, D::Cube),
            ],
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[texture_entry(0, D::D2), texture_entry(1, D::D2)],
        });
        let mut m = Maker {
            gpu,
            files,
            format,
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
            }),
            pipelines: device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&constants), Some(&layout)],
                immediate_size: 0,
            }),
            layout,
            textures: HashMap::new(),
        };
        let (side, frames) = m.layers(
            &(0..CAUSTICS)
                .map(|i| format!("caust{i:02}.tga"))
                .collect::<Vec<_>>(),
        )?;
        let caustics = gpu.upload_rgba(side, &frames, true);
        let (side, faces) = m.layers(&FACES.map(|f| format!("underwater_{f}.tga")))?;
        let water = gpu.upload_rgba(side, &faces, true);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dimension = |t: &wgpu::Texture, d| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(d),
                ..Default::default()
            })
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &constants,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dimension(&caustics, D::D2Array)),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&dimension(&water, D::Cube)),
                },
            ],
        });

        let model = files.model("creature.nmb")?;
        let animation = files.animation(&model)?;
        let creature: Result<Vec<_>, Error> = model.shapes.iter().map(|s| m.mesh(s)).collect();
        let (fish, beam) = (files.model("fish.nmb")?, files.model("beam.nmb")?);
        let (fish, beam) = (m.mesh(one(&fish)?)?, m.mesh(one(&beam)?)?);

        use wgpu::VertexStepMode::{Instance as Each, Vertex as Every};
        let mesh = [
            buffers(size_of::<Vertex>() as u64, Every, &MESH),
            buffers(INSTANCE, Each, &PLACE),
        ];
        let solid = |fs| Pipe {
            vs: "vs",
            fs,
            buffers: &mesh,
            strip: false,
            blend: false,
            test: true,
            write: true,
        };
        let moving = |label, size: u64, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertex = wgpu::BufferUsages::VERTEX;
        Ok(Self {
            format,
            lit: m.pipeline(solid("lit_fs")),
            textured: m.pipeline(solid("textured_fs")),
            bump: m.pipeline(solid("bump_fs")),
            laser: m.pipeline(Pipe {
                blend: true,
                write: false,
                ..solid("beam_fs")
            }),
            sky: m.pipeline(Pipe {
                vs: "sky_vs",
                buffers: &[],
                test: false,
                write: false,
                ..solid("sky_fs")
            }),
            specks: m.pipeline(Pipe {
                vs: "plankton_vs",
                buffers: &[buffers(16, Each, &SPECK), buffers(16, Each, &CELL)],
                strip: true,
                ..solid("plankton_fs")
            }),
            image: m.pipeline(Pipe {
                vs: "image_vs",
                buffers: &[buffers(16, Every, &IMAGE)],
                blend: true,
                test: false,
                write: false,
                ..solid("image_fs")
            }),
            mask: model.find("originCreature_pMask").ok_or("no mask")?,
            radius: model.radius(),
            model,
            animation,
            creature: creature?,
            fish,
            beam,
            uniforms,
            group,
            instances: moving(
                "instances",
                (INSTANCE as usize * (512 + MOST_FISH + BEAMS)) as u64,
                vertex,
            ),
            plankton: moving("plankton", 2000 * 16, vertex),
            cells: moving("cells", 2000 * CELLS as u64 * 16, vertex),
            badge: m.group(&["nvidialogo.dds".into()])?,
            quad: moving("quad", 96, vertex),
            targets: None,
        })
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// The drifting specks, in one cell.
    pub fn set_plankton(&self, gpu: &Gpu, specks: &[[f32; 4]]) {
        gpu.queue
            .write_buffer(&self.plankton, 0, bytemuck::cast_slice(specks));
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        window: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let make = |f| {
                let t = gpu.target(size.0, size.1, f, SAMPLES);
                t.create_view(&Default::default())
            };
            self.targets = Some(Targets {
                size,
                color: make(self.format),
                depth: make(DEPTH),
            });
        }
        let t = self.targets.as_ref().expect("just made");
        let aspect = size.0 as f32 / size.1 as f32;
        // On screens narrower than 4:3 keep the width instead.
        let narrow = (4.0 / 3.0 / aspect).max(1.0);
        let fov = ((FOV / 2.0).tan() * narrow).atan() * 2.0;
        let projection = glam::camera::rh::proj::directx::perspective(fov, aspect, NEAR, FAR * 1.4);
        let view_proj = projection * frame.view;
        let seconds = frame.ms / 1000.0;
        let speck = (size.1 as f32 / 768.0).max(1.0);
        let drift = |rate: f64, far: f64| ((frame.ms * rate).cos() * far) as f32;
        let uniform = Uniform {
            view_proj: view_proj.to_cols_array(),
            view: frame.view.to_cols_array(),
            inverse: view_proj.inverse().to_cols_array(),
            light: frame.light.extend(0.0).to_array(),
            fog: frame.fog.extend(FAR).to_array(),
            misc: [
                ((seconds * 30.0) as usize % CAUSTICS) as f32,
                (frame.ms * 0.0007).fract() as f32,
                (frame.ms * 0.0043).fract() as f32,
                0.0,
            ],
            drift: [
                drift(0.00051, 11.0),
                drift(0.00019, 7.0),
                speck / size.0 as f32,
                speck / size.1 as f32,
            ],
            eye: frame.eye.extend(1.0).to_array(),
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));

        let pose = self.model.pose(&self.animation, frame.ms);
        let put = |m: &Mesh, place: Mat4| Instance {
            place: place.to_cols_array(),
            diffuse: m.diffuse.to_array(),
            ambient: m.ambient.to_array(),
        };
        let mut data: Vec<Instance> = self.creature.iter().map(|m| put(m, pose[m.node])).collect();
        let fish_at = data.len();
        data.extend(
            frame
                .fish
                .iter()
                .take(MOST_FISH)
                .map(|p| put(&self.fish, *p)),
        );
        let beam_at = data.len();
        if frame.laser {
            // From the mask's eye on (0x403760).
            data.extend((0..BEAMS).map(|i| {
                let along = Vec3::new(0.0, 2.7, 12.0 + 10.0 * i as f32);
                put(&self.beam, pose[self.mask] * Mat4::from_translation(along))
            }));
        }
        gpu.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));

        // The cells of plankton around the camera's.
        let home = (frame.eye / FAR).floor();
        let mut cells: Vec<[f32; 4]> = Vec::new();
        for i in 0..CELLS {
            let d = Vec3::new((i / 9) as f32, (i / 3 % 3) as f32, (i % 3) as f32) - 1.0;
            let corner = (home + d) * FAR;
            cells.extend(std::iter::repeat_n(corner.extend(0.0).to_array(), 2000));
        }
        gpu.queue
            .write_buffer(&self.cells, 0, bytemuck::cast_slice(&cells));

        let (w, h) = (0.4 * (4.0 / 3.0 / aspect).min(1.0), 0.14 / narrow);
        let (x, y) = (1.0 - w, -1.0);
        let v = |i: f32, j: f32| [x + i * w, y + j * h, i, 1.0 - j];
        let quad = [
            v(0.0, 0.0),
            v(1.0, 0.0),
            v(0.0, 1.0),
            v(0.0, 1.0),
            v(1.0, 0.0),
            v(1.0, 1.0),
        ];
        gpu.queue
            .write_buffer(&self.quad, 0, bytemuck::cast_slice(&quad));

        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.color,
                depth_slice: None,
                resolve_target: Some(window),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        rp.set_bind_group(0, &self.group, &[]);
        rp.set_bind_group(1, &self.badge, &[]);
        rp.set_pipeline(&self.sky);
        rp.draw(0..3, 0..1);
        let mut mesh = |pipe: &wgpu::RenderPipeline, m: &Mesh, at: usize, n: usize| {
            rp.set_pipeline(pipe);
            rp.set_bind_group(1, &m.textures, &[]);
            rp.set_vertex_buffer(0, m.vertices.slice(..));
            // By offset: WebGL has no first instance.
            rp.set_vertex_buffer(1, self.instances.slice(at as u64 * INSTANCE..));
            rp.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..m.count, 0, 0..n as u32);
        };
        for (i, m) in self.creature.iter().enumerate() {
            let pipe = match m.kind {
                Kind::Lit => &self.lit,
                Kind::Textured => &self.textured,
                Kind::Bump => &self.bump,
            };
            mesh(pipe, m, i, 1);
        }
        if beam_at > fish_at {
            mesh(&self.textured, &self.fish, fish_at, beam_at - fish_at);
        }
        rp.set_pipeline(&self.specks);
        rp.set_bind_group(1, &self.badge, &[]);
        rp.set_vertex_buffer(0, self.plankton.slice(..));
        for c in 0..CELLS as u64 {
            rp.set_vertex_buffer(1, self.cells.slice(c * 2000 * 16..));
            rp.draw(0..4, 0..2000);
        }
        if frame.laser {
            rp.set_pipeline(&self.laser);
            rp.set_bind_group(1, &self.beam.textures, &[]);
            rp.set_vertex_buffer(0, self.beam.vertices.slice(..));
            rp.set_vertex_buffer(1, self.instances.slice(beam_at as u64 * INSTANCE..));
            rp.set_index_buffer(self.beam.indices.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..self.beam.count, 0, 0..BEAMS as u32);
        }
        if frame.logo {
            rp.set_pipeline(&self.image);
            rp.set_bind_group(1, &self.badge, &[]);
            rp.set_vertex_buffer(0, self.quad.slice(..));
            rp.draw(0..6, 0..1);
        }
    }
}
