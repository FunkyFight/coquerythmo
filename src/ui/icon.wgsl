struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) uv_min: vec2<f32>,
    @location(3) uv_max: vec2<f32>,
};

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var icon_texture: texture_2d<f32>;
@group(1) @binding(1)
var icon_sampler: sampler;

struct IconInstance {
    @location(0) rect: vec4<f32>,     // x, y, w, h in pixels
    @location(1) uv_rect: vec4<f32>,  // u_min, v_min, u_max, v_max
    @location(2) tint: vec4<f32>,     // color tint
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: IconInstance,
) -> VertexOutput {
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let indices = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = corners[indices[vertex_index]];

    let pos_px = instance.rect.xy + corner * instance.rect.zw;
    let ndc = vec2<f32>(
        (pos_px.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (pos_px.y / uniforms.screen_size.y) * 2.0,
    );
    let uv = mix(instance.uv_rect.xy, instance.uv_rect.zw, corner);

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    out.tint = instance.tint;
    out.uv_min = instance.uv_rect.xy;
    out.uv_max = instance.uv_rect.zw;
    return out;
}

fn bilinear_sample_clamped(
    uv: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
) -> vec4<f32> {
    let dimensions_u = textureDimensions(icon_texture, 0u);
    let dimensions = vec2<f32>(dimensions_u);
    let min_pixel = vec2<i32>(floor(uv_min * dimensions));
    let max_pixel = vec2<i32>(ceil(uv_max * dimensions)) - vec2<i32>(1, 1);
    let minimum = vec2<f32>(min_pixel);
    let maximum = vec2<f32>(max_pixel);
    let sample_position = clamp(uv * dimensions - vec2<f32>(0.5, 0.5), minimum, maximum);
    let base = vec2<i32>(floor(sample_position));
    let fraction = fract(sample_position);
    let next = min(base + vec2<i32>(1, 1), max_pixel);

    let c00 = textureLoad(icon_texture, base, 0);
    let c10 = textureLoad(icon_texture, vec2<i32>(next.x, base.y), 0);
    let c01 = textureLoad(icon_texture, vec2<i32>(base.x, next.y), 0);
    let c11 = textureLoad(icon_texture, next, 0);
    return mix(mix(c00, c10, fraction.x), mix(c01, c11, fraction.x), fraction.y);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // The export renderer intentionally used a nearest sampler, which made
    // moving glyph edges pop whenever their quad crossed a fractional pixel.
    // Sampling explicitly here gives UI and offscreen GPU exports the same
    // sub-pixel reconstruction while clamping atlas entries against bleeding.
    let tex = bilinear_sample_clamped(in.uv, in.uv_min, in.uv_max);
    return tex * in.tint;
}
