// Bubbles that rise around the eye. After `bubble.vxp`, with squares for points.
struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(0) var shape: texture_2d<f32>;
@group(1) @binding(1) var colors: texture_2d<f32>;
@group(1) @binding(2) var linear: sampler;
@group(1) @binding(3) var linear_colors: sampler;

struct Bubble {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) fog: f32,
    @location(2) tone: f32,
}

fn hash(index: u32) -> f32 {
    var x = index * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return f32((x >> 22u) ^ x) * (1.0 / 4294967296.0);
}

@vertex fn vs(@builtin(vertex_index) corner: u32, @builtin(instance_index) bubble: u32) -> Bubble {
    let eye = draw.c[4].xyz;
    let time = draw.c[4].w;
    let box = draw.c[5].x;
    let i = 7919u + bubble * 4u;
    let speed = 4.0 + 8.0 * hash(i + 3u);
    var p = vec3f(hash(i), hash(i + 1u), hash(i + 2u)) * box;
    p.y += time * speed;
    p.x += sin(time * 1.7 + f32(bubble)) * 1.2;
    p.z += cos(time * 1.3 + f32(bubble)) * 1.2;
    p = eye + (fract((p - eye) / box + 0.5) - 0.5) * box;
    let at = array(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
        vec2f(-1.0, 1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0))[corner];
    let radius = 0.15 + 0.35 * hash(i + 2u);
    let q = vec4f(p, 1.0);
    let clip = vec4f(dot(draw.c[0], q), dot(draw.c[1], q), dot(draw.c[2], q), dot(draw.c[3], q));
    let offset = at * radius * draw.c[6].xy;
    let away = length(p - eye);
    let fog = away / (box * 0.5);
    // Close ones would fill the picture.
    let near = smoothstep(6.0, 14.0, away);
    let tone = 0.5 + 0.5 * (p.y - eye.y) / max(away, 0.01);
    return Bubble(vec4f(clip.xy + offset * near, (clip.z + clip.w) * 0.5, clip.w),
        at * vec2f(0.5, -0.5) + 0.5, fog * fog, tone);
}

@fragment fn fs(s: Bubble) -> @location(0) vec4f {
    let t = textureSample(shape, linear, s.uv);
    let tint = textureSample(colors, linear_colors, vec2f(s.tone, 0.5));
    return vec4f(t.rgb * tint.rgb, t.a * saturate(1.0 - s.fog));
}
