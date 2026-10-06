//! GPU textures of the karaoke dot styles shown in the editor: the project's
//! dot on the band and the choices of the band style window.

use std::collections::HashMap;

use crate::band_style::KaraokeDot;
use crate::karaoke_dot::{mip_chain, texture_key, DotFace, TEXTURE_BASE_SIZE};

struct CachedDotTexture {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

#[derive(Default)]
pub struct KaraokeDotTextureCache {
    entries: HashMap<u64, CachedDotTexture>,
    /// Keys that failed to rasterize, so they are not retried every frame.
    failures: HashMap<u64, ()>,
}

impl KaraokeDotTextureCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes sure every dot in `dots` has a texture and drops the textures
    /// of custom images no longer in use (built-in shapes stay cached).
    pub fn sync<'a>(
        &mut self,
        dots: impl IntoIterator<Item = &'a KaraokeDot>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let builtin_keys: Vec<u64> = KaraokeDot::BUILTINS
            .iter()
            .filter_map(|dot| texture_key(dot, DotFace::Ground))
            .collect();
        let mut used: Vec<u64> = builtin_keys.clone();
        for dot in dots {
            for face in DotFace::ALL {
                let Some(key) = texture_key(dot, face) else {
                    continue;
                };
                used.push(key);
                if self.entries.contains_key(&key) || self.failures.contains_key(&key) {
                    continue;
                }
                match create_texture(dot, face, device, queue, bind_group_layout, sampler) {
                    Some(cached) => {
                        self.entries.insert(key, cached);
                    }
                    None => {
                        log::warn!("Failed to rasterize karaoke dot texture");
                        self.failures.insert(key, ());
                    }
                }
            }
        }
        self.entries.retain(|key, _| used.contains(key));
        self.failures.retain(|key, _| used.contains(key));
    }

    pub fn bind_group_for(&self, dot: &KaraokeDot, face: DotFace) -> Option<&wgpu::BindGroup> {
        self.bind_group_for_key(texture_key(dot, face)?)
    }

    pub fn bind_group_for_key(&self, key: u64) -> Option<&wgpu::BindGroup> {
        self.entries.get(&key).map(|cached| &cached.bind_group)
    }
}

fn create_texture(
    dot: &KaraokeDot,
    face: DotFace,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> Option<CachedDotTexture> {
    let levels = mip_chain(dot, face, TEXTURE_BASE_SIZE)?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Karaoke Dot"),
        size: wgpu::Extent3d {
            width: TEXTURE_BASE_SIZE,
            height: TEXTURE_BASE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip_level, (size, rgba)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size),
                rows_per_image: Some(*size),
            },
            wgpu::Extent3d {
                width: *size,
                height: *size,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Karaoke Dot"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    Some(CachedDotTexture {
        _texture: texture,
        _view: view,
        bind_group,
    })
}
