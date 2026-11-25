struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var picture: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;

struct Corner {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex fn vs(@builtin(vertex_index) i: u32) -> Corner {
    let uv = array(vec2f(0.0, 1.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0),
        vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(1.0, 0.0))[i];
    // Left, top, width and height, as parts of the window.
    let rect = draw.c[0];
    let at = rect.xy + uv * rect.zw;
    return Corner(vec4f(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0), uv);
}

@fragment fn fs(s: Corner) -> @location(0) vec4f {
    return textureSample(picture, linear, s.uv);
}
