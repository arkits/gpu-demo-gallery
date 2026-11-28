// Mad Mod Mike: skinned meshes, hair lines, cards and particles.
//
// Group 0: frame uniforms and the bone palette (dynamic offset per shape).
// Group 1: up to four 2D textures and a sampler.
struct Uniforms {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    eye: vec4<f32>,
    // Local seconds, window size, and how white the picture is.
    time: vec4<f32>,
    effects: vec4<f32>,
    shadow_view_proj: mat4x4<f32>,
    // Point lights: color and gain, then position and range.
    lights: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<uniform> bones: array<mat4x4<f32>, 96>;

@group(1) @binding(0) var tex0: texture_2d<f32>;
@group(1) @binding(1) var tex1: texture_2d<f32>;
@group(1) @binding(2) var tex2: texture_2d<f32>;
@group(1) @binding(3) var tex3: texture_2d<f32>;
@group(1) @binding(4) var samp: sampler;
@group(2) @binding(0) var mermaid_tile: texture_2d<f32>;
@group(2) @binding(1) var shadow_depth: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;
@group(2) @binding(3) var atlas_sampler: sampler;

struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) weights0: vec4<f32>,
    @location(5) weights1: vec4<f32>,
    @location(6) bones0: vec4<f32>,
    @location(7) bones1: vec4<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

fn skin(weights: array<f32, 8>, joints: array<u32, 8>, position: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    var wsum = 0.0;
    var p = vec4<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0u; i < 8u; i++) {
        let w = weights[i];
        wsum += w;
        if (w != 0.0) {
            let m = bones[joints[i]];
            p += w * (m * vec4<f32>(position, 1.0));
            n += w * (m * vec4<f32>(normal, 0.0)).xyz;
        }
    }
    if (wsum < 0.0001) {
        // No weights filed: rigidly carried by the single bone.
        let m = bones[0];
        p = m * vec4<f32>(position, 1.0);
        n = (m * vec4<f32>(normal, 0.0)).xyz;
    }
    return vec4<f32>(p.xyz, 1.0);
}

fn skin_normal(weights: array<f32, 8>, joints: array<u32, 8>, normal: vec3<f32>) -> vec3<f32> {
    var wsum = 0.0;
    var n = vec3<f32>(0.0);
    for (var i = 0u; i < 8u; i++) {
        let w = weights[i];
        wsum += w;
        if (w != 0.0) {
            n += w * (bones[joints[i]] * vec4<f32>(normal, 0.0)).xyz;
        }
    }
    if (wsum < 0.0001) {
        // Rigid carry, like the positions above.
        return (bones[0] * vec4<f32>(normal, 0.0)).xyz;
    }
    return n;
}

fn mesh_vertex(in: MeshIn) -> MeshOut {
    let weights = array<f32, 8>(
        in.weights0.x, in.weights0.y, in.weights0.z, in.weights0.w,
        in.weights1.x, in.weights1.y, in.weights1.z, in.weights1.w,
    );
    let joints = array<u32, 8>(
        u32(in.bones0.x), u32(in.bones0.y), u32(in.bones0.z), u32(in.bones0.w),
        u32(in.bones1.x), u32(in.bones1.y), u32(in.bones1.z), u32(in.bones1.w),
    );
    let world = skin(weights, joints, in.position, in.normal.xyz);
    let nrm = skin_normal(weights, joints, in.normal.xyz);
    var out: MeshOut;
    out.clip = uniforms.projection * uniforms.view * world;
    out.world = world.xyz;
    out.normal = nrm;
    out.uv = in.uv;
    out.tangent = vec4<f32>(skin_normal(weights, joints, in.tangent.xyz), in.tangent.w);
    return out;
}

@vertex
fn vs_main(in: MeshIn) -> MeshOut {
    return mesh_vertex(in);
}

fn unit_or(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let l2 = dot(v, v);
    if (l2 > 0.0000001 && l2 < 100000000.0) {
        return v * inverseSqrt(l2);
    }
    return fallback;
}

