struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var sky: texture_cube<f32>;
@group(1) @binding(1) var linear: sampler;

struct Corner {
    @builtin(position) position: vec4f,
    @location(0) at: vec2f,
}

@vertex fn vs(@builtin(vertex_index) i: u32) -> Corner {
    let p = array(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0))[i];
    return Corner(vec4f(p, 1.0, 1.0), p);
}

@fragment fn fs(s: Corner) -> @location(0) vec4f {
    let far = vec4f(s.at, 1.0, 1.0);
    let w = dot(draw.c[3], far);
    let direction = vec3f(dot(draw.c[0], far), dot(draw.c[1], far), dot(draw.c[2], far)) / w;
    return vec4f(textureSample(sky, linear, direction).rgb, 1.0);
}
