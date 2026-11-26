struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var dust: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;
struct Vertex {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) light: f32,
}
fn hash(seed: u32) -> f32 {
    let n = (seed ^ (seed >> 16u)) * 0x45d9f3bu;
    let m = (n ^ (n >> 16u)) * 0x45d9f3bu;
    return f32((m ^ (m >> 16u)) & 65535u) / 65535.0;
}
@vertex fn vs(@builtin(vertex_index) i: u32, @builtin(instance_index) j: u32) -> Vertex {
    let uv = array(vec2f(0.0, 1.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0),
        vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(1.0, 0.0))[i];
    let travel = hash(j + 100u) * 1000.0 + draw.c[4].w * 900.0;
    let x = hash(j + u32(travel / 1000.0) * 1003u) * 1000.0 - 500.0;
    let z = travel % 1000.0 - 500.0;
    let at = draw.c[5].xyz + vec3f(x, 10.0 + 75.0 * hash(j + 200u), z);
    let v = normalize(at - draw.c[4].xyz);
    let beam = normalize(draw.c[5].xyz - draw.c[4].xyz);
    let light = smoothstep(0.967, 0.994, dot(v, beam));
    let p = vec4f(at + draw.c[6].xyz * ((uv.x - 0.5) * 2.0)
        + draw.c[7].xyz * ((0.5 - uv.y) * 28.0), 1.0);
    let clip = vec4f(dot(draw.c[0], p), dot(draw.c[1], p), dot(draw.c[2], p), dot(draw.c[3], p));
    return Vertex(vec4f(clip.xy, (clip.z + clip.w) * 0.5, clip.w), uv, light);
}
@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    let texel = textureSample(dust, linear, v.uv);
    return vec4f(texel.rgb * texel.a * 0.45 * v.light, 0.0);
}