fn material_normal(in: MeshOut, room: bool) -> vec3<f32> {
    let dx = dpdx(in.world);
    let dy = dpdy(in.world);
    let du = dpdx(in.uv);
    let dv = dpdy(in.uv);
    let geometric = unit_or(cross(dx, dy), vec3<f32>(0.0, 1.0, 0.0));
    let oriented = select(-geometric, geometric, dot(geometric, uniforms.eye.xyz - in.world) > 0.0);
    let n = unit_or(in.normal, oriented);
    if (any(textureDimensions(tex1) > vec2<u32>(1u))) {
        let bump = textureSample(tex1, samp, in.uv);
        let xy = select((bump.xy * 255.0 - 128.0) / 127.0, bump.wy * 2.0 - 1.0, room);
        let uv_sign = sign(du.x * dv.y - du.y * dv.x);
        let local_t = uv_sign * (dx * dv.y - dy * du.y);
        let local_b = uv_sign * (dy * du.x - dx * dv.x);
        let tangent = unit_or(in.tangent.xyz - n * dot(in.tangent.xyz, n), unit_or(local_t, vec3<f32>(0.0)));
        let binormal = unit_or(cross(n, tangent) * in.tangent.w, unit_or(local_b, vec3<f32>(0.0)));
        return unit_or(n + 2.4 * (tangent * xy.x + binormal * xy.y), n);
    }
    return n;
}

@vertex
fn vs_shadow(in: MeshIn) -> @builtin(position) vec4<f32> {
    return uniforms.shadow_view_proj * vec4<f32>(mesh_vertex(in).world, 1.0);
}

fn shadow_at(world: vec3<f32>) -> f32 {
    let clip = uniforms.shadow_view_proj * vec4<f32>(world, 1.0);
    let ndc = clip.xyz / max(clip.w, 0.001);
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if (uniforms.effects.w < 0.5 || any(abs(ndc.xy) > vec2<f32>(0.99)) || ndc.z <= 0.0 || ndc.z >= 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_depth, shadow_sampler, uv, ndc.z - 0.0018);
}

fn video_color(uv: vec2<f32>) -> vec3<f32> {
    let cell = u32(uniforms.effects.x) % 9u;
    let top_left = vec2<f32>(f32(cell % 3u), f32(cell / 3u)) * 341.0;
    let tile_uv = (top_left + clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * 339.0 + 1.0) / 1024.0;
    return textureSampleLevel(mermaid_tile, atlas_sampler, tile_uv, 0.0).rag;
}

