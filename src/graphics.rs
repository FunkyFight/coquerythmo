use std::sync::Arc;

use winit::window::Window;

/// Coquerythmo always presents through strict FIFO VSync.
///
/// Frame pacing belongs to the swapchain itself. CPU-side code must not try to
/// predict VBlank with timers or fixed redraw intervals.
fn present_mode() -> wgpu::PresentMode {
    wgpu::PresentMode::Fifo
}

fn graphics_backends() -> wgpu::Backends {
    #[cfg(target_os = "windows")]
    {
        wgpu::Backends::DX12
    }

    #[cfg(not(target_os = "windows"))]
    {
        wgpu::Backends::PRIMARY
    }
}

pub struct GraphicsContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,
}

pub struct WindowSurface {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,
}

impl WindowSurface {
    pub fn resize(&mut self, device: &wgpu::Device, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(device, &self.config);
        }
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

impl GraphicsContext {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: graphics_backends(),
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create GPU surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No compatible GPU adapter found");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .expect("Failed to create GPU device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: present_mode(),
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],

            // Allow the CPU and GPU to overlap enough work to absorb a small
            // scheduling or rendering spike without immediately missing the
            // next FIFO presentation slot. FIFO still controls presentation.
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Self {
            instance,
            adapter,
            surface,
            device,
            queue,
            config,
            size,
            window,
        }
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn create_window_surface(&self, window: Arc<Window>) -> Result<WindowSurface, String> {
        let size = window.inner_size();

        let surface = self
            .instance
            .create_surface(window.clone())
            .map_err(|error| format!("Failed to create GPU surface: {error}"))?;

        let surface_caps = surface.get_capabilities(&self.adapter);

        if !surface_caps.formats.contains(&self.config.format) {
            return Err("Secondary display does not support the main surface format".into());
        }

        let alpha_mode = if surface_caps.alpha_modes.contains(&self.config.alpha_mode) {
            self.config.alpha_mode
        } else {
            surface_caps.alpha_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.config.format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: present_mode(),
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: self.config.desired_maximum_frame_latency,
        };

        surface.configure(&self.device, &config);

        Ok(WindowSurface {
            surface,
            config,
            size,
            window,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{graphics_backends, present_mode};

    #[test]
    fn presentation_always_uses_strict_vsync() {
        assert_eq!(present_mode(), wgpu::PresentMode::Fifo);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_uses_only_dx12() {
        assert_eq!(graphics_backends(), wgpu::Backends::DX12);
    }
}
