{
        // ── Markers ──
        let marker_margin_frames = (10.0 * s / ppf).ceil() as i64 + 1;
        let marker_window = geometry.visible_frame_window(current_frame, marker_margin_frames);
        let first_marker_frame = marker_window.first;
        let last_marker_frame = marker_window.last;
        for marker in &common_scene.markers {
            if marker.frame < first_marker_frame || marker.frame > last_marker_frame {
                continue;
            }
            let mx = geometry.frame_x(marker.frame as f64, current_frame);
            if mx < -10.0 * s || mx > w + 10.0 * s {
                continue;
            }
            match &marker.kind {
                MarkerKind::Boucle => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        1.0,
                        0.02,
                        0.05,
                        230.0 / 255.0,
                    ));
                    let cy = h / 2.0;
                    let arm = 10.0 * s;
                    let diag_len = arm * 2.0 * std::f32::consts::SQRT_2;
                    quads.push(rotated_line(
                        mx,
                        cy,
                        diag_len,
                        2.5 * s,
                        std::f32::consts::FRAC_PI_4,
                        1.0,
                        0.02,
                        0.05,
                        230.0 / 255.0,
                    ));
                    quads.push(rotated_line(
                        mx,
                        cy,
                        diag_len,
                        2.5 * s,
                        -std::f32::consts::FRAC_PI_4,
                        217.0 / 255.0,
                        38.0 / 255.0,
                        38.0 / 255.0,
                        230.0 / 255.0,
                    ));
                }
                MarkerKind::Out => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        217.0 / 255.0,
                        115.0 / 255.0,
                        115.0 / 255.0,
                        180.0 / 255.0,
                    ));
                    let cy = h / 2.0;
                    let bh = h * 0.15;
                    for &offset in &[-5.0_f32, 5.0] {
                        let dx = bh * 0.3;
                        let length = (dx * 2.0_f32).hypot(bh * 2.0);
                        let angle = (bh * 2.0).atan2(dx * 2.0);
                        quads.push(rotated_line(
                            mx + offset * s,
                            cy,
                            length,
                            2.0 * s,
                            angle,
                            217.0 / 255.0,
                            115.0 / 255.0,
                            115.0 / 255.0,
                            180.0 / 255.0,
                        ));
                    }
                }
                MarkerKind::SceneChange => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        230.0 / 255.0,
                        230.0 / 255.0,
                        240.0 / 255.0,
                        200.0 / 255.0,
                    ));
                }
                MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => {
                    let is_left = matches!(marker.kind, MarkerKind::LiaisonLeft);
                    let ay = ruler_h / 2.0;
                    let arm_x = if is_left { -3.0 } else { 3.0 } * s;
                    let arm_y = 4.0 * s;
                    let tip_x = mx + arm_x;
                    for &dy in &[-arm_y, arm_y] {
                        let sx = mx - arm_x;
                        let length = ((tip_x - sx).powi(2) + dy.powi(2)).sqrt();
                        let angle = dy.atan2(tip_x - sx);
                        quads.push(rotated_line(
                            (sx + tip_x) / 2.0,
                            ay + dy / 2.0,
                            length,
                            1.5 * s,
                            angle,
                            180.0 / 255.0,
                            180.0 / 255.0,
                            190.0 / 255.0,
                            200.0 / 255.0,
                        ));
                    }
                }
            }
        }
}