fn lit_material(in: MeshOut, room: bool, skin_wrap: bool, emissive: bool) -> vec4<f32> {
    let diffuse = textureSample(tex0, samp, in.uv);
    let n = material_normal(in, room);
    let eye_ray = uniforms.eye.xyz - in.world;
    let eye = eye_ray * inverseSqrt(max(dot(eye_ray, eye_ray), 0.0001));
    var ao = 1.0;
    if (any(textureDimensions(tex2) > vec2<u32>(1u))) {
        let a = textureSample(tex2, samp, in.uv).r;
        ao = select(0.29 + 0.71 * a * 1.3, a * 1.5 + 0.1, room);
    }
    // The static room occlusion textures modulate both key and bounce light.
    let bounce = uniforms.lights[10];
    let sky = bounce.rgb * (0.7 + 0.3 * n.y) * (0.55 + 0.45 * bounce.w);
    var light = sky + select(vec3<f32>(0.17, 0.18, 0.23) * (0.5 + 0.5 * bounce.w), vec3<f32>(0.0), room);
    var spec = vec3<f32>(0.0);
    for (var i = 0u; i < 5u; i++) {
        let source = uniforms.lights[i * 2u];
        let point = uniforms.lights[i * 2u + 1u];
        let ray = point.xyz - in.world;
        let d = max(length(ray), 0.01);
        let l = ray / d;
        let ndl = dot(l, n);
        let wrap = select(vec3<f32>(max(ndl, 0.0)),
            clamp((vec3<f32>(ndl) + vec3<f32>(0.33, 0.12, 0.08)) / vec3<f32>(1.33, 1.12, 1.08), vec3<f32>(0.0), vec3<f32>(1.0)), skin_wrap);
        var falloff = 1.0;
        if (point.w > 0.0) {
            let range = d / point.w;
            falloff = 1.0 / (1.0 + range * range * 2.0);
        }
        if (i == 1u && uniforms.effects.y == 0.0) {
            let lamp_axis = normalize(vec3<f32>(17.51, -33.7, -7.8));
            falloff *= 0.12 + 0.88 * smoothstep(0.26, 0.88, dot(-l, lamp_axis));
        }
        var tint = source.rgb;
        if (i == 3u && source.w > 0.0) {
            let forward = dot(-ray, vec3<f32>(-0.9798, -0.2, 0.0));
            let across = vec2<f32>(-ray.z, dot(-ray, vec3<f32>(0.2, -0.9798, 0.0)));
            let st = 0.5 + across / max(0.8 * forward, 1.0);
            let inside = 1.0 - smoothstep(0.46, 0.52, max(abs(st.x - 0.5), abs(st.y - 0.5)));
            tint = mix(vec3<f32>(0.10, 0.34, 0.38), video_color(st), inside);
            falloff *= max(forward / d, 0.0);
        }
        if (i == u32(uniforms.effects.z)) {
            falloff *= mix(0.36, 1.0, shadow_at(in.world));
        }
        let value = tint * (source.w * falloff);
        light += value * wrap;
        let highlight = pow(clamp(dot(reflect(-l, n), eye), 0.0001, 1.0), select(28.0, 48.0, room));
        spec += value * highlight * smoothstep(0.0, 0.2, ndl);
    }
    let hall = uniforms.lights[12];
    let hall_ray = uniforms.lights[13].xyz - in.world;
    let hall_dir = unit_or(hall_ray, vec3<f32>(0.0, 0.0, 1.0));
    light += hall.rgb * hall.w * max(dot(hall_dir, n), 0.24)
        * exp(-dot(hall_ray, hall_ray) / 7100.0) * smoothstep(177.0, 217.0, in.world.z);
    if (!room) {
        // The original Mike shader includes the +Z hallway fill independently of the key.
        light += vec3<f32>(0.21, 0.17, 0.16) * max(n.z, 0.0);
    }
    let luminance = 0.84 * diffuse.rgb * light * ao + spec * diffuse.a * 0.58;
    let glow = select(vec3<f32>(0.0), diffuse.rgb * 0.52, emissive);
    return vec4<f32>((luminance + glow) * 1.26, diffuse.a);
}

fn with_depth(color: vec4<f32>, world: vec3<f32>) -> vec4<f32> {
    let depth = -(uniforms.view * vec4<f32>(world, 1.0)).z;
    return vec4<f32>(color.rgb, clamp(depth / 800.0, 0.0, 1.0));
}

@fragment
fn fs_main(in: MeshOut) -> @location(0) vec4<f32> {
    return with_depth(lit_material(in, false, false, false), in.world);
}

@fragment
fn fs_main_alpha(in: MeshOut) -> @location(0) vec4<f32> {
    return lit_material(in, false, false, false);
}

@fragment
fn fs_skin(in: MeshOut) -> @location(0) vec4<f32> {
    return with_depth(lit_material(in, false, true, false), in.world);
}

@fragment
fn fs_room(in: MeshOut) -> @location(0) vec4<f32> {
    return with_depth(lit_material(in, true, false, false), in.world);
}

@fragment
fn fs_room_alpha(in: MeshOut) -> @location(0) vec4<f32> {
    return lit_material(in, true, false, false);
}

@fragment
fn fs_emissive(in: MeshOut) -> @location(0) vec4<f32> {
    return with_depth(lit_material(in, true, false, true), in.world);
}

@fragment
fn fs_keyboard(in: MeshOut) -> @location(0) vec4<f32> {
    let panel = textureSample(tex0, samp, in.uv);
    let lcd = dot(panel.rgb, vec3<f32>(0.20, 0.72, 0.08));
    return with_depth(vec4<f32>(lcd * vec3<f32>(0.26, 1.26, 0.20), 1.0), in.world);
}

