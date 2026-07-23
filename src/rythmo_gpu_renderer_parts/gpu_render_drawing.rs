{
        // Match the editor's layer order: drawings cover the rendered BR and
        // are themselves free of editing handles or selection UI.
        let drawing_origin_local_x = geometry.timeline_origin_x - geometry.viewport_left;
        let drawing_icon_index = if self.prepare_drawing_overlay(
            &common_scene,
            current_frame,
            width,
            height,
            drawing_origin_local_x,
            ppf,
        ) {
                let index = all_icons.len() as u32;
                all_icons.push(IconInstance {
                    rect: [0.0, 0.0, width as f32, height as f32],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
                Some(index)
            } else {
                None
            };

        Self::coalesce_icon_batches(&mut icon_batches);
        drawing_icon_index
}
