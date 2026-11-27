//! The solver's passes in the original's order (`Fluid.cpp`, 0x40a7a0).
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};
use wgpu::util::DeviceExt;

use crate::Error;
use crate::data::Files;
use crate::gpu::{FIELD, Gpu, SLOT};

pub const DIM: IVec3 = IVec3::new(128, 64, 64);
/// Slices per row of the atlas.
pub const COLS: i32 = 8;
pub const ATLAS: (u32, u32) = (1024, 512);
const JACOBI: usize = 10;
/// Uniform slots per frame; several steps may share one submission.
const SLOTS: u64 = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub water: bool,
    pub timestep: f32,
    /// Inverse radius of the source.
    pub size: f32,
    pub saturation: f32,
    /// Smoke kept per step.
    pub fade: f32,
    /// Vorticity confinement strength.
    pub confinement: f32,
    /// Second-order advection.
    pub bfecc: bool,
}

impl Settings {
    pub fn smoke() -> Self {
        Self {
            water: false,
            timestep: 2.0,
            size: 0.14,
            saturation: 0.28,
            fade: 0.985,
            confinement: 0.1,
            bfecc: true,
        }
    }
    pub fn logo() -> Self {
        Self {
            saturation: 0.0,
            fade: 1.0,
            confinement: 0.19,
            bfecc: false,
            ..Self::smoke()
        }
    }
    pub fn water(old: &Self) -> Self {
        Self {
            water: true,
            timestep: 4.0,
            size: 0.18,
            fade: 1.0,
            bfecc: true,
            ..*old
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Inject {
    /// Nothing is injected until this is set (0x18 in the original object).
    pub active: bool,
    /// Adds smoke (0x1a).
    pub color: bool,
    /// Adds velocity (0x1b).
    pub push: bool,
    pub pos: Vec3,
    pub velocity: Vec3,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Params {
    dim: [i32; 3],
    cols: i32,
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}

#[derive(Clone, Copy)]
enum Blend {
    None,
    Add,
    /// Source alpha over.
    Over,
    /// Source alpha, plus destination.
    Glow,
}

macro_rules! passes {
    ($($name:ident $entry:literal $blend:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        enum Pass { $($name),* }
        const PASSES: &[(&str, Blend)] = &[$(($entry, Blend::$blend)),*];
    };
}
passes! {
    Advect "advect" None,
    Bfecc "advect_bfecc" None,
    Boundary "boundary" None,
    Vorticity "vorticity" None,
    Confinement "confinement" Add,
    Gaussian "gaussian" Over,
    Divergence "divergence" None,
    Jacobi "jacobi" None,
    Project "project" None,
    Decal "decal" Over,
    LevelInit "level_init" None,
    Lid "lid" None,
    Inject "inject" Add,
    Gradient "gradient" None,
    Gravity "gravity" Glow,
}

struct Field {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub struct Sim {
    pipelines: Vec<wgpu::RenderPipeline>,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    used: u64,
    velocity: Field,
    /// Smoke color and density, or the water's level set.
    density: Field,
    pressure: Field,
    vorticity: Field,
    divergence: Field,
    scratch: Field,
    logo: wgpu::TextureView,
    phase: f32,
    rng: u32,
}

impl Sim {
    pub fn new(gpu: &Gpu, files: &Files) -> Result<Self, Error> {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::include_wgsl!("../shaders/sim.wgsl"));
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                },
                count: None,
            }],
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[0, 1, 2].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&uniform_layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = PASSES
            .iter()
            .map(|&(entry, blend)| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &module,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: FIELD,
                            blend: blend.state(),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        let uniforms = gpu.uniforms(SLOTS);
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                }),
            }],
        });
        let logo = files.get("textures/logo.raw")?;
        if logo.len() != 128 * 64 * 4 {
            return Err("logo.raw has an unexpected size".into());
        }
        let logo = device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("logo"),
                size: wgpu::Extent3d {
                    width: 128,
                    height: 64,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::gpu::COLOR,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            logo,
        );
        let field = || {
            let texture = gpu.target(ATLAS.0, ATLAS.1, FIELD);
            let view = texture.create_view(&Default::default());
            Field { texture, view }
        };
        Ok(Self {
            pipelines,
            layout,
            uniforms,
            uniform_group,
            used: 0,
            velocity: field(),
            density: field(),
            pressure: field(),
            vorticity: field(),
            divergence: field(),
            scratch: field(),
            logo: logo.create_view(&Default::default()),
            phase: 0.0,
            rng: 1,
        })
    }

    /// Restarts the color cycle and the random numbers.
    pub fn rewind(&mut self) {
        (self.phase, self.rng) = (0.0, 1);
    }

    /// Call once per submission, before recording.
    pub fn begin(&mut self) {
        self.used = 0;
    }

    /// The smoke or level set, for drawing.
    pub fn density(&self) -> &wgpu::TextureView {
        &self.density.view
    }
    pub fn density_texture(&self) -> &wgpu::Texture {
        &self.density.texture
    }
    pub fn velocity_texture(&self) -> &wgpu::Texture {
        &self.velocity.texture
    }

    pub fn reset(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, water: bool) {
        for f in [&self.velocity, &self.pressure, &self.density] {
            let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
            drop(begin(enc, &f.view, clear));
        }
        if water {
            let mut r = Recorder::new(self, gpu, enc);
            r.run(Pass::LevelInit, &[], Out::Density, Params::default());
        }
    }

    pub fn stamp_logo(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder) {
        let mut r = Recorder::new(self, gpu, enc);
        let p = Params {
            a: [27.0, 37.0, 0.0, 0.0],
            ..Default::default()
        };
        r.run(Pass::Decal, &[In::Logo], Out::Density, p);
    }

    pub fn step(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, s: &Settings, i: &Inject) {
        let dt = s.timestep;
        let (color, centers) = (self.splat_color(s, i), self.jitter(i));
        let mut r = Recorder::new(self, gpu, enc);
        // a: timestep, interior scale, boundary scale, direction.
        let advect = |inner: f32, edge: f32, dir: f32| Params {
            a: [dt, inner, edge, dir],
            ..Default::default()
        };
        if s.water {
            r.run(Pass::Lid, &[], Out::Velocity, Params::default());
            r.swap_density(Pass::Boundary, &[In::Density], Params::default());
        }
        if s.bfecc {
            use In::{Density, Divergence, Scratch, Velocity};
            let there = advect(1.0, 1.0, 1.0);
            r.run(Pass::Advect, &[Velocity, Density], Out::Scratch, there);
            let back = advect(1.0, 1.0, -1.0);
            r.run(Pass::Advect, &[Velocity, Scratch], Out::Divergence, back);
            let (limit, inner, edge) = match s.water {
                true => (128.0, 1.0, -1.0),
                false => (1.0, s.fade, s.fade),
            };
            let p = Params {
                b: [limit, 0.0, 0.0, 0.0],
                ..advect(inner, edge, 1.0)
            };
            r.swap_density(Pass::Bfecc, &[Velocity, Density, Divergence], p);
        } else {
            let p = advect(s.fade, s.fade, 1.0);
            r.swap_density(Pass::Advect, &[In::Velocity, In::Density], p);
        }
        let p = advect(1.0, -1.0, 1.0);
        r.swap_velocity(Pass::Advect, &[In::Velocity, In::Velocity], p);
        if s.water {
            r.run(Pass::Inject, &[], Out::Density, Params::default());
            r.swap_density(Pass::Gradient, &[In::Density], Params::default());
        } else {
            r.run(
                Pass::Vorticity,
                &[In::Velocity],
                Out::Vorticity,
                Params::default(),
            );
            let p = Params {
                a: [dt, s.confinement, 0.0, 0.0],
                ..Default::default()
            };
            r.run(Pass::Confinement, &[In::Vorticity], Out::Velocity, p);
        }
        if i.active {
            let splat = |at: Vec3, color: Vec3| Params {
                a: at.extend(s.size).to_array(),
                b: color.extend(1.0).to_array(),
                ..Default::default()
            };
            if !s.water && i.color {
                r.run(Pass::Gaussian, &[], Out::Density, splat(i.pos, color));
            }
            if i.push || (i.color && !s.water) {
                r.run(
                    Pass::Gaussian,
                    &[],
                    Out::Velocity,
                    splat(centers, i.velocity),
                );
            }
        }
        if s.water {
            let p = Params {
                a: [-0.01 * dt, 0.0, 0.0, 0.0],
                ..Default::default()
            };
            r.run(Pass::Gravity, &[In::Density], Out::Velocity, p);
        }
        r.run(
            Pass::Divergence,
            &[In::Velocity],
            Out::Divergence,
            Params::default(),
        );
        for _ in 0..JACOBI {
            let inputs = [In::Pressure, In::Divergence];
            r.run(Pass::Jacobi, &inputs, Out::Scratch, Params::default());
            std::mem::swap(&mut r.sim.pressure, &mut r.sim.scratch);
        }
        r.swap_velocity(
            Pass::Project,
            &[In::Velocity, In::Pressure],
            Params::default(),
        );
    }

    /// The color cycles through hues, strength set by the saturation.
    fn splat_color(&mut self, s: &Settings, i: &Inject) -> Vec3 {
        if !(i.active && i.color) || s.water {
            return Vec3::ZERO;
        }
        self.phase += 0.02;
        let hue = |offset: f32| {
            let wave = ((self.phase + offset).sin() + 1.0) * 0.5;
            ((1.0 - s.saturation) + wave * s.saturation) * 1.5
        };
        Vec3::new(hue(2.094_395), hue(4.188_79), hue(std::f32::consts::TAU))
    }

    /// The velocity source moves up to 2.5 cells at random. Uses the C library's
    /// `rand` of the original's compiler.
    fn jitter(&mut self, i: &Inject) -> Vec3 {
        let mut next = || {
            self.rng = self.rng.wrapping_mul(214_013).wrapping_add(2_531_011);
            (((self.rng >> 16) & 0x7fff) as f32 / 32767.0 - 0.5) * 5.0
        };
        // The original draws z first.
        let (z, y, x) = (next(), next(), next());
        i.pos + Vec3::new(x, y, z)
    }
}

