use std::collections::HashMap;

const ICON_SIZE: u32 = 32;
const RHUBARB_ICON_SIZE: u32 = 128;
const STRETCHABLE_ICON_WIDTH: u32 = 512;
const STRETCHABLE_ICON_HEIGHT: u32 = 64;

pub struct IconAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    icon_positions: HashMap<String, (u32, u32, u32, u32)>,
    atlas_width: u32,
    atlas_height: u32,
}

#[derive(Clone, Copy)]
enum AtlasEntryKind {
    Svg,
    Png,
}

struct AtlasEntry {
    name: &'static str,
    data: &'static [u8],
    kind: AtlasEntryKind,
    width: u32,
    height: u32,
    flip_h: bool,
}

impl AtlasEntry {
    const fn svg(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            kind: AtlasEntryKind::Svg,
            width: ICON_SIZE,
            height: ICON_SIZE,
            flip_h: false,
        }
    }

    const fn flipped_svg(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            kind: AtlasEntryKind::Svg,
            width: ICON_SIZE,
            height: ICON_SIZE,
            flip_h: true,
        }
    }

    const fn stretchable_svg(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            kind: AtlasEntryKind::Svg,
            width: STRETCHABLE_ICON_WIDTH,
            height: STRETCHABLE_ICON_HEIGHT,
            flip_h: false,
        }
    }

    const fn rhubarb_png(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            kind: AtlasEntryKind::Png,
            width: RHUBARB_ICON_SIZE,
            height: RHUBARB_ICON_SIZE,
            flip_h: false,
        }
    }

    fn rasterize(&self) -> Vec<u8> {
        let mut pixels = match self.kind {
            AtlasEntryKind::Svg => {
                let tree =
                    resvg::usvg::Tree::from_data(self.data, &resvg::usvg::Options::default())
                        .expect("Failed to parse SVG");
                let mut pixmap = resvg::tiny_skia::Pixmap::new(self.width, self.height).unwrap();
                let svg_size = tree.size();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(
                        self.width as f32 / svg_size.width(),
                        self.height as f32 / svg_size.height(),
                    ),
                    &mut pixmap.as_mut(),
                );
                let mut pixels = pixmap.data().to_vec();
                for chunk in pixels.chunks_exact_mut(4) {
                    let alpha = chunk[3] as f32 / 255.0;
                    chunk[0] = (255.0 * alpha) as u8;
                    chunk[1] = (255.0 * alpha) as u8;
                    chunk[2] = (255.0 * alpha) as u8;
                }
                pixels
            }
            AtlasEntryKind::Png => {
                let source = image::load_from_memory(self.data)
                    .expect("Failed to decode PNG icon")
                    .to_rgba8();
                let (source_width, source_height) = source.dimensions();
                let scale = (self.width as f32 / source_width.max(1) as f32)
                    .min(self.height as f32 / source_height.max(1) as f32);
                let resized_width =
                    ((source_width as f32 * scale).round() as u32).clamp(1, self.width);
                let resized_height =
                    ((source_height as f32 * scale).round() as u32).clamp(1, self.height);
                let resized = image::imageops::resize(
                    &source,
                    resized_width,
                    resized_height,
                    image::imageops::FilterType::Lanczos3,
                );
                let mut pixels = vec![0_u8; (self.width * self.height * 4) as usize];
                let x_offset = (self.width - resized_width) / 2;
                let y_offset = (self.height - resized_height) / 2;
                for y in 0..resized_height {
                    for x in 0..resized_width {
                        let source_index = ((y * resized_width + x) * 4) as usize;
                        let destination_index =
                            (((y + y_offset) * self.width + x + x_offset) * 4) as usize;
                        let alpha = resized.as_raw()[source_index + 3] as f32 / 255.0;
                        pixels[destination_index] =
                            (resized.as_raw()[source_index] as f32 * alpha) as u8;
                        pixels[destination_index + 1] =
                            (resized.as_raw()[source_index + 1] as f32 * alpha) as u8;
                        pixels[destination_index + 2] =
                            (resized.as_raw()[source_index + 2] as f32 * alpha) as u8;
                        pixels[destination_index + 3] = resized.as_raw()[source_index + 3];
                    }
                }
                pixels
            }
        };

        if self.flip_h {
            for y in 0..self.height {
                for x in 0..self.width / 2 {
                    let left = ((y * self.width + x) * 4) as usize;
                    let right = ((y * self.width + (self.width - 1 - x)) * 4) as usize;
                    for channel in 0..4 {
                        pixels.swap(left + channel, right + channel);
                    }
                }
            }
        }
        pixels
    }
}

