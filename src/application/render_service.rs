//! Composition of the graphics context and the generic UI renderer.
//!
//! Interactive frame pacing is not implemented here.
//!
//! FIFO presentation is the only authority controlling display cadence.
//! [`FrameTiming`] provides shared frame timestamps and passive refresh-rate
//! metadata, but never schedules redraws.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::frame_timing::{FrameSample, FrameTiming};
use crate::graphics::GraphicsContext;
use crate::ui::renderer::UiRenderer;

pub struct RenderCoordinator {
    pub gfx: GraphicsContext,
    pub ui_renderer: UiRenderer,

    /// Shared frame timestamps, refresh metadata and passive diagnostics.
    ///
    /// This object does not decide when another frame should be rendered.
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
    /// The result is metadata only and is never used to predict VBlank.
    pub fn update_refresh_interval(&mut self) {
        self.frame_timing.update_monitor(&self.gfx.window);
    }

    /// Approximate duration of one physical display refresh.
    ///
    /// Intended for diagnostics and input throttling, not frame scheduling.
    pub fn refresh_interval(&self) -> Duration {
        self.frame_timing.refresh_interval()
    }

    /// Beginning of the latest sampled rendered frame.
    pub fn last_redraw(&self) -> Instant {
        self.frame_timing.last_frame_started_at()
    }

    /// Creates the unique time sample shared by every visual component in the
    /// current rendered frame.
    #[must_use]
    pub fn begin_frame(&mut self, now: Instant) -> FrameSample {
        self.frame_timing.begin_frame(now)
    }

    /// Records completion of a successful surface presentation.
    pub fn finish_present(&mut self, presented_at: Instant) {
        self.frame_timing.finish_present(presented_at);
    }
}
