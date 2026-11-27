// Dust in a box around the eye. After `silt.vxp`, with squares for points.
struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;

struct Mote {
    @builtin(position) position: vec4f,
    @location(0) at: vec2f,
    @location(1) fog: f32,
}

fn hash(index: u32) -> f32 {
    var x = index * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return f32((x >> 22u) ^ x) * (1.0 / 4294967296.0);
}

@vertex fn vs(@builtin(vertex_index) corner: u32, @builtin(instance_index) mote: u32) -> Mote {
    let eye = draw.c[4].xyz;
    let time = draw.c[4].w;
    let box = draw.c[5].x;
    let i = mote * 3u;
    var p = vec3f(hash(i), hash(i + 1u), hash(i + 2u)) * box;
    p.x += 10.0 * sin(p.x * 0.05 + time * 0.2);
    p.z += 10.0 * sin(p.z * 0.025 + time * 0.2 + 1.5);
    // The box repeats, so that dust stays where it is as the eye moves.
    p = eye + (fract((p - eye) / box + 0.5) - 0.5) * box;
    let clip = vec4f(dot(draw.c[0], vec4f(p, 1.0)), dot(draw.c[1], vec4f(p, 1.0)),
        dot(draw.c[2], vec4f(p, 1.0)), dot(draw.c[3], vec4f(p, 1.0)));
    let at = array(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
        vec2f(-1.0, 1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0))[corner];
    // Two pixels at 768 lines.
    let size = vec2f(1.0 / draw.c[5].y, 1.0) / 384.0 * clip.w;
    let fog = length(p - eye) / (box * 0.5);
    return Mote(vec4f(clip.xy + at * size, (clip.z + clip.w) * 0.5, clip.w), at, fog * fog);
}

@fragment fn fs(s: Mote) -> @location(0) vec4f {
    return vec4f(0.7, 0.8, 0.7, 0.4 * (1.0 - smoothstep(0.3, 1.0, length(s.at))) * saturate(1.0 - s.fog));
}
