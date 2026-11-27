// Copies the finished frame to the window surface.
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.clip = vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}

// Scene alpha holds shadow data, so output is always opaque.
@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    return vec4(textureSample(image, samp, i.uv).rgb, 1.0);
}

// For surfaces that only offer sRGB formats: undo the hardware encode.
@fragment
fn fs_srgb(i: Out) -> @location(0) vec4<f32> {
    let c = textureSample(image, samp, i.uv).rgb;
    let lin = select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
    return vec4(lin, 1.0);
}
