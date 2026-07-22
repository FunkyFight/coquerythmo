//! Composition of the graphics context and the generic UI renderer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::graphics::GraphicsContext;
use crate::ui::renderer::UiRenderer;

pub struct RenderCoordinator {
    pub gfx: GraphicsContext,
    pub ui_renderer: UiRenderer,
    pub last_redraw: Instant,
    pub refresh_interval: Duration,
}

fn refresh_interval_from_millihertz(refresh_rate_millihertz: Option<u32>) -> Duration {
    let millihertz = refresh_rate_millihertz
        .unwrap_or(60_000)
        .clamp(30_000, 360_000) as f64;
    Duration::from_secs_f64(1_000.0 / millihertz)
}

fn window_refresh_interval(window: &Window) -> Duration {
    refresh_interval_from_millihertz(
        window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz()),
    )
}

impl RenderCoordinator {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window.clone()).await;
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, gfx.surface_format());
        Self {
            gfx,
            ui_renderer,
            last_redraw: Instant::now(),
            refresh_interval: window_refresh_interval(&window),
        }
    }

    pub fn update_refresh_interval(&mut self) {
        self.refresh_interval = window_refresh_interval(&self.gfx.window);
    }
}

#[cfg(test)]
mod tests {
    use super::refresh_interval_from_millihertz;

    #[test]
    fn refresh_interval_tracks_the_real_monitor_rate() {
        assert_eq!(
            refresh_interval_from_millihertz(Some(60_000)).as_nanos(),
            16_666_667
        );
        assert_eq!(
            refresh_interval_from_millihertz(Some(144_000)).as_nanos(),
            6_944_444
        );
        assert_eq!(
            refresh_interval_from_millihertz(None).as_nanos(),
            16_666_667
        );
    }
}
