struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
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
    @location(3) transform: vec4<f32>, // rotation, skew x, pivot x/y
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

    let pivot = instance.transform.zw * instance.rect.zw;
    var local = corner * instance.rect.zw - pivot;
    local.x += instance.transform.y * local.y;
    let cs = cos(instance.transform.x);
    let sn = sin(instance.transform.x);
    local = vec2<f32>(local.x * cs - local.y * sn, local.x * sn + local.y * cs);
    let pos_px = instance.rect.xy + pivot + local;
    let ndc = vec2<f32>(
        (pos_px.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (pos_px.y / uniforms.screen_size.y) * 2.0,
    );

    let uv = mix(instance.uv_rect.xy, instance.uv_rect.zw, corner);

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    out.tint = instance.tint;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(icon_texture, icon_sampler, in.uv);
    return tex * in.tint;
}
