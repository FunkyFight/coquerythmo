use std::collections::HashMap;

const ICON_SIZE: u32 = 32;

pub struct IconAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    icon_positions: HashMap<String, (u32, u32)>,
    atlas_width: u32,
    atlas_height: u32,
}

struct SvgEntry {
    name: &'static str,
    data: &'static [u8],
    flip_h: bool,
}

impl SvgEntry {
    const fn new(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            flip_h: false,
        }
    }

    const fn flipped(name: &'static str, data: &'static [u8]) -> Self {
        Self {
            name,
            data,
            flip_h: true,
        }
    }
}

impl IconAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let svgs = vec![
            SvgEntry::new("pause", include_bytes!("../icons/pause.svg")),
            SvgEntry::new("resume", include_bytes!("../icons/resume.svg")),
            SvgEntry::new("select-mode", include_bytes!("../icons/select-mode.svg")),
            SvgEntry::new("draw-mode", include_bytes!("../icons/draw-mode.svg")),
            SvgEntry::new("eraser", include_bytes!("../icons/eraser.svg")),
            SvgEntry::new("prev_frame", include_bytes!("../icons/goto.svg")),
            SvgEntry::flipped("next_frame", include_bytes!("../icons/goto.svg")),
            SvgEntry::new("boucle", include_bytes!("../icons/boucle.svg")),
            SvgEntry::new("out", include_bytes!("../icons/out.svg")),
            SvgEntry::new("scene", include_bytes!("../icons/changement_scene.svg")),
            SvgEntry::new("respirations", include_bytes!("../icons/respirations.svg")),
            SvgEntry::new("reactions", include_bytes!("../icons/reactions.svg")),
            SvgEntry::new("liaison_left", include_bytes!("../icons/liason.svg")),
            SvgEntry::flipped("liaison_right", include_bytes!("../icons/liason.svg")),
            SvgEntry::new("settings", include_bytes!("../icons/settings.svg")),
            SvgEntry::new("project", include_bytes!("../icons/project.svg")),
            SvgEntry::new("stretcher", include_bytes!("../icons/stretcher.svg")),
            SvgEntry::new("br-edit", include_bytes!("../icons/br-edit.svg")),
            SvgEntry::new("note", include_bytes!("../icons/note.svg")),
            SvgEntry::new("karaoke", include_bytes!("../icons/karaoke.svg")),
            SvgEntry::new("sound", include_bytes!("../icons/sound.svg")),
            SvgEntry::new("mute", include_bytes!("../icons/mute-svgrepo-com (1).svg")),
            SvgEntry::new(
                "detection/labial",
                include_bytes!("../icons/detection/labial.svg"),
            ),
            SvgEntry::new(
                "detection/semi_labial",
                include_bytes!("../icons/detection/semi_labial.svg"),
            ),
            SvgEntry::new(
                "detection/mouth_open",
                include_bytes!("../icons/detection/mouth_open.svg"),
            ),
            SvgEntry::new(
                "detection/mouth_closed",
                include_bytes!("../icons/detection/mouth_closed.svg"),
            ),
            SvgEntry::new(
                "detection/teeth_visible",
                include_bytes!("../icons/detection/teeth_visible.svg"),
            ),
            SvgEntry::new(
                "detection/breath",
                include_bytes!("../icons/detection/breath.svg"),
            ),
            SvgEntry::new(
                "detection/reaction",
                include_bytes!("../icons/detection/reaction.svg"),
            ),
        ];

        let count = svgs.len() as u32;
        let atlas_width = count * ICON_SIZE;
        let atlas_height = ICON_SIZE;
        let mut atlas_data = vec![0u8; (atlas_width * atlas_height * 4) as usize];
        let mut icon_positions = HashMap::new();

        for (index, entry) in svgs.iter().enumerate() {
            let tree = resvg::usvg::Tree::from_data(entry.data, &resvg::usvg::Options::default())
                .expect("Failed to parse SVG");
            let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
            let svg_size = tree.size();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(
                    ICON_SIZE as f32 / svg_size.width(),
                    ICON_SIZE as f32 / svg_size.height(),
                ),
                &mut pixmap.as_mut(),
            );

            let pixels = pixmap.data_mut();
            for chunk in pixels.chunks_exact_mut(4) {
                let alpha = chunk[3] as f32 / 255.0;
                chunk[0] = (255.0 * alpha) as u8;
                chunk[1] = (255.0 * alpha) as u8;
                chunk[2] = (255.0 * alpha) as u8;
            }
            if entry.flip_h {
                for y in 0..ICON_SIZE {
                    for x in 0..ICON_SIZE / 2 {
                        let left = ((y * ICON_SIZE + x) * 4) as usize;
                        let right = ((y * ICON_SIZE + (ICON_SIZE - 1 - x)) * 4) as usize;
                        for channel in 0..4 {
                            pixels.swap(left + channel, right + channel);
                        }
                    }
                }
            }

            let x_offset = index as u32 * ICON_SIZE;
            icon_positions.insert(entry.name.to_string(), (x_offset, 0));
            for y in 0..ICON_SIZE {
                for x in 0..ICON_SIZE {
                    let source = ((y * ICON_SIZE + x) * 4) as usize;
                    let destination = ((y * atlas_width + x_offset + x) * 4) as usize;
                    atlas_data[destination..destination + 4]
                        .copy_from_slice(&pixels[source..source + 4]);
                }
            }
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
        self.icon_positions.get(name).map(|&(x, y)| {
            let u_min = x as f32 / self.atlas_width as f32;
            let v_min = y as f32 / self.atlas_height as f32;
            let u_max = (x + ICON_SIZE) as f32 / self.atlas_width as f32;
            let v_max = (y + ICON_SIZE) as f32 / self.atlas_height as f32;
            [u_min, v_min, u_max, v_max]
        })
    }
}