struct Nv12Params {
    width: u32,
    height: u32,
    padded_height: u32,
    total_bytes: u32,
};

@group(0) @binding(0)
var source_tex: texture_2d<f32>;

@group(0) @binding(1)
var<storage, read_write> nv12_words: array<u32>;

@group(0) @binding(2)
var<uniform> params: Nv12Params;

fn clamp_byte(value: f32, lo: f32, hi: f32) -> u32 {
    return u32(clamp(value, lo, hi));
}

fn rgb_at(x: u32, y: u32) -> vec3<f32> {
    if (x >= params.width || y >= params.height) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    let rgba = textureLoad(source_tex, vec2<i32>(i32(x), i32(y)), 0);
    return round(clamp(rgba.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0);
}

fn y_from_rgb(rgb: vec3<f32>) -> u32 {
    let y = ((66.0 * rgb.r + 129.0 * rgb.g + 25.0 * rgb.b + 128.0) / 256.0) + 16.0;
    return clamp_byte(y, 16.0, 235.0);
}

fn u_from_rgb(rgb: vec3<f32>) -> u32 {
    let u = ((-38.0 * rgb.r - 74.0 * rgb.g + 112.0 * rgb.b + 128.0) / 256.0) + 128.0;
    return clamp_byte(u, 16.0, 240.0);
}

fn v_from_rgb(rgb: vec3<f32>) -> u32 {
    let v = ((112.0 * rgb.r - 94.0 * rgb.g - 18.0 * rgb.b + 128.0) / 256.0) + 128.0;
    return clamp_byte(v, 16.0, 240.0);
}

fn nv12_byte_at(index: u32) -> u32 {
    if (index >= params.total_bytes) {
        return 0u;
    }

    let y_plane_size = params.width * params.padded_height;
    if (index < y_plane_size) {
        let y = index / params.width;
        if (y >= params.height) {
            return 16u;
        }
        let x = index - y * params.width;
        return y_from_rgb(rgb_at(x, y));
    }

    let uv_index = index - y_plane_size;
    let chroma_y = uv_index / params.width;
    let chroma_byte_x = uv_index - chroma_y * params.width;
    let chroma_x = chroma_byte_x / 2u;
    let px = chroma_x * 2u;
    let py = chroma_y * 2u;
    let rgb = (rgb_at(px, py)
        + rgb_at(px + 1u, py)
        + rgb_at(px, py + 1u)
        + rgb_at(px + 1u, py + 1u))
        * 0.25;

    if ((chroma_byte_x & 1u) == 0u) {
        return u_from_rgb(rgb);
    }
    return v_from_rgb(rgb);
}

fn pack4(a: u32, b: u32, c: u32, d: u32) -> u32 {
    return (a & 0xffu) | ((b & 0xffu) << 8u) | ((c & 0xffu) << 16u) | ((d & 0xffu) << 24u);
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let base = id.x * 4u;
    if (base >= params.total_bytes) {
        return;
    }
    nv12_words[id.x] = pack4(
        nv12_byte_at(base),
        nv12_byte_at(base + 1u),
        nv12_byte_at(base + 2u),
        nv12_byte_at(base + 3u),
    );
}
