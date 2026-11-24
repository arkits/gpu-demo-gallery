// Vertices as squares, edges as bands, and images.
struct M {
    view_proj: mat4x4f,
    view: mat4x4f,
    color0: vec4f,
    color1: vec4f,
    diffuse: vec4f,
    light: vec4f,
    screen: vec4f,
}
@group(0) @binding(0) var<uniform> m: M;
@group(0) @binding(1) var samp: sampler;

fn corner(i: u32) -> vec2f {
    return vec2f(f32(i & 1u), f32(i >> 1u)) * 2.0 - 1.0;
}
@vertex
fn point_vs(@builtin(vertex_index) i: u32, @location(0) pos: vec3f) -> @builtin(position) vec4f {
    let c = m.view_proj * vec4f(pos, 1.0);
    return vec4f(c.xy + corner(i) * 2.0 * m.screen.z / m.screen.xy * c.w, c.zw);
}
@vertex
fn line_vs(@builtin(vertex_index) i: u32, @location(0) a: vec3f, @location(1) b: vec3f) -> @builtin(position) vec4f {
    let ca = m.view_proj * vec4f(a, 1.0);
    let cb = m.view_proj * vec4f(b, 1.0);
    let along = (cb.xy / cb.w - ca.xy / ca.w) * m.screen.xy;
    let side = vec2f(-along.y, along.x) / max(length(along), 1e-4);
    let k = corner(i);
    let c = select(ca, cb, k.x > 0.0);
    return vec4f(c.xy + side * k.y * m.screen.z / m.screen.xy * c.w, c.zw);
}
@fragment
fn white_fs() -> @location(0) vec4f {
    return vec4f(1.0);
}

struct Q {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}
@group(1) @binding(0) var image: texture_2d<f32>;
@vertex
fn image_vs(@location(0) pos: vec2f, @location(1) uv: vec2f, @location(2) color: vec4f) -> Q {
    return Q(vec4f(pos, 0.0, 1.0), uv, color);
}
@fragment
fn image_fs(q: Q) -> @location(0) vec4f {
    return textureSample(image, samp, q.uv) * q.color;
}
@fragment
fn color_fs(q: Q) -> @location(0) vec4f {
    return q.color;
}
