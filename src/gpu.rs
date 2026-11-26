//! Device creation and small wgpu helpers.
use wgpu::util::DeviceExt;

use crate::Error;
use crate::dds::{self, Image};

/// The original works in gamma space, so nothing is an sRGB format.
pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const SAMPLES: u32 = 4;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
/// Spacing of uniform slots; every backend accepts 256.
pub const SLOT: u64 = 256;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, Error> {
        // Compressed textures go to the GPU as they are where it knows them.
        let wanted = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let required_features = adapter.features() & wanted;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("tidepool"),
                required_features,
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

    pub fn target(
        &self,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> wgpu::Texture {
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if samples == 1 {
            usage |= wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING;
        }
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size(w, h, 1),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// Uploads images of one size, as layers.
    pub fn upload(&self, images: &[Image]) -> Result<wgpu::Texture, Error> {
        let first = images.first().ok_or("no image")?;
        let levels = first.levels.len();
        let compressed = self
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let format = match (compressed, first.format) {
            (true, dds::Format::Dxt1) => wgpu::TextureFormat::Bc1RgbaUnorm,
            (true, dds::Format::Dxt3) => wgpu::TextureFormat::Bc2RgbaUnorm,
            (true, dds::Format::Dxt5) => wgpu::TextureFormat::Bc3RgbaUnorm,
            (false, _) => COLOR,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size(first.width as u32, first.height as u32, images.len() as u32),
            mip_level_count: levels as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, i) in images.iter().enumerate() {
            if (i.width, i.height, i.format, i.levels.len())
                != (first.width, first.height, first.format, levels)
            {
                return Err("images differ in size or format".into());
            }
            for (k, data) in i.levels.iter().enumerate() {
                let (w, h) = ((i.width >> k).max(1), (i.height >> k).max(1));
                let decoded;
                let (data, row, w, h) = if compressed {
                    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                    (*data, bw * i.format.block(), bw * 4, bh * 4)
                } else {
                    decoded = i.decode(k).2;
                    (decoded.as_slice(), w * 4, w, h)
                };
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: k as u32,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row as u32),
                        rows_per_image: None,
                    },
                    size(w as u32, h as u32, 1),
                );
            }
        }
        Ok(texture)
    }

    /// Uploads RGBA images of one size, as layers, with box-filtered mips if asked.
    pub fn upload_rgba(&self, side: (u32, u32), layers: &[Vec<u8>], mips: bool) -> wgpu::Texture {
        let levels = if mips {
            side.0.max(side.1).ilog2() + 1
        } else {
            1
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: size(side.0, side.1, layers.len() as u32),
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, image) in layers.iter().enumerate() {
            let (mut pixels, mut w, mut h) = (image.clone(), side.0, side.1);
            for level in 0..levels {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        rows_per_image: None,
                    },
                    size(w, h, 1),
                );
                let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
                let (dx, dy) = ((w > 1) as u32, (h > 1) as u32);
                let mut half = Vec::with_capacity((hw * hh * 4) as usize);
                for (y, x) in (0..hh).flat_map(|y| (0..hw).map(move |x| (y, x))) {
                    for c in 0..4 {
                        let at = |i: u32, j: u32| u32::from(pixels[((j * w + i) * 4 + c) as usize]);
                        let (x0, y0) = (x << dx, y << dy);
                        let sum =
                            at(x0, y0) + at(x0 + dx, y0) + at(x0, y0 + dy) + at(x0 + dx, y0 + dy);
                        half.push((sum / 4) as u8);
                    }
                }
                (pixels, w, h) = (half, hw, hh);
            }
        }
        texture
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
