// M2 preview shading: base texture or material color times baked vertex light.
// Replaced by translated original shaders in M3.
struct Frame { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var base: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct In {
    @location(0) position: vec3<f32>,
    @location(1) light: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};
struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) light: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs(v: In) -> Out {
    var o: Out;
    o.clip = frame.view_proj * vec4(v.position, 1.0);
    // D3DCOLOR is stored BGRA.
    o.light = v.light.bgr;
    o.color = v.color.bgr;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    let b = textureSample(base, samp, i.uv).rgb;
    return vec4(b * i.color * i.light * 2.0, 1.0);
}
