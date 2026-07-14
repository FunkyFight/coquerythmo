use super::primitives::Rect;

pub const TOPBAR_H: f32 = 32.0;
pub const TOOLBAR_H: f32 = 76.0;
pub const PROPS_MIN_W: f32 = 200.0;
pub const PROPS_MAX_W: f32 = 500.0;
pub const PROPS_DEFAULT_W: f32 = 320.0;
pub const PROPS_DRAG_ZONE: f32 = 5.0;

/// Height of the draggable band at each resizable boundary.
pub const SPLIT_DRAG_ZONE: f32 = 6.0;
/// Minimum height reserved for the video preview when resizing.
pub const VIDEO_MIN_H: f32 = 80.0;
/// Minimum height reserved for the bande rythmo when resizing.
pub const RYTHMO_MIN_H: f32 = 120.0;

pub struct Layout {
    pub topbar: Rect,
    pub video_preview: Rect,
    pub toolbar: Rect,
    pub rythmo: Rect,
    pub properties: Option<Rect>,
}

impl Layout {
    pub fn compute(
        screen_w: f32,
        screen_h: f32,
        props_visible: bool,
        props_width: f32,
        video_split: f32,
    ) -> Self {
        let props_w = if props_visible {
            props_width.clamp(PROPS_MIN_W, PROPS_MAX_W)
        } else {
            0.0
        };
        let main_w = screen_w - props_w;
        let content_h = screen_h - TOPBAR_H;

        let free_h = (content_h - TOOLBAR_H).max(0.0);
        let min_split = (VIDEO_MIN_H / free_h.max(1.0)).clamp(0.0, 1.0);
        let max_split = (1.0 - RYTHMO_MIN_H / free_h.max(1.0))
            .clamp(0.0, 1.0)
            .max(min_split);
        let split = video_split.clamp(min_split, max_split);

        let video_h = free_h * split;
        let rythmo_h = free_h - video_h;

        let topbar = Rect {
            x: 0.0,
            y: 0.0,
            width: screen_w,
            height: TOPBAR_H,
        };

        let video_preview = Rect {
            x: 0.0,
            y: TOPBAR_H,
            width: main_w,
            height: video_h,
        };

        let toolbar = Rect {
            x: 0.0,
            y: TOPBAR_H + video_h,
            width: main_w,
            height: TOOLBAR_H,
        };

        let rythmo = Rect {
            x: 0.0,
            y: TOPBAR_H + video_h + TOOLBAR_H,
            width: main_w,
            height: rythmo_h,
        };

        let properties = if props_visible {
            Some(Rect {
                x: main_w,
                y: TOPBAR_H,
                width: props_w,
                height: content_h,
            })
        } else {
            None
        };

        Self {
            topbar,
            video_preview,
            toolbar,
            rythmo,
            properties,
        }
    }

    /// Draggable band at the boundary between the video preview and the toolbar.
    pub fn video_split_handle_rect(&self) -> Rect {
        Rect {
            x: self.video_preview.x,
            y: self.toolbar.y - SPLIT_DRAG_ZONE / 2.0,
            width: self.video_preview.width,
            height: SPLIT_DRAG_ZONE,
        }
    }

    /// Draggable band at the boundary between the toolbar and the bande rythmo.
    pub fn rythmo_split_handle_rect(&self) -> Rect {
        Rect {
            x: self.rythmo.x,
            y: self.rythmo.y - SPLIT_DRAG_ZONE / 2.0,
            width: self.rythmo.width,
            height: SPLIT_DRAG_ZONE,
        }
    }
}
