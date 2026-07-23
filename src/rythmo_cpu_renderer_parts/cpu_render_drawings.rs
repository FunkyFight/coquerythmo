{
        // Drawings are an overlay in the editor, so composite them last in the
        // exported BR as well (above lines, labels and markers).
        let drawing_origin_local_x = geometry.timeline_origin_x - geometry.viewport_left;
        let (first_frame, last_frame) = crate::rythmo_drawing::visible_frame_window_with_origin(
            width as f32,
            drawing_origin_local_x,
            current_frame,
            ppf,
            4,
        );
        let strokes: Vec<_> = scene
            .drawings
            .iter()
            .filter(|stroke| stroke.intersects_window(first_frame, last_frame))
            .collect();
        if !strokes.is_empty() {
            let drawing = crate::rythmo_drawing::rasterize_window_with_origin(
                &strokes,
                width,
                height,
                drawing_origin_local_x,
                current_frame,
                ppf,
            );
            crate::rythmo_drawing::composite_rgba_over(pixmap.data_mut(), &drawing);
        }

        pixmap.data().to_vec()
}
