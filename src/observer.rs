/// Observer pattern via event bus.
/// The timeline subject emits events, observers poll them each frame.
/// No closures/callbacks — avoids Rust borrow conflicts.

#[derive(Debug, Clone, Copy)]
pub enum TimelineEvent {
    FrameChanged { frame: i64 },
    PlaybackStarted,
    PlaybackStopped,
    VideoLoaded { fps: f64, total_frames: i64 },
}

pub struct TimelineBus {
    events: Vec<TimelineEvent>,
}

impl Default for TimelineBus {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineBus {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Emit an event. Observers will see it next drain().
    pub fn emit(&mut self, event: TimelineEvent) {
        self.events.push(event);
    }

    /// Drain all pending events. Call once per frame from the render/update loop.
    pub fn drain(&mut self) -> Vec<TimelineEvent> {
        std::mem::take(&mut self.events)
    }

    /// Check if there's a FrameChanged pending (without consuming).
    pub fn has_frame_change(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, TimelineEvent::FrameChanged { .. }))
    }
}
