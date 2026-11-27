//! Device creation, texture upload and offscreen targets.
use sushi_formats::texture::{Format, Texture as SourceTexture};

/// The original renders in gamma space, so nothing here is an sRGB format.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Stands in for 16 bit integers, which browsers cannot draw to or filter everywhere.
pub const DEEP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Ten bits per color.
pub const TEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
/// Shadow maps: depth as color, which every browser can read.
pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, wgpu::RequestDeviceError> {
        let bc = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sushi"),
                required_features: adapter.features() & bc,
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        Ok(Self { device, queue })
    }

    pub async fn headless() -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        Ok(Self::from_adapter(&adapter).await?)
    }

    /// Uploads a decoded texture with its full mip chain.
    pub fn upload(&self, src: &SourceTexture) -> sushi_formats::Result<wgpu::Texture> {
        use wgpu::TextureFormat as F;
        let compressed = self
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            && src.width().is_multiple_of(4)
            && src.height().is_multiple_of(4);
        let blocks = if compressed { src.blocks()? } else { None };
        let block = blocks.is_some();
        let faces = match blocks {
            Some(b) => b,
            None => src.decode()?,
        };
        let base = &faces[0][0];
        let format = match src.format() {
            Some(Format::Dxt1) if block => F::Bc1RgbaUnorm,
            Some(Format::Dxt5) if block => F::Bc3RgbaUnorm,
            Some(Format::Ati1n) if block => F::Bc4RUnorm,
            Some(Format::Ati2n) if block => F::Bc5RgUnorm,
            Some(Format::V16u16) => F::Rg16Float,
            Some(Format::Abgr16) => F::Rgba16Float,
            Some(Format::A2r10g10b10) => F::Rgb10a2Unorm,
            Some(Format::S8888) => F::Rgba8Snorm,
            _ => COLOR_FORMAT,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&src.name),
            size: extent(base.width, base.height, faces.len()),
            mip_level_count: faces[0].len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, mips) in faces.iter().enumerate() {
            for (mip, l) in mips.iter().enumerate() {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: mip as u32,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &l.rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(
                            (l.rgba.len()
                                / if block {
                                    l.height.div_ceil(4)
                                } else {
                                    l.height
                                }) as u32,
                        ),
                        rows_per_image: None,
                    },
                    match block {
                        // Sizes are of whole blocks.
                        true => {
                            extent(l.width.next_multiple_of(4), l.height.next_multiple_of(4), 1)
                        }
                        false => extent(l.width, l.height, 1),
                    },
                );
            }
        }
        Ok(texture)
    }

    pub fn picture(&self, level: &sushi_formats::texture::Level) -> wgpu::Texture {
        use wgpu::util::DeviceExt;
        self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("picture"),
                size: extent(level.width, level.height, 1),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &level.rgba,
        )
    }

    /// Reads an RGBA8 texture back as tightly packed rows.
    pub fn read_rgba(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (texture.width(), texture.height());
        let padded = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        self.queue.submit([enc.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll device");
        let data = buffer.slice(..).get_mapped_range().expect("mapped range");
        let mut rgba: Vec<u8> = data
            .chunks(padded as usize)
            .flat_map(|r| r[..w as usize * 4].to_vec())
            .collect();
        if texture.format() == TEN_FORMAT {
            for pixel in rgba.chunks_exact_mut(4) {
                let packed = u32::from_le_bytes(pixel.try_into().unwrap());
                for (bits, channel) in pixel[..3].iter_mut().enumerate() {
                    *channel = (((packed >> (10 * bits)) & 1023) * 255 / 1023) as u8;
                }
                pixel[3] = ((packed >> 30) * 85) as u8;
            }
        }
        rgba
    }
}

pub fn extent(w: usize, h: usize, layers: usize) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: w as u32,
        height: h as u32,
        depth_or_array_layers: layers as u32,
    }
}
