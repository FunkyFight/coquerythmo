//! Rendering view for the file explorer modal.

use super::FileExplorerModal;
use crate::ui::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

impl FileExplorerModal {
    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let layout = Self::layout(screen_w, screen_h);

        quads.push(super::quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.78],
            [0.0, 0.0, 0.0, 0.78],
            [0.0; 4],
            0.0,
            0.0,
        ));
        quads.push(super::quad(
            layout.card,
            [0.22, 0.22, 0.26, 1.0],
            [0.15, 0.15, 0.18, 1.0],
            [0.45, 0.45, 0.52, 0.8],
            1.5,
            14.0,
        ));

        labels.push(LabelInfo {
            text: &self.title,
            bounds: Rect {
                x: layout.card.x,
                y: layout.card.y + 10.0,
                width: layout.card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 16.0,
            font_size_override: Some(17.0),
            color_override: None,
            font_family_override: None,
        });

        self.render_toolbar(quads, labels, &layout);
        self.render_sidebar(quads, labels, &layout);
        self.render_list(quads, labels, &layout);
        self.render_footer(quads, labels, &layout);
        let focus = self.focus_rect(&layout);
        quads.push(super::quad(
            Rect {
                x: focus.x - 2.0,
                y: focus.y - 2.0,
                width: focus.width + 4.0,
                height: focus.height + 4.0,
            },
            [0.0; 4],
            [0.0; 4],
            [0.25, 0.52, 1.0, 1.0],
            2.0,
            5.0,
        ));
        if self.overwrite_path.is_some() {
            self.render_overwrite_prompt(quads, labels, &layout);
            let (_, cancel, overwrite) = super::overwrite_rects(layout.card);
            let focused = if self.overwrite_focus_replace {
                overwrite
            } else {
                cancel
            };
            quads.push(super::quad(
                Rect {
                    x: focused.x - 2.0,
                    y: focused.y - 2.0,
                    width: focused.width + 4.0,
                    height: focused.height + 4.0,
                },
                [0.0; 4],
                [0.0; 4],
                [0.25, 0.52, 1.0, 1.0],
                2.0,
                5.0,
            ));
        } else {
            self.render_filename_suggestions(quads, labels, &layout);
            if self.show_filter_dropdown {
                self.render_filter_dropdown(quads, labels, &layout);
            }
        }
    }
}
