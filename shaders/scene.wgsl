struct Frame {
    view_proj: mat4x4<f32>,
    inverse: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
    parameters: vec4<f32>,
}
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var wrap: sampler;
@group(1) @binding(0) var base: texture_2d<f32>;
@group(1) @binding(1) var second: texture_2d<f32>;
@group(1) @binding(2) var environment: texture_cube<f32>;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv2: vec2<f32>,
    @location(4) x: vec4<f32>,
    @location(5) y: vec4<f32>,
    @location(6) z: vec4<f32>,
    @location(7) origin: vec4<f32>,
    @location(8) diffuse: vec4<f32>,
    @location(9) ambient_mode: vec4<f32>,
}

struct Surface {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv2: vec2<f32>,
    @location(4) @interpolate(flat) diffuse: vec4<f32>,
    @location(5) @interpolate(flat) ambient_mode: vec4<f32>,
    @location(6) @interpolate(flat) axis_x: vec3<f32>,
    @location(7) @interpolate(flat) axis_y: vec3<f32>,
    @location(8) @interpolate(flat) axis_z: vec3<f32>,
}

@vertex fn surface_vs(v: Vertex) -> Surface {
    let world = mat4x4(v.x, v.y, v.z, v.origin) * vec4(v.position, 1.0);
    let axes = mat3x3(normalize(v.x.xyz), normalize(v.y.xyz), normalize(v.z.xyz));
    return Surface(frame.view_proj * world, world.xyz, axes * v.normal, v.uv, v.uv2,
        v.diffuse, v.ambient_mode, axes[0], axes[1], axes[2]);
}

fn cube_sample(direction: vec3<f32>) -> vec4<f32> {
    return textureSample(environment, wrap, direction * vec3(1.0, 1.0, -1.0));
}

@fragment fn surface_fs(s: Surface) -> @location(0) vec4<f32> {
    let mode = u32(s.ambient_mode.w);
    let normal = normalize(s.normal);
    let light = normalize(frame.light.xyz);
    let view = normalize(s.world - frame.eye.xyz);
    let map0 = textureSample(base, wrap, s.uv);
    let map1 = textureSample(second, wrap, s.uv2);
    let reflection = cube_sample(reflect(view, normal)).rgb;
    let refraction = cube_sample(refract(view, normal, 0.71)).rgb;
    let n = normalize(mat3x3(s.axis_x, s.axis_y, s.axis_z) * (map0.xyz * 2.0 - 1.0));
    let lit = max(s.ambient_mode.xyz, vec3(0.23)) + 0.76 * max(dot(normal, light), 0.0);
    var rgb = s.diffuse.rgb * lit;
    var alpha = 1.0;

    switch mode {
        case 1u: { rgb *= map0.rgb; }
        case 2u: { rgb = map1.rgb * (0.18 + 0.82 * max(dot(n, light), 0.0)); }
        case 3u: { rgb = rgb * map0.rgb + reflection * 0.34; }
        case 4u: { rgb = mix(rgb, reflection, 0.72); }
        case 5u: {
            rgb = refraction * vec3(0.94, 0.98, 1.0);
        }
        case 6u: {
            let fresnel = pow(1.0 - abs(dot(-view, normal)), 3.0);
            rgb = reflection;
            alpha = 0.09 + 0.58 * fresnel;
        }
        case 7u: { rgb = map0.rgb * (0.56 + map1.rgb * 0.44); }
        default: {}
    }
    return vec4(rgb * frame.parameters.x, alpha);
}

struct Quad {
    @builtin(position) position: vec4<f32>,
    @location(0) xy: vec2<f32>,
};

@vertex fn sky_vs(@builtin(vertex_index) index: u32) -> Quad {
    let p = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0))[index];
    return Quad(vec4(p, 1.0, 1.0), p);
}

@fragment fn sky_fs(s: Quad) -> @location(0) vec4<f32> {
    let far = frame.inverse * vec4(s.xy, 1.0, 1.0);
    let direction = normalize(far.xyz / far.w - frame.eye.xyz);
    return vec4(cube_sample(direction).rgb * frame.parameters.x * frame.parameters.y, 1.0);
}

@vertex fn badge_vs(@builtin(vertex_index) index: u32) -> Quad {
    let uv = array(vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(0.0, 0.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(1.0, 0.0))[index];
    let extent = vec2(min(0.38, 0.51 / frame.parameters.z), 0.15);
    return Quad(vec4(vec2(1.0 - extent.x, -1.0) + vec2(uv.x, 1.0 - uv.y) * extent, 0.0, 1.0), uv);
}

@fragment fn badge_fs(s: Quad) -> @location(0) vec4<f32> {
    return textureSample(base, wrap, s.xy);
}

@vertex fn shadow_vs(v: Vertex) -> @builtin(position) vec4<f32> {
    let p = mat4x4(v.x, v.y, v.z, v.origin) * vec4(v.position, 1.0);
    let shift = frame.light.xz / frame.light.y * p.y;
    return frame.view_proj * vec4(p.x - shift.x, 0.025, p.z - shift.y, 1.0);
}

@fragment fn shadow_fs() -> @location(0) vec4<f32> {
    return vec4(0.07, 0.05, 0.08, 0.34);
}

fn random(seed: u32) -> f32 {
    var x = seed * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return f32((x >> 22u) ^ x) * (1.0 / 4294967296.0);
}

@vertex fn snow_vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) flake: u32) -> Quad {
    let i = flake * 7u;
    let age = frame.parameters.w;
    let azimuth = random(i + 1u) * 6.2831853 + age * mix(0.1, 0.5, random(i + 2u));
    let polar = random(i + 3u) * 3.1415927 + age * mix(0.1, 0.3, random(i + 4u));
    let radius = mix(2.0, 9.8, random(i));
    let local = array(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0))[vertex];
    let p = radius * vec3(sin(polar) * cos(azimuth), cos(polar), sin(polar) * sin(azimuth));
    let clip = frame.view_proj * vec4(p, 1.0);
    let scale = select(0.0, 0.15 + 0.06 * sin(age * (0.1 + random(i + 5u) * 9.9)),
        polar < 3.1415927);
    // The glass is a separate pass. A small depth offset keeps flakes inside it visible.
    return Quad(vec4(clip.xy + local * scale, clip.z - 0.25 * clip.w, clip.w), local);
}

@fragment fn snow_fs(s: Quad) -> @location(0) vec4<f32> {
    return vec4(0.93, 0.96, 1.0, (1.0 - smoothstep(0.25, 1.0, length(s.xy))) * 0.86 * frame.parameters.x);
}
