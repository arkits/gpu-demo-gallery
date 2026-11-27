//! Device creation, texture upload and offscreen targets.
use sushi_formats::texture::Texture as SourceTexture;

/// The original renders in gamma space, so nothing here is an sRGB format.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, wgpu::RequestDeviceError> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sushi"),
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
        let faces = src.decode()?;
        let base = &faces[0][0];
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&src.name),
            size: extent(base.width, base.height, faces.len()),
            mip_level_count: faces[0].len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
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
                        bytes_per_row: Some(l.width as u32 * 4),
                        rows_per_image: Some(l.height as u32),
                    },
                    extent(l.width, l.height, 1),
                );
            }
        }
        Ok(texture)
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
        data.chunks(padded as usize)
            .flat_map(|r| r[..w as usize * 4].to_vec())
            .collect()
    }
}

pub fn extent(w: usize, h: usize, layers: usize) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: w as u32,
        height: h as u32,
        depth_or_array_layers: layers as u32,
    }
}
