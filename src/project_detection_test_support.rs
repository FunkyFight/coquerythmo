//! Test-only ergonomic proxy for detection fixtures.
//!
//! Tests use the same canonical project mutation path as production so revision
//! invalidation and validation remain covered.

use crate::detection::{DetectionAddress, DetectionChange, DetectionCue};
use crate::project::Project;

pub struct ProjectDetectionsMut<'a> {
    project: &'a mut Project,
}

impl Project {
    pub fn detections_mut(&mut self) -> ProjectDetectionsMut<'_> {
        ProjectDetectionsMut { project: self }
    }
}

impl ProjectDetectionsMut<'_> {
    pub fn insert_detection(&mut self, address: DetectionAddress, cue: DetectionCue) -> bool {
        self.project.apply_detection_change(
            &DetectionChange::Add { address, cue },
            true,
        )
    }
}