impl IconAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let entries = vec![
            AtlasEntry::svg("pause", include_bytes!("../icons/pause.svg")),
            AtlasEntry::svg("resume", include_bytes!("../icons/resume.svg")),
            AtlasEntry::svg("select-mode", include_bytes!("../icons/select-mode.svg")),
            AtlasEntry::svg("draw-mode", include_bytes!("../icons/draw-mode.svg")),
            AtlasEntry::svg("eraser", include_bytes!("../icons/eraser.svg")),
            AtlasEntry::svg("prev_frame", include_bytes!("../icons/goto.svg")),
            AtlasEntry::flipped_svg("next_frame", include_bytes!("../icons/goto.svg")),
            AtlasEntry::svg("boucle", include_bytes!("../icons/boucle.svg")),
            AtlasEntry::svg("out", include_bytes!("../icons/out.svg")),
            AtlasEntry::svg("scene", include_bytes!("../icons/changement_scene.svg")),
            AtlasEntry::svg("respirations", include_bytes!("../icons/respirations.svg")),
            AtlasEntry::svg("reactions", include_bytes!("../icons/reactions.svg")),
            AtlasEntry::svg("liaison_left", include_bytes!("../icons/liason.svg")),
            AtlasEntry::flipped_svg("liaison_right", include_bytes!("../icons/liason.svg")),
            AtlasEntry::svg("settings", include_bytes!("../icons/settings.svg")),
            AtlasEntry::svg("project", include_bytes!("../icons/project.svg")),
            AtlasEntry::svg("stretcher", include_bytes!("../icons/stretcher.svg")),
            AtlasEntry::svg("br-edit", include_bytes!("../icons/br-edit.svg")),
            AtlasEntry::svg("note", include_bytes!("../icons/note.svg")),
            AtlasEntry::svg("karaoke", include_bytes!("../icons/karaoke.svg")),
            AtlasEntry::svg("sound", include_bytes!("../icons/sound.svg")),
            AtlasEntry::svg("mute", include_bytes!("../icons/mute-svgrepo-com (1).svg")),
            AtlasEntry::stretchable_svg(
                "detection/labial",
                include_bytes!("../icons/detection/labial.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/semi_labial",
                include_bytes!("../icons/detection/semi_labial.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/mouth_open",
                include_bytes!("../icons/detection/mouth_open.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/mouth_closed",
                include_bytes!("../icons/detection/mouth_closed.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/teeth_visible",
                include_bytes!("../icons/detection/teeth_visible.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/breath",
                include_bytes!("../icons/detection/breath.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/reaction",
                include_bytes!("../icons/detection/reaction.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/th",
                include_bytes!("../icons/detection/th.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/neutral",
                include_bytes!("../icons/detection/neutral.svg"),
            ),
            AtlasEntry::stretchable_svg(
                "detection/pucker",
                include_bytes!("../icons/cul_de_poule.svg"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/AA",
                include_bytes!("../icons/detection/rhubarb_lips/AA.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/AO_ER",
                include_bytes!("../icons/detection/rhubarb_lips/AO_ER.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/EH_AE",
                include_bytes!("../icons/detection/rhubarb_lips/EH_AE.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/F_V",
                include_bytes!("../icons/detection/rhubarb_lips/F_V.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/K_S_T_EE",
                include_bytes!("../icons/detection/rhubarb_lips/K_S_T_EE.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/L",
                include_bytes!("../icons/detection/rhubarb_lips/L.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/P_B_M",
                include_bytes!("../icons/detection/rhubarb_lips/P_B_M.png"),
            ),
            AtlasEntry::rhubarb_png(
                "detection/rhubarb_lips/UW_OW_W",
                include_bytes!("../icons/detection/rhubarb_lips/UW_OW_W.png"),
            ),
        ];

        let atlas_width = entries.iter().map(|entry| entry.width).sum::<u32>();
        let atlas_height = entries
            .iter()
            .map(|entry| entry.height)
            .max()
            .unwrap_or(ICON_SIZE);
        let mut atlas_data = vec![0_u8; (atlas_width * atlas_height * 4) as usize];
        let mut icon_positions = HashMap::new();
        let mut x_offset = 0_u32;

        for entry in &entries {
            let pixels = entry.rasterize();
            icon_positions.insert(
                entry.name.to_string(),
                (x_offset, 0, entry.width, entry.height),
            );
            for y in 0..entry.height {
                for x in 0..entry.width {
                    let source = ((y * entry.width + x) * 4) as usize;
                    let destination = ((y * atlas_width + x_offset + x) * 4) as usize;
                    atlas_data[destination..destination + 4]
                        .copy_from_slice(&pixels[source..source + 4]);
                }
            }
            x_offset += entry.width;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Icon Atlas"),
            size: wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * atlas_width),
                rows_per_image: Some(atlas_height),
            },
            wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Icon BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Icon BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            sampler,
            bind_group,
            bind_group_layout,
            icon_positions,
            atlas_width,
            atlas_height,
        }
    }

    pub fn get_uv(&self, name: &str) -> Option<[f32; 4]> {
        self.icon_positions.get(name).map(|&(x, y, width, height)| {
            let u_min = x as f32 / self.atlas_width as f32;
            let v_min = y as f32 / self.atlas_height as f32;
            let u_max = (x + width) as f32 / self.atlas_width as f32;
            let v_max = (y + height) as f32 / self.atlas_height as f32;
            [u_min, v_min, u_max, v_max]
        })
    }
}
