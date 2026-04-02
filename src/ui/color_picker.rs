use super::widget::{IconInstance, QuadInstance, Rect};

const SV_SIZE: f32 = 150.0;
const HUE_BAR_HEIGHT: f32 = 16.0;
const GAP: f32 = 6.0;
const PICKER_PADDING: f32 = 8.0;
const PICKER_RADIUS: f32 = 6.0;
const INDICATOR_SIZE: f32 = 8.0;
const SV_TEX_SIZE: u32 = 64;
const HUE_TEX_W: u32 = 256;
const HUE_TEX_H: u32 = 1;

pub struct ColorPickerState {
    pub active: bool,
    pub origin: (f32, f32), // top-left of the picker
    pub hue: f32,           // 0.0 - 360.0
    pub sat: f32,           // 0.0 - 1.0
    pub val: f32,           // 0.0 - 1.0
    pub dragging_sv: bool,
    pub dragging_hue: bool,
    sv_texture_dirty: bool,
    sv_bind_group: Option<wgpu::BindGroup>,
    hue_bind_group: Option<wgpu::BindGroup>,
}

impl ColorPickerState {
    pub fn new() -> Self {
        Self {
            active: false,
            origin: (0.0, 0.0),
            hue: 0.0,
            sat: 1.0,
            val: 1.0,
            dragging_sv: false,
            dragging_hue: false,
            sv_texture_dirty: true,
            sv_bind_group: None,
            hue_bind_group: None,
        }
    }

    pub fn open(&mut self, x: f32, y: f32, color: [f32; 4]) {
        self.active = true;
        self.origin = (x, y);
        let (h, s, v) = rgb_to_hsv(color[0], color[1], color[2]);
        self.hue = h;
        self.sat = s;
        self.val = v;
        self.sv_texture_dirty = true;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.dragging_sv = false;
        self.dragging_hue = false;
    }

    pub fn current_color(&self) -> [f32; 4] {
        let (r, g, b) = hsv_to_rgb(self.hue, self.sat, self.val);
        [r, g, b, 1.0]
    }

    fn total_rect(&self) -> Rect {
        let w = SV_SIZE + PICKER_PADDING * 2.0;
        let h = SV_SIZE + GAP + HUE_BAR_HEIGHT + PICKER_PADDING * 2.0;
        Rect { x: self.origin.0, y: self.origin.1, width: w, height: h }
    }

    fn sv_rect(&self) -> Rect {
        Rect {
            x: self.origin.0 + PICKER_PADDING,
            y: self.origin.1 + PICKER_PADDING,
            width: SV_SIZE,
            height: SV_SIZE,
        }
    }

    fn hue_rect(&self) -> Rect {
        Rect {
            x: self.origin.0 + PICKER_PADDING,
            y: self.origin.1 + PICKER_PADDING + SV_SIZE + GAP,
            width: SV_SIZE,
            height: HUE_BAR_HEIGHT,
        }
    }

    pub fn ensure_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        // Hue bar texture (created once)
        if self.hue_bind_group.is_none() {
            let mut pixels = vec![0u8; (HUE_TEX_W * HUE_TEX_H * 4) as usize];
            for x in 0..HUE_TEX_W {
                let h = x as f32 / HUE_TEX_W as f32 * 360.0;
                let (r, g, b) = hsv_to_rgb(h, 1.0, 1.0);
                let i = (x * 4) as usize;
                pixels[i] = (r * 255.0) as u8;
                pixels[i + 1] = (g * 255.0) as u8;
                pixels[i + 2] = (b * 255.0) as u8;
                pixels[i + 3] = 255;
            }
            self.hue_bind_group = Some(upload_texture(device, queue, bind_group_layout, sampler, &pixels, HUE_TEX_W, HUE_TEX_H));
        }

