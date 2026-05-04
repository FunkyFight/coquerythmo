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

impl IconAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let svgs = vec![
            SvgEntry { name: "pause", data: include_bytes!("../icons/pause.svg"), flip_h: false },
            SvgEntry { name: "resume", data: include_bytes!("../icons/resume.svg"), flip_h: false },
            SvgEntry { name: "prev_frame", data: include_bytes!("../icons/goto.svg"), flip_h: false },
            SvgEntry { name: "next_frame", data: include_bytes!("../icons/goto.svg"), flip_h: true },
            SvgEntry { name: "boucle", data: include_bytes!("../icons/boucle.svg"), flip_h: false },
            SvgEntry { name: "out", data: include_bytes!("../icons/out.svg"), flip_h: false },
            SvgEntry { name: "scene", data: include_bytes!("../icons/changement_scene.svg"), flip_h: false },
            SvgEntry { name: "respirations", data: include_bytes!("../icons/respirations.svg"), flip_h: false },
            SvgEntry { name: "reactions", data: include_bytes!("../icons/reactions.svg"), flip_h: false },
            SvgEntry { name: "liaison_left", data: include_bytes!("../icons/liason.svg"), flip_h: false },
            SvgEntry { name: "liaison_right", data: include_bytes!("../icons/liason.svg"), flip_h: true },
            SvgEntry { name: "settings", data: include_bytes!("../icons/settings.svg"), flip_h: false },
            SvgEntry { name: "stretcher", data: include_bytes!("../icons/stretcher.svg"), flip_h: false },
            SvgEntry { name: "br-edit", data: include_bytes!("../icons/br-edit.svg"), flip_h: false },
            SvgEntry { name: "note", data: include_bytes!("../icons/note.svg"), flip_h: false },
            SvgEntry { name: "sound", data: include_bytes!("../icons/sound.svg"), flip_h: false },
            SvgEntry { name: "mute", data: include_bytes!("../icons/mute-svgrepo-com (1).svg"), flip_h: false },
        ];

        let count = svgs.len() as u32;
        let atlas_width = count * ICON_SIZE;
        let atlas_height = ICON_SIZE;

        let mut atlas_data = vec![0u8; (atlas_width * atlas_height * 4) as usize];
        let mut icon_positions = HashMap::new();

        for (i, entry) in svgs.iter().enumerate() {
            let tree = resvg::usvg::Tree::from_data(entry.data, &resvg::usvg::Options::default())
                .expect("Failed to parse SVG");

            let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();

            let svg_size = tree.size();
            let sx = ICON_SIZE as f32 / svg_size.width();
            let sy = ICON_SIZE as f32 / svg_size.height();

            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(sx, sy),
                &mut pixmap.as_mut(),
            );

            // Recolor to white
            let pixels = pixmap.data_mut();
            for chunk in pixels.chunks_exact_mut(4) {
                let a = chunk[3] as f32 / 255.0;
                chunk[0] = (255.0 * a) as u8;
                chunk[1] = (255.0 * a) as u8;
                chunk[2] = (255.0 * a) as u8;
            }

            // Flip horizontal if needed
            if entry.flip_h {
                for y in 0..ICON_SIZE {
                    for x in 0..ICON_SIZE / 2 {
                        let left = ((y * ICON_SIZE + x) * 4) as usize;
                        let right = ((y * ICON_SIZE + (ICON_SIZE - 1 - x)) * 4) as usize;
                        for c in 0..4 {
                            pixels.swap(left + c, right + c);
                        }
                    }
                }
            }

            let x_offset = i as u32 * ICON_SIZE;
            icon_positions.insert(entry.name.to_string(), (x_offset, 0));

            for y in 0..ICON_SIZE {
                for x in 0..ICON_SIZE {
                    let src = ((y * ICON_SIZE + x) * 4) as usize;
                    let dst = ((y * atlas_width + x_offset + x) * 4) as usize;
                    atlas_data[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
                }
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Icon Atlas"),
            size: wgpu::Extent3d { width: atlas_width, height: atlas_height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &atlas_data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * atlas_width), rows_per_image: Some(atlas_height) },
            wgpu::Extent3d { width: atlas_width, height: atlas_height, depth_or_array_layers: 1 },
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
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Icon BG"), layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Self { texture, view, sampler, bind_group, bind_group_layout, icon_positions, atlas_width, atlas_height }
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
