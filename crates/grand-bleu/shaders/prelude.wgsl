struct Params { v: array<vec4<f32>, 32> };
@group(0) @binding(0) var<uniform> params: Params;
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var s0: sampler;
@group(1) @binding(2) var t1: texture_2d<f32>;
@group(1) @binding(3) var s1: sampler;
@group(1) @binding(4) var t2: texture_2d<f32>;
@group(1) @binding(5) var s2: sampler;
@group(1) @binding(6) var t3: texture_2d<f32>;
@group(1) @binding(7) var s3: sampler;
@group(1) @binding(8) var cube: texture_cube<f32>;
@group(1) @binding(9) var cube_s: sampler;

struct In {
    @location(0) position: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv0: vec4<f32>,
    @location(3) uv1: vec4<f32>,
    @location(4) uv2: vec4<f32>,
    @location(5) uv3: vec4<f32>,
};
struct V {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv0: vec4<f32>,
    @location(2) uv1: vec4<f32>,
    @location(3) uv2: vec4<f32>,
    @location(4) uv3: vec4<f32>,
};

@vertex
fn vs(i: In) -> V {
    var o: V;
    o.clip = i.position;
    o.color = i.color;
    o.uv0 = i.uv0;
    o.uv1 = i.uv1;
    o.uv2 = i.uv2;
    o.uv3 = i.uv3;
    return o;
}

// Glide's w-buffer (`grDepthBufferMode(GR_DEPTHBUFFER_WBUFFER)`), which the ATB programs
// use with `GR_CMP_LEQUAL`: 16 bits, the eye distance w as a float with a 4-bit exponent
// and a 12-bit mantissa. Surfaces in one plane, such as decals, round to the same value
// and the later one wins; a finer depth buffer lets them flicker. params[31] has the
// camera's near plane and far / (far - near); without them depth is left as it is.
struct Out {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};
fn glide_out(color: vec4<f32>, clip: vec4<f32>) -> Out {
    let d = params.v[31];
    if d.x <= 0.0 {
        return Out(color, clip.z);
    }
    let w = d.y * d.x / max(d.y - clip.z, 1e-12);
    if w < 1.0 {
        return Out(color, 0.0);
    }
    let e = min(floor(log2(w)), 15.0);
    let m = min(floor((w / exp2(e) - 1.0) * 4096.0), 4095.0);
    return Out(color, (e * 4096.0 + m + 0.5) / 65536.0);
}

// The color alone, and times the first picture.
@fragment
fn plain(i: V) -> Out {
    return glide_out(i.color, i.clip);
}
@fragment
fn textured(i: V) -> Out {
    return glide_out(i.color * textureSample(t0, s0, i.uv0.xy), i.clip);
}
