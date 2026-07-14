use std::io::ErrorKind;
use std::time::Duration;

use crate::rythmo_gpu_renderer;

use super::EXPORT_CANCELLED_MESSAGE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExportPipeline {
    Cuda,
    Cpu,
}

impl ExportPipeline {
    pub(super) fn uses_cuda(self) -> bool {
        matches!(self, Self::Cuda)
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Cuda => "ffmpeg CUDA scale/overlay",
            Self::Cpu => "ffmpeg CPU filters",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BrRenderBackend {
    GpuWgpuNv12,
    GpuWgpuRgbaCuda,
    Cpu,
}

impl BrRenderBackend {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::GpuWgpuNv12 => "GPU WGPU->NV12",
            Self::GpuWgpuRgbaCuda => "GPU WGPU->RGBA->CUDA",
            Self::Cpu => "CPU fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BrInputFormat {
    Nv12,
    Rgba,
}

impl BrInputFormat {
    pub(super) fn pix_fmt(self) -> &'static str {
        match self {
            Self::Nv12 => "nv12",
            Self::Rgba => "rgba",
        }
    }

    pub(super) fn frame_size(self, width: usize, height: usize) -> usize {
        match self {
            Self::Nv12 => width * height * 3 / 2,
            Self::Rgba => width * height * 4,
        }
    }
}

pub(super) struct BrFrameWriteStats {
    pub(super) backend: BrRenderBackend,
    pub(super) frames: u64,
    pub(super) total: Duration,
    pub(super) renderer_init: Duration,
    pub(super) submit: Duration,
    pub(super) finish_readback: Duration,
    pub(super) convert: Duration,
    pub(super) write: Duration,
    pub(super) cpu_render: Duration,
    pub(super) gpu_stats: Option<rythmo_gpu_renderer::GpuRenderStats>,
}

impl BrFrameWriteStats {
    pub(super) fn new() -> Self {
        Self {
            backend: BrRenderBackend::Cpu,
            frames: 0,
            total: Duration::ZERO,
            renderer_init: Duration::ZERO,
            submit: Duration::ZERO,
            finish_readback: Duration::ZERO,
            convert: Duration::ZERO,
            write: Duration::ZERO,
            cpu_render: Duration::ZERO,
            gpu_stats: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct StdinWriteError {
    pub(super) kind: ErrorKind,
    pub(super) message: String,
}

impl StdinWriteError {
    pub(super) fn new(context: &str, error: std::io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: format!("{context}: {error}"),
        }
    }

    pub(super) fn render_panic(context: &str) -> Self {
        Self {
            kind: ErrorKind::Other,
            message: context.to_string(),
        }
    }

    pub(super) fn cancelled() -> Self {
        Self {
            kind: ErrorKind::Interrupted,
            message: EXPORT_CANCELLED_MESSAGE.to_string(),
        }
    }

    pub(super) fn is_broken_pipe(&self) -> bool {
        self.kind == ErrorKind::BrokenPipe
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.kind == ErrorKind::Interrupted && self.message == EXPORT_CANCELLED_MESSAGE
    }
}
