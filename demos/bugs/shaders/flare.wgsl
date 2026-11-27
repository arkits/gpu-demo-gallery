struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var flare: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;
struct Vertex {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}
@vertex fn vs(@builtin(vertex_index) i: u32, @builtin(instance_index) j: u32) -> Vertex {
    let uv = array(vec2f(0.0, 1.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0),
        vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(1.0, 0.0))[i];
    let lamp = draw.c[4].xyz;
    let beam = normalize(draw.c[5].xyz - lamp);
    let right = normalize(cross(beam, vec3f(0.0, 0.0, 1.0)));
    let up = cross(right, beam);
    let angle = lamp.x * select(-0.003, 0.0048, j == 1u);
    let q = (uv - 0.5) * vec2f(800.0, -800.0);
    let offset = vec2f(cos(angle) * q.x - sin(angle) * q.y, sin(angle) * q.x + cos(angle) * q.y);
    let p = vec4f(lamp + right * offset.x + up * offset.y, 1.0);
    let clip = vec4f(dot(draw.c[0], p), dot(draw.c[1], p), dot(draw.c[2], p), dot(draw.c[3], p));
    return Vertex(vec4f(clip.xy, (clip.z + clip.w) * 0.5, clip.w), uv);
}
@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    return vec4f(textureSample(flare, linear, v.uv).rgb * 0.55, 0.0);
}