impl Blend {
    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::BlendFactor::{One, OneMinusSrcAlpha, SrcAlpha};
        let (src_factor, dst_factor) = match self {
            Self::None => return None,
            Self::Add => (One, One),
            Self::Over => (SrcAlpha, OneMinusSrcAlpha),
            Self::Glow => (SrcAlpha, One),
        };
        let c = wgpu::BlendComponent {
            src_factor,
            dst_factor,
            operation: wgpu::BlendOperation::Add,
        };
        Some(wgpu::BlendState { color: c, alpha: c })
    }
}

#[derive(Clone, Copy)]
enum In {
    Velocity,
    Density,
    Pressure,
    Vorticity,
    Divergence,
    Scratch,
    Logo,
}

#[derive(Clone, Copy)]
enum Out {
    Velocity,
    Density,
    Vorticity,
    Divergence,
    Scratch,
}

fn begin<'e>(
    enc: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    })
}

struct Recorder<'a> {
    sim: &'a mut Sim,
    gpu: &'a Gpu,
    enc: &'a mut wgpu::CommandEncoder,
}

impl<'a> Recorder<'a> {
    fn new(sim: &'a mut Sim, gpu: &'a Gpu, enc: &'a mut wgpu::CommandEncoder) -> Self {
        Self { sim, gpu, enc }
    }

