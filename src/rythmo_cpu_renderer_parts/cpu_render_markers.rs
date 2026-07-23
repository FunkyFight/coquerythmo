{
        // -- Markers --
        for marker in &scene.markers {
            let x = geometry.frame_x(marker.frame as f64, current_frame);
            if x < -12.0 * s || x > geometry.viewport_right() + 12.0 * s {
                continue;
            }
            match &marker.kind {
                crate::rythmo_line::MarkerKind::Boucle => {
                    blit_rect(&mut pixmap, x - s, 0.0, 2.0 * s, h, [255, 5, 13, 230]);
                    let cy = h * 0.5;
                    let arm = 10.0 * s;
                    blit_thick_line(
                        &mut pixmap,
                        x - arm,
                        cy - arm,
                        x + arm,
                        cy + arm,
                        2.5 * s,
                        [255, 5, 13, 230],
                    );
                    blit_thick_line(
                        &mut pixmap,
                        x - arm,
                        cy + arm,
                        x + arm,
                        cy - arm,
                        2.5 * s,
                        [217, 38, 38, 230],
                    );
                }
                crate::rythmo_line::MarkerKind::Out => {
                    blit_rect(
                        &mut pixmap,
                        x - s,
                        0.0,
                        2.0 * s,
                        h,
                        [217, 115, 115, 180],
                    );
                }
                crate::rythmo_line::MarkerKind::SceneChange => {
                    blit_rect(
                        &mut pixmap,
                        x - s,
                        0.0,
                        2.0 * s,
                        h,
                        [230, 230, 240, 200],
                    );
                }
                crate::rythmo_line::MarkerKind::LiaisonLeft
                | crate::rythmo_line::MarkerKind::LiaisonRight => {
                    let left = matches!(&marker.kind, crate::rythmo_line::MarkerKind::LiaisonLeft);
                    let direction = if left { -1.0 } else { 1.0 };
                    let y = ruler_h * 0.5;
                    let tip_x = x + direction * 3.0 * s;
                    let base_x = x - direction * 3.0 * s;
                    for dy in [-4.0 * s, 4.0 * s] {
                        blit_thick_line(
                            &mut pixmap,
                            base_x,
                            y - dy,
                            tip_x,
                            y,
                            1.5 * s,
                            [180, 180, 190, 200],
                        );
                    }
                }
            }
        }
}
