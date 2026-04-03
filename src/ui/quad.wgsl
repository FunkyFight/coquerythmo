struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) color_top: vec4<f32>,
    @location(2) color_bottom: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) border_radius: f32,
    @location(6) rect_size: vec2<f32>,
    @location(7) shadow_color: vec4<f32>,
    @location(8) shadow_offset: vec2<f32>,
    @location(9) shadow_blur: f32,
};

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct QuadInstance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) color_bottom: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_width_radius: vec2<f32>,
    @location(5) shadow_offset: vec2<f32>,
    @location(6) shadow_color: vec4<f32>,
    @location(7) shadow_blur_rotation: vec2<f32>,
};

fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn rotate2d(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: QuadInstance,
) -> VertexOutput {
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let indices = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = corners[indices[vertex_index]];

    let shadow_blur = instance.shadow_blur_rotation.x;
    let rotation = instance.shadow_blur_rotation.y;

    let expand = max(shadow_blur * 2.0, 0.0) + abs(max(instance.shadow_offset.x, instance.shadow_offset.y));
    let expanded_size = instance.rect.zw + vec2<f32>(expand * 2.0);

    // Local position relative to center, before rotation
    let local_unrotated = (corner - 0.5) * expanded_size;

    // Apply rotation around center
    let local_rotated = rotate2d(local_unrotated, rotation);

    // World position = center + rotated offset
    let center = instance.rect.xy + instance.rect.zw * 0.5;
    let pos_px = center + local_rotated;

    let ndc = vec2<f32>(
        (pos_px.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (pos_px.y / uniforms.screen_size.y) * 2.0,
    );

    // Pass un-rotated local for SDF (SDF operates in local space)
    let local_for_sdf = local_unrotated;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.local_pos = local_for_sdf;
    out.color_top = instance.color;
    out.color_bottom = instance.color_bottom;
    out.border_color = instance.border_color;
    out.border_width = instance.border_width_radius.x;
    out.border_radius = instance.border_width_radius.y;
    out.rect_size = instance.rect.zw;
    out.shadow_color = instance.shadow_color;
    out.shadow_offset = instance.shadow_offset;
    out.shadow_blur = shadow_blur;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let radius = min(in.border_radius, min(half_size.x, half_size.y));

    var shadow = vec4<f32>(0.0);
    if in.shadow_color.a > 0.0 {
        let shadow_pos = in.local_pos - in.shadow_offset;
        let shadow_dist = sdf_rounded_rect(shadow_pos, half_size, radius);
        let shadow_alpha = 1.0 - smoothstep(-in.shadow_blur, in.shadow_blur, shadow_dist);
        shadow = vec4<f32>(in.shadow_color.rgb, in.shadow_color.a * shadow_alpha);
    }

    let dist = sdf_rounded_rect(in.local_pos, half_size, radius);

    if dist > 0.5 {
        return shadow;
    }

    let t = (in.local_pos.y + half_size.y) / in.rect_size.y;
    let bg = mix(in.color_top, in.color_bottom, vec4<f32>(t));

    let inner_dist = sdf_rounded_rect(in.local_pos, half_size - vec2<f32>(in.border_width), max(radius - in.border_width, 0.0));
    let border_mask = smoothstep(-0.5, 0.5, inner_dist);
    let color = mix(bg, in.border_color, vec4<f32>(border_mask));

    let aa = 1.0 - smoothstep(-0.5, 0.5, dist);

    let shape = vec4<f32>(color.rgb, color.a * aa);
    return vec4<f32>(
        mix(shadow.rgb, shape.rgb, shape.a),
        shadow.a * (1.0 - shape.a) + shape.a,
    );
}