        // SV texture (regenerated when hue changes)
        if self.sv_texture_dirty || self.sv_bind_group.is_none() {
            let mut pixels = vec![0u8; (SV_TEX_SIZE * SV_TEX_SIZE * 4) as usize];
            for y in 0..SV_TEX_SIZE {
                for x in 0..SV_TEX_SIZE {
                    let s = x as f32 / (SV_TEX_SIZE - 1) as f32;
                    let v = 1.0 - y as f32 / (SV_TEX_SIZE - 1) as f32;
                    let (r, g, b) = hsv_to_rgb(self.hue, s, v);
                    let i = ((y * SV_TEX_SIZE + x) * 4) as usize;
                    pixels[i] = (r * 255.0) as u8;
                    pixels[i + 1] = (g * 255.0) as u8;
                    pixels[i + 2] = (b * 255.0) as u8;
                    pixels[i + 3] = 255;
                }
            }
            self.sv_bind_group = Some(upload_texture(device, queue, bind_group_layout, sampler, &pixels, SV_TEX_SIZE, SV_TEX_SIZE));
            self.sv_texture_dirty = false;
        }
    }

    pub fn render<'a>(&'a self, quads: &mut Vec<QuadInstance>, icons: &mut Vec<(IconInstance, &'a wgpu::BindGroup)>) {
        if !self.active {
            return;
        }

        let total = self.total_rect();
        let sv = self.sv_rect();
        let hue = self.hue_rect();

        // Background
        quads.push(QuadInstance {
            rect: [total.x, total.y, total.width, total.height],
            color: [0.12, 0.12, 0.14, 0.95],
            color_bottom: [0.10, 0.10, 0.12, 0.95],
            border_color: [0.3, 0.3, 0.36, 0.6],
            border_width: 1.0,
            border_radius: PICKER_RADIUS,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            _padding: [0.0; 3],
        });

        // SV gradient quad
        if let Some(bg) = &self.sv_bind_group {
            icons.push((
                IconInstance {
                    rect: [sv.x, sv.y, sv.width, sv.height],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                },
                bg,
            ));
        }

        // SV indicator
        let sv_ix = sv.x + self.sat * sv.width - INDICATOR_SIZE / 2.0;
        let sv_iy = sv.y + (1.0 - self.val) * sv.height - INDICATOR_SIZE / 2.0;
        quads.push(QuadInstance {
            rect: [sv_ix, sv_iy, INDICATOR_SIZE, INDICATOR_SIZE],
            color: [1.0, 1.0, 1.0, 0.9],
            color_bottom: [1.0, 1.0, 1.0, 0.9],
            border_color: [0.0, 0.0, 0.0, 0.8],
            border_width: 1.5,
            border_radius: INDICATOR_SIZE / 2.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });

        // Hue bar quad
        if let Some(bg) = &self.hue_bind_group {
            icons.push((
                IconInstance {
                    rect: [hue.x, hue.y, hue.width, hue.height],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                },
                bg,
            ));
        }

        // Hue indicator
        let hue_ix = hue.x + (self.hue / 360.0) * hue.width - 2.0;
        quads.push(QuadInstance {
            rect: [hue_ix, hue.y - 1.0, 4.0, hue.height + 2.0],
            color: [1.0, 1.0, 1.0, 0.9],
            color_bottom: [1.0, 1.0, 1.0, 0.9],
            border_color: [0.0, 0.0, 0.0, 0.8],
            border_width: 1.0,
            border_radius: 2.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });

        // Preview swatch
        let color = self.current_color();
        let preview_size = 20.0;
        let px = total.x + total.width - PICKER_PADDING - preview_size;
        let py = hue.y + hue.height + GAP;
        quads.push(QuadInstance {
            rect: [px, py - GAP, preview_size, preview_size],
            color, color_bottom: color,
            border_color: [0.5, 0.5, 0.55, 0.5],
            border_width: 1.0,
            border_radius: 3.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });
    }

    pub fn handle_event(&mut self, event: &super::widget::UiEvent) -> bool {
        if !self.active {
            return false;
        }

        let sv = self.sv_rect();
        let hue = self.hue_rect();

        match event {
            super::widget::UiEvent::MousePress { x, y } => {
                if sv.contains(*x, *y) {
                    self.dragging_sv = true;
                    self.sat = ((*x - sv.x) / sv.width).clamp(0.0, 1.0);
                    self.val = 1.0 - ((*y - sv.y) / sv.height).clamp(0.0, 1.0);
                    return true;
                }
                if hue.contains(*x, *y) {
                    self.dragging_hue = true;
                    self.hue = ((*x - hue.x) / hue.width).clamp(0.0, 1.0) * 360.0;
                    self.sv_texture_dirty = true;
                    return true;
                }
                // Click outside picker → close
                if !self.total_rect().contains(*x, *y) {
                    self.close();
                    return true;
                }
                false
            }
            super::widget::UiEvent::MouseMove { x, y } => {
                if self.dragging_sv {
                    self.sat = ((*x - sv.x) / sv.width).clamp(0.0, 1.0);
                    self.val = 1.0 - ((*y - sv.y) / sv.height).clamp(0.0, 1.0);
                    return true;
                }
                if self.dragging_hue {
                    self.hue = ((*x - hue.x) / hue.width).clamp(0.0, 1.0) * 360.0;
                    self.sv_texture_dirty = true;
                    return true;
                }
                false
            }
            super::widget::UiEvent::MouseRelease { .. } => {
                if self.dragging_sv || self.dragging_hue {
                    self.dragging_sv = false;
                    self.dragging_hue = false;
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    pixels: &[u8],
    w: u32, h: u32,
) -> wgpu::BindGroup {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ColorPicker Tex"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ColorPicker BG"), layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r1 + m, g1 + m, b1 + m)
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 0.0001 {
        0.0
    } else if (max - r).abs() < 0.0001 {
        60.0 * (((g - b) / d) % 6.0)
    } else if (max - g).abs() < 0.0001 {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max < 0.0001 { 0.0 } else { d / max };
    (h, s, max)
}
