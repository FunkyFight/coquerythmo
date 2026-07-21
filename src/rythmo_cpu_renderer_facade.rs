//! CPU export renderer facade with cached semantic project normalization.

#[path = "rythmo_cpu_renderer.rs"]
mod legacy;

use crate::project::Project;

pub struct CpuRenderer {
    inner: legacy::CpuRenderer,
    cached_revision: u64,
    cached_project: Option<Project>,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self {
            inner: legacy::CpuRenderer::new(),
            cached_revision: u64::MAX,
            cached_project: None,
        }
    }

    fn semantic_project<'a>(&'a mut self, project: &Project) -> &'a Project {
        let revision = project.revision();
        if self.cached_revision != revision || self.cached_project.is_none() {
            self.cached_project = Some(crate::rythmo_export_project::normalize_for_video(project));
            self.cached_revision = revision;
        }
        self.cached_project
            .as_ref()
            .expect("semantic CPU export project should be cached")
    }

    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: i64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        // Split the borrows explicitly: the normalized project lives in the
        // cache while the legacy renderer mutates only its own backend state.
        let revision = project.revision();
        if self.cached_revision != revision || self.cached_project.is_none() {
            self.cached_project = Some(crate::rythmo_export_project::normalize_for_video(project));
            self.cached_revision = revision;
        }
        let semantic = self
            .cached_project
            .as_ref()
            .expect("semantic CPU export project should be cached");
        self.inner.render_br(
            semantic,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_project_cache_is_revision_bound() {
        let mut renderer = CpuRenderer::new();
        let mut project = Project::new();
        let first_revision = project.revision();
        let _ = renderer.semantic_project(&project);
        assert_eq!(renderer.cached_revision, first_revision);
        project.add_line(0, 24, 0.0);
        let _ = renderer.semantic_project(&project);
        assert_eq!(renderer.cached_revision, project.revision());
    }
}
