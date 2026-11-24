//! Draws one logo in one of its six stages, with caption, logo and fade.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};
use wgpu::BlendFactor as F;

use crate::Error;
use crate::combiner::Program;
use crate::data::{FACES, Files, MODELS, STAGES};
use crate::dds::Image;
use crate::gpu::{DEPTH, Gpu, SAMPLES, SLOT};
use crate::model::{Model, Style, Vertex};

/// 0.7 radians, vertically.
const FOV: f32 = 0.7;
/// The solid stage's material (0x414a05).
const SOLID: f32 = 0.8;
/// Draws with their own constants in a frame.
const SLOTS: u64 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_proj: [f32; 16],
    view: [f32; 16],
    color0: [f32; 4],
    color1: [f32; 4],
    diffuse: [f32; 4],
    light: [f32; 4],
    screen: [f32; 4],
}

struct Shape {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    points: u32,
    edges: wgpu::Buffer,
    edge_count: u32,
}

struct Styled {
    style: Style,
    pipeline: usize,
    textures: Option<wgpu::BindGroup>,
}

struct Logo {
    shapes: Vec<Shape>,
    /// Per style, per shape.
    styles: Vec<Vec<Styled>>,
}

/// What to show.
pub struct Frame {
    pub model: usize,
    pub stage: usize,
    /// Radians.
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub caption: bool,
    pub logo: bool,
    /// 0 is black.
    pub brightness: f32,
}

impl Frame {
    /// World to camera (0x44d5ed).
    fn view(&self) -> Mat4 {
        let turn = Mat4::from_rotation_x(-self.pitch) * Mat4::from_rotation_y(self.yaw);
        Mat4::from_translation(Vec3::new(0.0, 0.0, -self.distance)) * turn
    }
}

/// Direction to the light in the camera's space: the viewer's light moves with it,
/// turned by 0.6 both ways (0x41125a).
pub fn light() -> Vec3 {
    Mat3::from_rotation_x(-0.6) * Mat3::from_rotation_y(0.6) * Vec3::Z
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

pub struct Scene {
    format: wgpu::TextureFormat,
    logos: Vec<Logo>,
    programs: Vec<wgpu::RenderPipeline>,
    solid: wgpu::RenderPipeline,
    points: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    added: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    shaded: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    captions: Vec<Vec<wgpu::BindGroup>>,
    badge: wgpu::BindGroup,
    images: wgpu::Buffer,
    targets: Option<Targets>,
}

struct Maker<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    format: wgpu::TextureFormat,
    constants: wgpu::BindGroupLayout,
    textures: HashMap<String, wgpu::Texture>,
    programs: HashMap<Program, usize>,
    pipelines: Vec<wgpu::RenderPipeline>,
    layouts: Vec<wgpu::BindGroupLayout>,
}

struct Kind<'a> {
    module: &'a wgpu::ShaderModule,
    vs: &'a str,
    fs: &'a str,
    group1: Option<&'a wgpu::BindGroupLayout>,
    buffer: wgpu::VertexBufferLayout<'a>,
    strip: bool,
    blend: Option<(F, F)>,
    depth: bool,
}

