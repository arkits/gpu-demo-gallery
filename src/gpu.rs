//! Device creation and small wgpu helpers.
use wgpu::util::DeviceExt;

use crate::Error;

/// The original works in gamma space, so nothing is an sRGB format.
pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const FIELD: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
/// Spacing of uniform slots; every backend accepts 256.
pub const SLOT: u64 = 256;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, Error> {
        let field = adapter.get_texture_format_features(FIELD);
        if !field
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err("this GPU cannot render to half-float textures".into());
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("box of smoke"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        Ok(Self { device, queue })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless() -> Result<Self, Error> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
        pollster::block_on(Self::from_adapter(&adapter))
    }

    pub fn target(&self, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size(w, h, 1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// Uploads square-power-of-two images (one per layer) with box-filtered mips,
    /// as `gluBuild2DMipmaps` makes them.
    pub fn mipped(&self, side: u32, layers: &[Vec<u8>]) -> wgpu::Texture {
        let levels = side.ilog2() + 1;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size(side, side, layers.len() as u32),
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, image) in layers.iter().enumerate() {
            let (mut pixels, mut s) = (image.clone(), side);
            for level in 0..levels {
                self.write(&texture, level, layer as u32, s, s, &pixels);
                if s > 1 {
                    pixels = halve(&pixels, s as usize);
                    s /= 2;
                }
            }
        }
        texture
    }

    pub fn write(&self, t: &wgpu::Texture, mip: u32, layer: u32, w: u32, h: u32, data: &[u8]) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: mip,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(data.len() as u32 / h),
                rows_per_image: Some(h),
            },
            size(w, h, 1),
        );
    }

    pub fn buffer(&self, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: data,
                usage,
            })
    }

    pub fn uniforms(&self, slots: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: SLOT * slots,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Texel rows of a texture, tightly packed. Native only: it waits.
    pub fn read(&self, t: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (t.width(), t.height());
        let texel = t.format().block_copy_size(None).expect("plain format");
        let padded = (w * texel).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            t.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            t.size(),
        );
        self.queue.submit([enc.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll device");
        let data = buffer.slice(..).get_mapped_range().expect("mapped range");
        data.chunks(padded as usize)
            .flat_map(|r| r[..(w * texel) as usize].to_vec())
            .collect()
    }
}

fn size(w: u32, h: u32, layers: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: layers,
    }
}

fn halve(p: &[u8], s: usize) -> Vec<u8> {
    let h = s / 2;
    let mut out = Vec::with_capacity(h * h * 4);
    for y in 0..h {
        for x in 0..h {
            for c in 0..4 {
                let at = |dx, dy| u32::from(p[((2 * y + dy) * s + 2 * x + dx) * 4 + c]);
                out.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
            }
        }
    }
    out
}

/// Expands 1- or 3-channel pixels to RGBA.
pub fn rgba(data: &[u8], channels: usize) -> Vec<u8> {
    data.chunks_exact(channels)
        .flat_map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            _ => [p[0], p[1], p[2], 255],
        })
        .collect()
}
