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

// The color alone, and times the first picture.
@fragment
fn plain(i: V) -> @location(0) vec4<f32> {
    return i.color;
}
@fragment
fn textured(i: V) -> @location(0) vec4<f32> {
    return i.color * textureSample(t0, s0, i.uv0.xy);
}
