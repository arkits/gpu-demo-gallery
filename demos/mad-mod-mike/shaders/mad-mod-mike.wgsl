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
    // Four point lights: color and intensity, then position.
    lights: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<uniform> bones: array<mat4x4<f32>, 96>;

@group(1) @binding(0) var tex0: texture_2d<f32>;
@group(1) @binding(1) var tex1: texture_2d<f32>;
@group(1) @binding(2) var tex2: texture_2d<f32>;
@group(1) @binding(3) var tex3: texture_2d<f32>;
@group(1) @binding(4) var samp: sampler;

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
        return vec4<f32>(position, 1.0);
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
        return normal;
    }
    return n;
}

@vertex
fn vs_main(in: MeshIn) -> MeshOut {
    let weights = array<f32, 8>(
        in.weights0.x, in.weights0.y, in.weights0.z, in.weights0.w,
        in.weights1.x, in.weights1.y, in.weights1.z, in.weights1.w,
    );
    let joints = array<u32, 8>(
        u32(in.bones0.x), u32(in.bones0.y), u32(in.bones0.z), u32(in.bones0.w),
        u32(in.bones1.x), u32(in.bones1.y), u32(in.bones1.z), u32(in.bones1.w),
    );
    let world = skin(weights, joints, in.position, in.normal.xyz);
    let nrm = normalize(skin_normal(weights, joints, in.normal.xyz));
    var out: MeshOut;
    out.clip = uniforms.projection * uniforms.view * world;
    out.world = world.xyz;
    out.normal = nrm;
    out.uv = in.uv;
    out.tangent = in.tangent;
    return out;
}

fn light_rig(base: vec3<f32>, normal: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    var color = base * 0.55;
    for (var i = 0u; i < 4u; i++) {
        let light_color = uniforms.lights[i * 2u].rgb * uniforms.lights[i * 2u].a;
        let to_light = uniforms.lights[i * 2u + 1u].xyz - world;
        let dist = length(to_light);
        let amount = max(dot(normal, to_light / max(dist, 0.001)), 0.0);
        color += base * light_color * amount / (1.0 + dist * dist * 0.000002);
    }
    return color;
}

@fragment
fn fs_main(in: MeshOut) -> @location(0) vec4<f32> {
    let diffuse = textureSample(tex0, samp, in.uv);
    var normal = normalize(in.normal);
    let bump = textureSample(tex1, samp, in.uv).rgb * 2.0 - 1.0;
    if (dot(bump, bump) > 0.01) {
        let tangent = normalize(in.tangent.xyz);
        let binormal = cross(normal, tangent) * in.tangent.w;
        normal = normalize(tangent * bump.x + binormal * bump.y + normal * bump.z);
    }
    var color = light_rig(diffuse.rgb, normal, in.world);
    let view_dir = normalize(uniforms.eye.xyz - in.world);
    let half_dir = normalize(view_dir + vec3<f32>(0.4, 0.8, 0.45));
    let spec = pow(max(dot(normal, half_dir), 0.0), 24.0);
    color += vec3<f32>(spec * 0.35 * diffuse.a);
    color = color / (1.0 + color);
    let white = uniforms.time.w;
    color = mix(color, vec3<f32>(1.0), white);
    return vec4<f32>(color, diffuse.a);
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
    let p = select(in.a_position, in.b_position, vertex == 1u || vertex == 3u || vertex == 5u);
    let tangent = normalize(select(in.a_tangent, in.b_tangent, vertex > 2u));
    let sway = sin(uniforms.time.x * 1.7 + p.x * 0.11 + p.y * 0.13) * 0.35;
    var moved = p + vec3<f32>(sway, sway * 0.3, 0.0);
    let eye = (uniforms.view * vec4<f32>(moved, 1.0)).xyz;
    let side = normalize(cross(tangent, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(0.001, 0.0, 0.0));
    let width = 0.22;
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let c = corner[vertex];
    let across = (uniforms.view * vec4<f32>(side, 0.0)).xy;
    let at = select(0.0, 1.0, vertex > 2u);
    moved = mix(in.a_position, in.b_position, at) + vec3<f32>(sway, sway * 0.3, 0.0);
    var out: HairOut;
    let clip = uniforms.projection * uniforms.view * vec4<f32>(moved, 1.0);
    out.clip = clip + vec4<f32>(across * c.x * width * 0.01 * clip.w, 0.0, 0.0);
    let diffuse = abs(dot(tangent, normalize(vec3<f32>(0.4, 0.8, 0.45))));
    out.shade = 0.25 + 0.75 * diffuse;
    out.along = mix(in.a_color.y, in.b_color.y, at);
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
