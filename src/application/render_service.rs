//! Composition of the graphics context and the generic UI renderer.

use std::sync::Arc;
use std::time::Instant;

use winit::window::Window;

use crate::graphics::GraphicsContext;
use crate::ui::renderer::UiRenderer;

pub struct RenderCoordinator {
    pub gfx: GraphicsContext,
    pub ui_renderer: UiRenderer,
    pub last_redraw: Instant,
}

impl RenderCoordinator {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window.clone()).await;
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, gfx.surface_format());
        Self {
            gfx,
            ui_renderer,
            last_redraw: Instant::now(),
        }
    }
}