@fragment
fn fs_screen_old(in: MeshOut) -> @location(0) vec4<f32> {
    // screen_old.vp30 and screen_old.fp30: rolling amber CRT interference.
    let t = uniforms.time.x;
    let tick = floor(t * 30.0);
    let offset = fract(vec2<f32>(tick * 0.75487766, tick * 0.56984029));
    let uv = in.uv * vec2<f32>(1.6, 1.0) * 0.35 + offset;
    let noise = textureSample(tex0, samp, uv).rgb * 0.20;
    let freq = 18.0 + 6.0 * sin(t * 1.7);
    let amplitude = 0.3 * clamp(-1.5 + 2.5 * sin(t * 3.31) + 1.5 * sin(t * 4.59), 0.0, 1.0);
    let phase = in.uv.y * 1.3 * 6.28 + t * 27.0 + amplitude * sin((in.uv.x - 0.5) * freq);
    let scan = 0.30 * pow(0.5 + 0.5 * sin(phase), 4.0);
    return with_depth(vec4<f32>((noise + vec3<f32>(scan)) * vec3<f32>(1.0, 0.9, 0.1) * 1.26, 1.0), in.world);
}

@fragment
fn fs_screen(in: MeshOut) -> @location(0) vec4<f32> {
    // Each filed 1024×1024 tile stores nine 341×341 frames. Cg swizzles XWY.
    let frame = video_color(vec2<f32>(in.uv.x, 1.0 - in.uv.y));
    let edge = max(1.0 - smoothstep(0.0, 0.055, in.uv.x), smoothstep(0.945, 1.0, in.uv.x));
    return with_depth(vec4<f32>(mix(frame, vec3<f32>(33.0, 82.0, 78.0) / 256.0, edge) * 1.26, 1.0), in.world);
}

@fragment
fn fs_flame_mask(in: MeshOut) -> @location(0) vec4<f32> {
    let strength = (110.0 + 40.0 * pow(clamp(1.0 - in.uv.y, 0.0, 1.0), 2.0)) / 256.0;
    return vec4<f32>(vec3<f32>(strength * uniforms.lights[11].x), 1.0);
}

// Hair strands: each instance is one segment (two endpoints of 11 floats).
struct HairIn {
    @location(0) a_position: vec3<f32>,
    @location(1) a_tangent: vec3<f32>,
    @location(2) a_normal: vec3<f32>,
    @location(3) a_color: vec2<f32>,
    @location(4) b_position: vec3<f32>,
    @location(5) b_tangent: vec3<f32>,
    @location(6) b_normal: vec3<f32>,
    @location(7) b_color: vec2<f32>,
};

struct HairOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) shade: f32,
    @location(1) along: f32,
};

@vertex
fn vs_hair(@builtin(vertex_index) vertex: u32, in: HairIn) -> HairOut {
    let end = vertex == 2u || vertex == 3u || vertex == 5u;
    let p = select(in.a_position, in.b_position, end);
    let local_tangent = select(in.a_tangent, in.b_tangent, end);
    let tangent = normalize((bones[0] * vec4<f32>(local_tangent, 0.0)).xyz);
    let sway = sin(uniforms.time.x * 1.7 + p.x * 0.11 + p.y * 0.13) * 0.35;
    let moved = p + vec3<f32>(sway, sway * 0.3, 0.0);
    let side = normalize(cross(tangent, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(0.001, 0.0, 0.0));
    let width = 0.22;
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let c = corner[vertex];
    let across = (uniforms.view * vec4<f32>(side, 0.0)).xy;

    var out: HairOut;
    let clip = uniforms.projection * uniforms.view * bones[0] * vec4<f32>(moved, 1.0);
    out.clip = clip + vec4<f32>(across * c.x * width * 0.01 * clip.w, 0.0, 0.0);
    let diffuse = abs(dot(tangent, normalize(vec3<f32>(0.4, 0.8, 0.45))));
    out.shade = 0.25 + 0.75 * diffuse;
    out.along = select(in.a_color.y, in.b_color.y, end);
    return out;
}

@fragment
fn fs_hair(in: HairOut) -> @location(0) vec4<f32> {
    let root = vec3<f32>(0.16, 0.10, 0.07);
    let tip = vec3<f32>(0.35, 0.22, 0.13);
    var color = mix(root, tip, in.along) * in.shade;
    color = color / (1.0 + color);
    let white = uniforms.time.w;
    color = mix(color, vec3<f32>(1.0), white);
    return vec4<f32>(color, 0.85);
}

// The wet cornea layer: a transparent gloss over the textured eyeball.
// The port has no cube maps, so the environment reflection that feeds it
// in the original (`wetballEnvCube`) is an analytic highlight instead [I].
@fragment
fn fs_wetball(in: MeshOut) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let view_dir = normalize(uniforms.eye.xyz - in.world);
    let light_dir = normalize(vec3<f32>(0.4, 0.8, 0.45));
    let half_dir = normalize(view_dir + light_dir);
    let spec = pow(max(dot(normal, half_dir), 0.0), 60.0);
    let alpha = 0.08 + 0.6 * spec;
    return vec4<f32>(vec3<f32>(0.9, 0.95, 1.0) * (0.04 + spec), alpha);
}

