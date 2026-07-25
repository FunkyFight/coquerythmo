//! Composition of the graphics context and the generic UI renderer.
//!
//! Interactive frame cadence is intentionally not implemented here.
//! [`FrameTiming`] is the sole authority for monitor refresh detection,
//! redraw deadlines and shared visual frame samples.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::frame_timing::{FrameSample, FrameTiming};
use crate::graphics::GraphicsContext;
use crate::ui::renderer::UiRenderer;

pub struct RenderCoordinator {
    pub gfx: GraphicsContext,
    pub ui_renderer: UiRenderer,

    /// Single source of truth for interactive rendering cadence.
    pub frame_timing: FrameTiming,
}

impl RenderCoordinator {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window).await;
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, gfx.surface_format());
        let frame_timing = FrameTiming::new(&gfx.window);

        Self {
            gfx,
            ui_renderer,
            frame_timing,
        }
    }

    /// Re-reads the refresh rate of the monitor currently containing the main
    /// window.
    ///
    /// All refresh-rate calculations remain inside `frame_timing.rs`.
    pub fn update_refresh_interval(&mut self) {
        self.frame_timing.update_monitor(&self.gfx.window);
    }

    /// Compatibility façade for callers that need the physical display
    /// interval.
    ///
    /// This method contains no independent timing state or calculation.
    pub fn refresh_interval(&self) -> Duration {
        self.frame_timing.refresh_interval()
    }

    /// Compatibility façade for the beginning of the latest rendered frame.
    pub fn last_redraw(&self) -> Instant {
        self.frame_timing.last_frame_started_at()
    }

    /// Whether the next continuously animated display frame is due.
    pub fn is_frame_due(&self, now: Instant) -> bool {
        self.frame_timing.is_frame_due(now)
    }

    /// Deadline for the next continuously animated display frame.
    pub fn next_frame_deadline(&self) -> Instant {
        self.frame_timing.next_frame_deadline()
    }

    /// Creates the one time sample that must be shared by every visual
    /// component rendered during this frame.
    #[must_use]
    pub fn begin_frame(&mut self, now: Instant) -> FrameSample {
        self.frame_timing.begin_frame(now)
    }

    /// Records completion of a successful surface presentation.
    pub fn finish_present(&mut self, presented_at: Instant) {
        self.frame_timing.finish_present(presented_at);
    }
}