fn view(t: &wgpu::Texture, cube: bool) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor {
        dimension: cube.then_some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

impl Maker<'_> {
    fn pipeline(&self, k: Kind) -> wgpu::RenderPipeline {
        let d = &self.gpu.device;
        let mut groups = vec![Some(&self.constants)];
        groups.extend(k.group1.map(Some));
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &groups,
            immediate_size: 0,
        });
        let blend = k.blend.map(|(src_factor, dst_factor)| {
            let c = wgpu::BlendComponent {
                src_factor,
                dst_factor,
                operation: wgpu::BlendOperation::Add,
            };
            wgpu::BlendState { color: c, alpha: c }
        });
        d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(k.fs),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: k.module,
                entry_point: Some(k.vs),
                compilation_options: Default::default(),
                buffers: &[Some(k.buffer)],
            },
            fragment: Some(wgpu::FragmentState {
                module: k.module,
                entry_point: Some(k.fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend,
                    // Browsers show the page through a canvas that is not opaque.
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: match k.strip {
                    true => wgpu::PrimitiveTopology::TriangleStrip,
                    false => wgpu::PrimitiveTopology::TriangleList,
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(k.depth),
                depth_compare: Some(match k.depth {
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

    fn module(&self, source: String) -> wgpu::ShaderModule {
        let source = wgpu::ShaderSource::Wgsl(source.into());
        self.gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source,
            })
    }

    fn texture_layout(&self, cubes: &[bool]) -> wgpu::BindGroupLayout {
        let entries: Vec<_> = (0..)
            .zip(cubes)
            .map(|(binding, cube)| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: match cube {
                        true => wgpu::TextureViewDimension::Cube,
                        false => wgpu::TextureViewDimension::D2,
                    },
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        self.gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &entries,
            })
    }

    fn texture_group(
        &self,
        layout: &wgpu::BindGroupLayout,
        views: &[wgpu::TextureView],
    ) -> wgpu::BindGroup {
        let entries: Vec<_> = (0..)
            .zip(views)
            .map(|(binding, v)| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        self.gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &entries,
            })
    }

    fn texture(&mut self, file: &str, cube: bool) -> Result<wgpu::TextureView, Error> {
        let key = file.to_ascii_lowercase();
        if !self.textures.contains_key(&key) {
            let names = match cube {
                true => FACES.map(|f| format!("sunol_{f}_mipmap.dds")).to_vec(),
                false => vec![key.clone()],
            };
            let images: Result<Vec<_>, Error> = names
                .iter()
                .map(|n| Image::read(self.files.get(n)?).map_err(|e| format!("{n}: {e}").into()))
                .collect();
            let t = self
                .gpu
                .upload(&images?)
                .map_err(|e| format!("{file}: {e}"))?;
            self.textures.insert(key.clone(), t);
        }
        Ok(view(&self.textures[&key], cube))
    }

    fn program(&mut self, p: &Program) -> Result<usize, Error> {
        if let Some(i) = self.programs.get(p) {
            return Ok(*i);
        }
        let source = include_str!("../shaders/mesh.wgsl").to_owned() + &p.wgsl()?;
        let module = self.module(source);
        let cubes: Vec<bool> = p.textures.iter().map(|t| t.1).collect();
        let layout = self.texture_layout(&cubes);
        let pipeline = self.pipeline(Kind {
            module: &module,
            vs: "vs",
            fs: "fs",
            group1: Some(&layout),
            buffer: mesh_layout(),
            strip: false,
            blend: None,
            depth: true,
        });
        self.pipelines.push(pipeline);
        self.layouts.push(layout);
        self.programs.insert(p.clone(), self.pipelines.len() - 1);
        Ok(self.pipelines.len() - 1)
    }

    fn logo(&mut self, model: &Model) -> Result<Logo, Error> {
        let mut shapes = Vec::new();
        for s in &model.shapes {
            let vertex = wgpu::BufferUsages::VERTEX;
            let vertices: Vec<Vertex> = s
                .vertices
                .iter()
                .map(|v| Vertex {
                    pos: s.transform.transform_point3(v.pos.into()).into(),
                    normal: s.transform.transform_vector3(v.normal.into()).into(),
                    uv: v.uv,
                })
                .collect();
            let edges = Model::edges(s);
            let ends: Vec<[f32; 3]> = edges
                .iter()
                .flatten()
                .map(|i| vertices[*i as usize].pos)
                .collect();
            let g = self.gpu;
            shapes.push(Shape {
                vertices: g.buffer(bytemuck::cast_slice(&vertices), vertex),
                indices: g.buffer(bytemuck::cast_slice(&s.indices), wgpu::BufferUsages::INDEX),
                count: s.indices.len() as u32,
                points: vertices.len() as u32,
                edges: g.buffer(bytemuck::cast_slice(&ends), vertex),
                edge_count: edges.len() as u32,
            });
        }
        let mut styles = Vec::new();
        for list in &model.styles {
            let mut out = Vec::new();
            for style in list {
                let pipeline = self.program(&style.program)?;
                let views: Result<Vec<_>, Error> = style
                    .textures
                    .iter()
                    .map(|t| self.texture(&t.file, t.cube))
                    .collect();
                out.push(Styled {
                    style: style.clone(),
                    pipeline,
                    textures: Some(self.texture_group(&self.layouts[pipeline], &views?)),
                });
            }
            styles.push(out);
        }
        Ok(Logo { shapes, styles })
    }
}

const MESH: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
const POINT: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
const LINE: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
const IMAGE: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

fn mesh_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &MESH,
    }
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
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
                        has_dynamic_offset: true,
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
            ],
        });
        // The files ask for the nearest mip; blending them avoids visible steps.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniforms = gpu.uniforms(SLOTS);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &constants,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniforms,
                        offset: 0,
                        size,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let mut m = Maker {
            gpu,
            files,
            format,
            constants,
            textures: HashMap::new(),
            programs: HashMap::new(),
            pipelines: Vec::new(),
            layouts: Vec::new(),
        };
        let mut logos = Vec::new();
        for (i, (name, _)) in MODELS.iter().enumerate() {
            let model = files.model(i)?;
            logos.push(m.logo(&model).map_err(|e| format!("{name}: {e}"))?);
        }
        let one = m.texture_layout(&[false]);
        let mut captions = Vec::new();
        for (_, name) in MODELS {
            let mut list = Vec::new();
            for stage in STAGES {
                let v = m.texture(&format!("{name}_{stage}.dds"), false)?;
                list.push(m.texture_group(&one, &[v]));
            }
            captions.push(list);
        }
        let v = m.texture("nvidialogo.dds", false)?;
        let badge = m.texture_group(&one, &[v]);

        let plain = "fn shade(v: V, primary: vec4f) -> vec4f { return primary; }";
        let solid = m.module(include_str!("../shaders/mesh.wgsl").to_owned() + plain);
        let marks = m.module(include_str!("../shaders/marks.wgsl").to_owned());
        let instances = |stride, attributes| wgpu::VertexBufferLayout {
            array_stride: stride,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };
        let mark = |vs, buffer| Kind {
            module: &marks,
            vs,
            fs: "white_fs",
            group1: None,
            buffer,
            strip: true,
            blend: None,
            depth: false,
        };
        let image = |fs, blend| Kind {
            fs,
            group1: Some(&one),
            strip: false,
            blend: Some(blend),
            ..mark(
                "image_vs",
                wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &IMAGE,
                },
            )
        };
        let over = (F::SrcAlpha, F::OneMinusSrcAlpha);
        Ok(Self {
            format,
            solid: m.pipeline(Kind {
                module: &solid,
                vs: "vs",
                fs: "fs",
                group1: None,
                buffer: mesh_layout(),
                strip: false,
                blend: None,
                depth: true,
            }),
            points: m.pipeline(mark("point_vs", instances(32, &POINT))),
            lines: m.pipeline(mark("line_vs", instances(24, &LINE))),
            added: m.pipeline(image("image_fs", (F::One, F::One))),
            blended: m.pipeline(image("image_fs", over)),
            shaded: m.pipeline(image("color_fs", over)),
            logos,
            programs: m.pipelines,
            uniforms,
            group,
            captions,
            badge,
            images: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("images"),
                size: 32 * 18,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            targets: None,
        })
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
        // On screens narrower than 4:3 keep the width instead, so the logo fits.
        let narrow = (4.0 / 3.0 / aspect).max(1.0);
        let fov = ((FOV / 2.0).tan() * narrow).atan() * 2.0;
        let projection = glam::camera::rh::proj::directx::perspective(fov, aspect, 10.0, 2000.0);
        let view = frame.view();
        let light = light();
        // What color0 becomes for bump mapping: the direction in the model's space.
        let local = view.inverse().transform_vector3(light);
        let logo = &self.logos[frame.model];
        let scale = (size.1 as f32 / 768.0).max(1.0);
        let base = Uniform {
            view_proj: (projection * view).to_cols_array(),
            view: view.to_cols_array(),
            color0: [0.0; 4],
            color1: [0.0; 4],
            diffuse: [SOLID, SOLID, SOLID, 1.0],
            light: light.extend(0.0).to_array(),
            screen: [size.0 as f32, size.1 as f32, scale, 0.0],
        };
        let mut data = vec![0u8; (SLOT * SLOTS) as usize];
        let mut put = |slot: usize, u: Uniform| {
            let at = slot * SLOT as usize;
            data[at..at + size_of::<Uniform>()].copy_from_slice(bytemuck::bytes_of(&u));
        };
        put(
            0,
            Uniform {
                // Points are two pixels wide, lines one.
                screen: [
                    size.0 as f32,
                    size.1 as f32,
                    if frame.stage == 0 { scale } else { scale * 0.5 },
                    0.0,
                ],
                ..base
            },
        );
        let styles = frame.stage.checked_sub(3).map(|s| &logo.styles[s]);
        for (i, s) in styles.into_iter().flatten().enumerate() {
            let color0 = match s.style.light_in_color0 {
                true => (local * 0.5 + 0.5).extend(1.0),
                false => s.style.color0,
            };
            put(
                i + 1,
                Uniform {
                    color0: color0.to_array(),
                    color1: s.style.color1.to_array(),
                    diffuse: s.style.diffuse.to_array(),
                    ..base
                },
            );
        }
        gpu.queue.write_buffer(&self.uniforms, 0, &data);

        // Images keep the shapes they have on a 4:3 screen.
        let (wide, tall) = ((4.0 / 3.0 / aspect).min(1.0), 1.0 / narrow);
        // Larger on phones, where the text would be too small to read.
        let caption = narrow.min(1.6);
        let quad = |x0: f32, y0: f32, w: f32, h: f32, color: [f32; 4]| {
            let v = |x: f32, y: f32| {
                let (u, t) = ((x - x0) / w, 1.0 - (y - y0) / h);
                [x, y, u, t, color[0], color[1], color[2], color[3]]
            };
            let (x1, y1) = (x0 + w, y0 + h);
            [
                v(x0, y0),
                v(x1, y0),
                v(x0, y1),
                v(x0, y1),
                v(x1, y0),
                v(x1, y1),
            ]
        };
        let white = [1.0; 4];
        let images = [
            quad(
                -1.0,
                -1.0,
                0.6 * caption * wide,
                0.8 * caption * tall,
                white,
            ),
            quad(1.0 - 0.4 * wide, -1.0, 0.4 * wide, 0.14 * tall, white),
            quad(
                -1.0,
                -1.0,
                2.0,
                2.0,
                [0.0, 0.0, 0.0, 1.0 - frame.brightness],
            ),
        ];
        gpu.queue
            .write_buffer(&self.images, 0, bytemuck::cast_slice(&images));

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
        rp.set_bind_group(0, &self.group, &[0]);
        match frame.stage {
            0 => rp.set_pipeline(&self.points),
            1 => rp.set_pipeline(&self.lines),
            2 => rp.set_pipeline(&self.solid),
            _ => {}
        }
        for (i, s) in logo.shapes.iter().enumerate() {
            match frame.stage {
                0 => {
                    rp.set_vertex_buffer(0, s.vertices.slice(..));
                    rp.draw(0..4, 0..s.points);
                    continue;
                }
                1 => {
                    rp.set_vertex_buffer(0, s.edges.slice(..));
                    rp.draw(0..4, 0..s.edge_count);
                    continue;
                }
                2 => {}
                stage => {
                    let styled = &logo.styles[stage - 3][i];
                    rp.set_pipeline(&self.programs[styled.pipeline]);
                    rp.set_bind_group(0, &self.group, &[(i as u32 + 1) * SLOT as u32]);
                    rp.set_bind_group(1, &styled.textures, &[]);
                }
            }
            rp.set_vertex_buffer(0, s.vertices.slice(..));
            rp.set_index_buffer(s.indices.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..s.count, 0, 0..1);
        }
        rp.set_bind_group(0, &self.group, &[0]);
        rp.set_vertex_buffer(0, self.images.slice(..));
        let mut image = |pipe, group: &wgpu::BindGroup, i: u32| {
            rp.set_pipeline(pipe);
            rp.set_bind_group(1, group, &[]);
            rp.draw(i * 6..i * 6 + 6, 0..1);
        };
        if frame.logo {
            image(&self.blended, &self.badge, 1);
        }
        if frame.caption {
            image(&self.added, &self.captions[frame.model][frame.stage], 0);
        }
        if frame.brightness < 1.0 {
            image(&self.shaded, &self.badge, 2);
        }
    }
}
