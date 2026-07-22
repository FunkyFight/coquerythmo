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
var text_texture: texture_2d<f32>;
@group(1) @binding(1)
var text_sampler: sampler;

struct TextInstance {
    @location(0) rect: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) tint: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: TextInstance,
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

    var out: VertexOutput;
    out.position = vec4<f32>(
        (pos_px.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (pos_px.y / uniforms.screen_size.y) * 2.0,
        0.0,
        1.0,
    );
    out.uv = mix(instance.uv_rect.xy, instance.uv_rect.zw, corner);
    out.tint = instance.tint;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // resvg/tiny-skia outputs premultiplied RGBA. Preserve that invariant so
    // fractional horizontal sampling does not multiply glyph-edge coverage a
    // second time and make moving text pulse between sharp and blurry phases.
    let tex = textureSample(text_texture, text_sampler, in.uv);
    return vec4<f32>(
        tex.rgb * in.tint.rgb * in.tint.a,
        tex.a * in.tint.a,
    );
}