// Menu buttons: NDC quads, textured like cards.
struct MenuIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct CardOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_menu(in: MenuIn) -> MeshOut {
    var out: MeshOut;
    out.clip = vec4<f32>(in.position, 0.0, 1.0);
    out.world = vec3<f32>(in.position, 0.0);
    out.normal = vec3<f32>(0.0, 0.0, 1.0);
    out.uv = in.uv;
    out.tangent = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    return out;
}

// Fullscreen cards and menu quads: positions are already placed.
@vertex
fn vs_card(@builtin(vertex_index) vertex: u32) -> CardOut {
    let corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    var out: CardOut;
    out.clip = vec4<f32>(corners[vertex], 0.0, 1.0);
    out.uv = vec2<f32>(corners[vertex].x * 0.5 + 0.5, 1.0 - (corners[vertex].y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_moon(in: CardOut) -> @location(0) vec4<f32> {
    let aspect = uniforms.time.y / uniforms.time.z;
    let scale = vec2<f32>(min(aspect / 1.25, 1.0), min(1.25 / aspect, 1.0));
    let uv = (in.uv - vec2<f32>(0.5)) * scale + vec2<f32>(0.5);
    return vec4<f32>(textureSample(tex0, samp, uv).rgb, 1.0);
}

@fragment
fn fs_card(in: MeshOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex0, samp, in.uv);
    if (texel.a < 0.01) {
        discard;
    }
    let white = uniforms.time.w;
    let color = mix(texel.rgb / (1.0 + texel.rgb), vec3<f32>(1.0), white);
    return vec4<f32>(color, texel.a);
}

// Particles: instanced billboards around an emitter.
struct ParticleIn {
    @location(0) emitter: vec3<f32>,
    @location(1) seed: vec4<f32>,
};

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
};

@vertex
fn vs_particle(@builtin(vertex_index) vertex: u32, in: ParticleIn) -> ParticleOut {
    let life = 2.0 + in.seed.w * 4.0;
    let age = (uniforms.time.x * (0.5 + in.seed.x * 0.5) + in.seed.y * life) % life;
    let t = age / life;
    let spread = vec3<f32>(in.seed.x - 0.5, in.seed.y - 0.5, in.seed.z - 0.5) * 60.0;
    let rise = vec3<f32>(t * 8.0 * (in.seed.z - 0.3), t * 45.0, t * 6.0 * (in.seed.x - 0.5));
    let center = in.emitter + spread * (0.3 + t) + rise;
    let size = (2.0 + in.seed.w * 7.0) * (1.0 - t * 0.6);
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[vertex];
    let right = vec3<f32>(uniforms.view[0][0], uniforms.view[1][0], uniforms.view[2][0]);
    let up = vec3<f32>(uniforms.view[0][1], uniforms.view[1][1], uniforms.view[2][1]);
    let world = center + (right * c.x + up * c.y) * size;
    var out: ParticleOut;
    out.clip = uniforms.projection * uniforms.view * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(c.x * 0.5 + 0.5, c.y * 0.5 + 0.5);
    out.fade = (1.0 - t) * smoothstep(0.0, 0.15, t + 0.05);
    return out;
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex0, samp, in.uv);
    let a = texel.a * in.fade;
    if (a < 0.01) {
        discard;
    }
    let color = texel.rgb * 1.4 / (1.0 + texel.rgb);
    let white = uniforms.time.w;
    return vec4<f32>(mix(color, vec3<f32>(1.0), white), a);
}

