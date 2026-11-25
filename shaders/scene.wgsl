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
    let mode = u32(v.ambient_mode.w);
    var position = v.position;
    if mode == 8u { position += v.normal * v.ambient_mode.x; }
    if mode == 5u && frame.parameters.y > 0.5 {
        position.y += sin(position.x * 1.1 + frame.parameters.x * 2.5) * 0.048
                    + sin(position.z * 1.23 - frame.parameters.x * 2.2) * 0.031;
    }
    let world = mat4x4(v.x, v.y, v.z, v.origin) * vec4(position, 1.0);
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
    let dx = dpdx(s.world);
    let dy = dpdy(s.world);
    let du = dpdx(s.uv);
    let dv = dpdy(s.uv);
    let side = cross(dy, normal);
    let up = cross(normal, dx);
    let tangent = side * du.x + up * dv.x;
    let bitangent = side * du.y + up * dv.y;
    let inverse_length = inverseSqrt(max(0.000001, max(dot(tangent, tangent), dot(bitangent, bitangent))));
    let n = normalize(mat3x3(tangent * inverse_length, bitangent * inverse_length, normal) * (map1.xyz * 2.0 - 1.0));
    let wave = 0.075 * frame.parameters.y * vec3(
        cos(s.world.x * 1.1 + frame.parameters.x * 2.5), 0.0,
        cos(s.world.z * 1.23 - frame.parameters.x * 2.2));
    let reflection = cube_sample(reflect(view, normalize(normal + select(vec3(0.0), wave, mode == 5u)))).rgb;
    let lit = max(s.ambient_mode.xyz * select(1.0, 0.0, mode == 8u), vec3(0.24))
        + 0.76 * max(dot(normal, light), 0.0);
    var rgb = s.diffuse.rgb * lit;
    var alpha = 1.0;
    switch mode {
        case 1u: { rgb *= map0.rgb; }
        case 2u: {
            let specular = pow(max(dot(n, normalize(light - view)), 0.0), 19.0);
            rgb = s.diffuse.rgb * map0.rgb * (0.25 + 0.75 * max(dot(n, light), 0.0)) + 0.19 * specular;
        }
        case 3u: { rgb = rgb * map0.rgb + reflection * 0.23; }
        case 4u: { rgb = mix(rgb, reflection, 0.64); }
        case 5u: {
            if length(s.world.xz) > 41.0 { discard; }
            let fresnel = pow(1.0 - clamp(abs(dot(-view, normal)), 0.0, 1.0), 3.0);
            rgb = mix(vec3(0.13, 0.32, 0.30), reflection, 0.56 + 0.32 * fresnel);
            alpha = (0.30 + 0.47 * fresnel) * (1.0 - smoothstep(39.0, 41.0, length(s.world.xz)));
        }
        case 6u: {
            if map0.a < 0.32 { discard; }
            rgb *= map0.rgb;
        }
        case 7u: { rgb = map0.rgb * (0.42 + map1.rgb * 0.58); }
        case 8u: {
            if map1.a < 0.06 { discard; }
            rgb = map0.rgb * lit * (0.48 + 0.52 * map1.rgb);
            alpha = map1.a * 0.72;
        }
        default: {}
    }
    if frame.parameters.w == 2.0 {
        let fog = 1.0 - exp(-length(s.world - frame.eye.xyz) / 320.0);
        rgb = mix(rgb, vec3(0.025, 0.22, 0.24), fog);
    }
    return vec4(rgb, alpha);
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
    var rgb = cube_sample(direction).rgb;
    if frame.parameters.w == 5.0 {
        rgb = vec3(0.027, 0.014, 0.049) + max(direction.y, 0.0) * vec3(0.06, 0.03, 0.09);
    }
    return vec4(rgb, 1.0);
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

@fragment fn overdraw_fs() -> @location(0) vec4<f32> {
    return vec4(0.12, 0.055, 0.024, 1.0);
}

fn hash(index: u32) -> f32 {
    var x = index * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return f32((x >> 22u) ^ x) * (1.0 / 4294967296.0);
}

@vertex fn silt_vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) mote: u32) -> Quad {
    let i = mote * 4u;
    let t = frame.parameters.x;
    let p = vec3(hash(i) - 0.5, hash(i + 1u) - 0.5, hash(i + 2u) - 0.5) * 210.0;
    let drift = vec3(3.0 * sin(p.x * 0.05 + t), 0.0, 4.0 * sin(p.z * 0.025 + t + 1.5));
    let clip = frame.view_proj * vec4(p + drift, 1.0);
    let local = array(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0))[vertex];
    return Quad(vec4(clip.xy + local * 0.12, clip.z, clip.w), local);
}

@fragment fn silt_fs(s: Quad) -> @location(0) vec4<f32> {
    return vec4(0.3, 0.7, 0.57, 0.44 * (1.0 - smoothstep(0.2, 1.0, length(s.xy))));
}

@vertex fn bubble_vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) bubble: u32) -> Quad {
    let i = 71u + bubble * 3u;
    let life = hash(i + 2u) * 2.0;
    let y = fract(0.08 * frame.parameters.x + life) * 120.0 - 55.0;
    let p = vec3((hash(i) - 0.5) * 170.0 + 1.9 * sin(y * 0.4 + life), y,
        (hash(i + 1u) - 0.5) * 160.0);
    let clip = frame.view_proj * vec4(p, 1.0);
    let local = array(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0))[vertex];
    return Quad(vec4(clip.xy + local * (0.18 + life * 0.27), clip.z, clip.w), local);
}

@fragment fn bubble_fs(s: Quad) -> @location(0) vec4<f32> {
    let r = length(s.xy);
    let rim = smoothstep(0.55, 0.84, r) * (1.0 - smoothstep(0.84, 1.0, r));
    let highlight = 1.0 - smoothstep(0.06, 0.26, length(s.xy - vec2(-0.35, 0.44)));
    return vec4(0.6, 0.85, 0.92, (0.37 * rim + 0.50 * highlight));
}
