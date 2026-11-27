struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;
struct Vertex {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}
@vertex fn vs(@builtin(vertex_index) i: u32) -> Vertex {
    let uv = array(vec2f(0.0, 1.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0),
        vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(1.0, 0.0))[i];
    let r = draw.c[0];
    let at = r.xy + uv * r.zw;
    return Vertex(vec4f(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0), uv);
}
@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    if draw.c[1].x > 0.0 {
        let size = textureDimensions(image);
        return vec4f(textureLoad(image, vec2i(v.uv * vec2f(size)), 0).rgb, 1.0);
    }
    return textureSample(image, linear, v.uv);
}