// The filed nozzle meshes carry the heat effect. Their vertices undulate in
// object space as in flames.vp30, anchored to the animated jetpack.
@vertex
fn vs_flame(in: MeshIn) -> MeshOut {
    var out = mesh_vertex(in);
    let time = uniforms.time.x;
    let p = in.position;
    let t = 0.07 + 0.37 * clamp(1.0 + p.y * 0.01, 0.0, 1.0);
    let phase = 0.21 * (sin(17.3412 * time) + sin(39.4123423 * time));
    let a = vec3<f32>(
        30.0 * sin(time * 5.2197 + 0.4 + 0.2 * dot(p, vec3<f32>(0.013, 0.085, -0.013)) + phase),
        8.0 * sin(time * 4.3957 + 1.1 + dot(p, vec3<f32>(0.010, -0.065, 0.015)) + phase),
        30.0 * sin(time * 6.0288 + 3.8 + 0.2 * dot(p, vec3<f32>(-0.012, 0.095, -0.012)) + phase)
    );
    let b = vec3<f32>(
        15.0 * sin(time * 10.5791 + 3.1 + 0.5 * dot(p, vec3<f32>(0.015, 0.050, -0.016)) + phase),
        12.0 * sin(time * 11.6259 + 4.9 + 2.5 * dot(p, vec3<f32>(0.014, -0.085, 0.011)) + phase),
        15.0 * sin(time * 10.0971 + 5.5 + 0.5 * dot(p, vec3<f32>(-0.011, 0.070, -0.019)) + phase)
    );
    out.clip = uniforms.projection * uniforms.view * bones[0] * vec4<f32>(p + t * (a + b), 1.0);
    return out;
}

fn highlights(uv: vec2<f32>) -> vec3<f32> {
    return max(textureSample(tex0, samp, uv).rgb - vec3<f32>(0.6), vec3<f32>(0.0));
}

@fragment
fn fs_composite(in: CardOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let pixel = 1.0 / uniforms.time.yz;
    let frame = floor(uniforms.time.x * 60.0);
    let noise_hf = textureSample(tex2, samp, fract(1.863 * uv + frame * vec2<f32>(0.723, 0.381))).rgb * 2.0 - 1.0;
    let noise_lf = textureSample(tex2, samp, fract(0.296 * uv + frame * vec2<f32>(0.327, 0.169))).rgb * 2.0 - 1.0;
    let crisp = textureSample(tex0, samp, uv);
    var main = crisp.rgb;
    var hilite = vec3<f32>(0.0);
    var heat = 0.0;
    let taps = array<vec2<f32>, 8>(
        vec2<f32>(0.7, 0.3), vec2<f32>(0.3, -0.7), vec2<f32>(-0.7, -0.3), vec2<f32>(-0.3, 0.7),
        vec2<f32>(0.4, 1.0), vec2<f32>(-1.0, 0.4), vec2<f32>(-0.4, -1.0), vec2<f32>(1.0, -0.4)
    );
    for (var i = 0u; i < 8u; i++) {
        hilite += highlights(uv + pixel * 6.0 * taps[i]);
    }
    if (uniforms.effects.w > 0.5) {
        let focus = uniforms.lights[14].x;
        let depth = crisp.a * 800.0;
        let circle = clamp(abs(depth - focus) / max(depth, 48.0) * 3.0 - 0.55, 0.0, 5.0);
        if (circle > 0.05) {
            var sum = main;
            var amount = 1.0;
            for (var i = 0u; i < 8u; i++) {
                let tap = textureSampleLevel(tex0, samp, uv + pixel * circle * taps[i], 0.0);
                // Keep sharp foreground silhouettes from bleeding into the far layer.
                let weight = 1.0 - smoothstep(0.045, 0.11, abs(tap.a - crisp.a));
                sum += tap.rgb * weight;
                amount += weight;
            }
            main = sum / amount;
        }
    }
    if (uniforms.lights[11].x > 0.0) {
        heat = textureSample(tex1, samp, uv).r;
        for (var i = 0u; i < 8u; i++) {
            heat += textureSample(tex1, samp, uv + pixel * 6.0 * taps[i]).r;
        }
        heat /= 9.0;
        let refract_uv = uv + (0.004425 * noise_hf.xy + 0.00935 * noise_lf.xy) * heat;
        var hot = textureSample(tex0, samp, refract_uv).rgb;
        for (var i = 0u; i < 8u; i++) {
            hot += textureSample(tex0, samp, refract_uv + pixel * 2.8 * taps[i]).rgb;
        }
        hot *= (1.03 + 0.06 * noise_hf.z + 0.22 * noise_lf.z) / 9.0;
        main = mix(main, hot, clamp(heat * 4.0, 0.0, 1.0));
    }
    let flame = 0.8 * (clamp(heat * 1.3, 0.0, 1.0) * vec3<f32>(0.25, 0.12, 0.02)
        + heat * heat * vec3<f32>(0.5, 0.4, 0.3));
    let color = main + hilite * 0.10 + flame;
    return vec4<f32>(mix(color, vec3<f32>((color.r + color.g + color.b) / 3.0), 0.22), 1.0);
}


