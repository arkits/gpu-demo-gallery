@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear: sampler;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex fn vs(@builtin(vertex_index) i: u32) -> Vertex {
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return Vertex(vec4f(2.0 * uv.x - 1.0, 1.0 - 2.0 * uv.y, 0.0, 1.0), uv);
}

@fragment fn fs(in: Vertex) -> @location(0) vec4f {
    return textureSample(source, linear, in.uv);
}
