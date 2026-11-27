// Common to the generated programs, which add `shade` and their textures.
struct M {
    view_proj: mat4x4f,
    view: mat4x4f,
    color0: vec4f,
    color1: vec4f,
    diffuse: vec4f,
    // Direction to the light, in the camera's space.
    light: vec4f,
    // Pixels: width, height, size of points and lines.
    screen: vec4f,
}
@group(0) @binding(0) var<uniform> m: M;
@group(0) @binding(1) var samp: sampler;

struct V {
    @builtin(position) clip: vec4f,
    @location(0) eye: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) primary: vec3f,
}
@vertex
fn vs(@location(0) pos: vec3f, @location(1) normal: vec3f, @location(2) uv: vec2f) -> V {
    var v: V;
    v.clip = m.view_proj * vec4f(pos, 1.0);
    v.eye = (m.view * vec4f(pos, 1.0)).xyz;
    v.normal = (m.view * vec4f(normal, 0.0)).xyz;
    v.uv = uv;
    // One white directional light and nothing else.
    v.primary = saturate(m.diffuse.rgb * max(dot(v.normal, m.light.xyz), 0.0));
    return v;
}
@fragment
fn fs(v: V) -> @location(0) vec4f {
    return shade(v, vec4f(v.primary, m.diffuse.a));
}