struct EffectIn {
    @location(0) tail: vec3<f32>,
    @location(1) to: vec3<f32>,
    @location(2) style: vec3<f32>,
};

struct EffectOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) style: vec2<f32>,
};

fn effect_corner(i: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0)
    );
    return corners[i];
}

@vertex
fn vs_smoke(in: EffectIn, @builtin(vertex_index) i: u32) -> EffectOut {
    let corner = effect_corner(i);
    let p = uniforms.view * vec4<f32>(in.to, 1.0);
    var out: EffectOut;
    out.clip = uniforms.projection * (p + vec4<f32>(corner * in.style.x, 0.0, 0.0));
    out.uv = corner * 0.5 + vec2<f32>(0.5);
    out.style = in.style.yz;
    return out;
}

@fragment
fn fs_smoke(in: EffectOut) -> @location(0) vec4<f32> {
    let smoke = textureSample(tex0, samp, in.uv);
    let falloff = clamp(1.0 - dot(in.uv - 0.5, in.uv - 0.5) * 4.0, 0.0, 1.0);
    let opacity = max(smoke.a * smoke.r, smoke.r * 0.45) * falloff * in.style.x;
    let tint = mix(vec3<f32>(1.0, 0.64, 0.31), vec3<f32>(0.54, 0.58, 0.67), smoothstep(0.0, 0.38, in.style.y));
    return vec4<f32>(tint, opacity);
}

@vertex
fn vs_streak(in: EffectIn, @builtin(vertex_index) i: u32) -> EffectOut {
    let corner = effect_corner(i);
    let a = uniforms.view * vec4<f32>(in.tail, 1.0);
    let b = uniforms.view * vec4<f32>(in.to, 1.0);
    let d = b.xy - a.xy;
    let width = max(in.style.x, 0.0018 * -b.z);
    let axis = d * inverseSqrt(max(dot(d, d), 0.0001));
    let side = vec2<f32>(-axis.y, axis.x);
    let p = mix(a, b + vec4<f32>(axis * width, 0.0, 0.0), corner.y * 0.5 + 0.5);
    var out: EffectOut;
    out.clip = uniforms.projection * (p + vec4<f32>(side * corner.x * width, 0.0, 0.0));
    out.uv = corner * 0.5 + vec2<f32>(0.5);
    out.style = in.style.yz;
    return out;
}

@fragment
fn fs_streak(in: EffectOut) -> @location(0) vec4<f32> {
    let taper = (1.0 - abs(in.uv.x * 2.0 - 1.0)) * in.uv.y * in.style.x;
    let color = mix(vec3<f32>(1.0, 0.39, 0.12), vec3<f32>(1.0, 0.90, 0.69), in.uv.y);
    return vec4<f32>(color * taper, 0.0);
}
