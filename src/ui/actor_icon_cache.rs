//! GPU texture cache for voice-actor icons.
#![allow(clippy::too_many_arguments)]

use std::collections::{HashMap, HashSet};

use crate::project::Project;
use crate::voice_actor::{decode_icon_rgba, icon_hash, VoiceActor, VOICE_ACTOR_ICON_SIZE};

struct CachedActorIcon {
    hash: u64,
    icon_ptr: usize,
    icon_len: usize,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

struct FailedActorIcon {
    hash: u64,
    icon_ptr: usize,
    icon_len: usize,
}

pub struct ActorIconCache {
    entries: HashMap<String, CachedActorIcon>,
    failures: HashMap<String, FailedActorIcon>,
}

impl Default for ActorIconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorIconCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            failures: HashMap::new(),
        }
    }

    pub fn sync(
        &mut self,
        project: &Project,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let active_names: HashSet<&str> = project
            .voice_actors()
            .iter()
            .map(|actor| actor.name.as_str())
            .collect();
        self.entries
            .retain(|name, _| active_names.contains(name.as_str()));
        self.failures
            .retain(|name, _| active_names.contains(name.as_str()));

        for actor in project.voice_actors() {
            let Some(icon_data) = actor.icon_png_base64.as_deref() else {
                self.entries.remove(&actor.name);
                self.failures.remove(&actor.name);
                continue;
            };
            let icon_ptr = icon_data.as_ptr() as usize;
            let icon_len = icon_data.len();
            if self
                .entries
                .get(&actor.name)
                .is_some_and(|cached| cached.icon_ptr == icon_ptr && cached.icon_len == icon_len)
            {
                continue;
            }
            if self
                .failures
                .get(&actor.name)
                .is_some_and(|failed| failed.icon_ptr == icon_ptr && failed.icon_len == icon_len)
            {
                continue;
            }

            let hash = icon_hash(icon_data);
            if let Some(cached) = self.entries.get_mut(&actor.name) {
                if cached.hash == hash {
                    cached.icon_ptr = icon_ptr;
                    cached.icon_len = icon_len;
                    continue;
                }
            }
            if let Some(failed) = self.failures.get_mut(&actor.name) {
                if failed.hash == hash {
                    failed.icon_ptr = icon_ptr;
                    failed.icon_len = icon_len;
                    continue;
                }
            }

            match create_cached_icon(
                actor,
                icon_data,
                hash,
                device,
                queue,
                bind_group_layout,
                sampler,
                icon_ptr,
                icon_len,
            ) {
                Ok(cached) => {
                    self.entries.insert(actor.name.clone(), cached);
                    self.failures.remove(&actor.name);
                }
                Err(e) => {
                    log::warn!("Failed to cache voice actor icon '{}': {e}", actor.name);
                    self.entries.remove(&actor.name);
                    self.failures.insert(
                        actor.name.clone(),
                        FailedActorIcon {
                            hash,
                            icon_ptr,
                            icon_len,
                        },
                    );
                }
            }
        }
    }

    pub fn bind_group_for(&self, actor: &VoiceActor) -> Option<&wgpu::BindGroup> {
        self.entries
            .get(&actor.name)
            .map(|cached| &cached.bind_group)
    }
}

fn create_cached_icon(
    actor: &VoiceActor,
    icon_data: &str,
    hash: u64,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    icon_ptr: usize,
    icon_len: usize,
) -> Result<CachedActorIcon, String> {
    let rgba = decode_icon_rgba(icon_data)?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Voice Actor Icon"),
        size: wgpu::Extent3d {
            width: VOICE_ACTOR_ICON_SIZE,
            height: VOICE_ACTOR_ICON_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * VOICE_ACTOR_ICON_SIZE),
            rows_per_image: Some(VOICE_ACTOR_ICON_SIZE),
        },
        wgpu::Extent3d {
            width: VOICE_ACTOR_ICON_SIZE,
            height: VOICE_ACTOR_ICON_SIZE,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group_label = format!("Voice Actor Icon {}", actor.name);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(bind_group_label.as_str()),
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

    Ok(CachedActorIcon {
        hash,
        icon_ptr,
        icon_len,
        _texture: texture,
        _view: view,
        bind_group,
    })
}