    fn swap_density(&mut self, pass: Pass, inputs: &[In], p: Params) {
        self.run(pass, inputs, Out::Scratch, p);
        std::mem::swap(&mut self.sim.density, &mut self.sim.scratch);
    }

    fn swap_velocity(&mut self, pass: Pass, inputs: &[In], p: Params) {
        self.run(pass, inputs, Out::Scratch, p);
        std::mem::swap(&mut self.sim.velocity, &mut self.sim.scratch);
    }

    fn run(&mut self, pass: Pass, inputs: &[In], out: Out, mut p: Params) {
        let s = &mut *self.sim;
        (p.dim, p.cols) = (DIM.to_array(), COLS);
        assert!(s.used < SLOTS, "too many passes in one submission");
        let offset = s.used * SLOT;
        s.used += 1;
        let queue = &self.gpu.queue;
        queue.write_buffer(&s.uniforms, offset, bytemuck::bytes_of(&p));
        let target = match out {
            Out::Velocity => &s.velocity,
            Out::Density => &s.density,
            Out::Vorticity => &s.vorticity,
            Out::Divergence => &s.divergence,
            Out::Scratch => &s.scratch,
        };
        let input = |i: usize| match inputs.get(i) {
            Some(In::Velocity) => &s.velocity.view,
            Some(In::Density) => &s.density.view,
            Some(In::Pressure) => &s.pressure.view,
            Some(In::Vorticity) => &s.vorticity.view,
            Some(In::Divergence) => &s.divergence.view,
            Some(In::Scratch) => &s.scratch.view,
            Some(In::Logo) | None => &s.logo,
        };
        let entries = [0, 1, 2].map(|i| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(input(i)),
        });
        let device = &self.gpu.device;
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &s.layout,
            entries: &entries,
        });
        let mut rp = begin(self.enc, &target.view, wgpu::LoadOp::Load);
        rp.set_pipeline(&s.pipelines[pass as usize]);
        rp.set_bind_group(0, &s.uniform_group, &[offset as u32]);
        rp.set_bind_group(1, &group, &[]);
        rp.draw(0..3, 0..1);
    }
}
