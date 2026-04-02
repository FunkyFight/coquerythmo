use super::widget::Rect;

pub const TOPBAR_H: f32 = 32.0;
pub const TOOLBAR_H: f32 = 40.0;
const RYTHMO_RATIO: f32 = 0.35;
pub const PROPS_MIN_W: f32 = 200.0;
pub const PROPS_MAX_W: f32 = 500.0;
pub const PROPS_DEFAULT_W: f32 = 320.0;
pub const PROPS_DRAG_ZONE: f32 = 5.0;

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
    ) -> Self {
        let props_w = if props_visible {
            props_width.clamp(PROPS_MIN_W, PROPS_MAX_W)
        } else {
            0.0
        };
        let main_w = screen_w - props_w;
        let content_h = screen_h - TOPBAR_H;

        let rythmo_h = (content_h - TOOLBAR_H) * RYTHMO_RATIO;
        let video_h = content_h - TOOLBAR_H - rythmo_h;

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
}
