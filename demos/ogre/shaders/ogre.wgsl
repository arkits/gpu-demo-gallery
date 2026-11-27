//! Hand ports of the demo's Cg 1.1 programs (`colorBumpSpec`,
//! `colorBumpSpecOcc`, `colorBumpSpecDisplace`, `colorBumpDiffuse`,
//! `shadow`, `background`): tangent-space bump mapping with a phong
//! look-up, soft shadow-map tests and textured backdrops.

struct Globals {
    view_proj: mat4x4f,
    light_dir: vec4f,
    eye: vec4f,
    shadow_proj: mat4x4f,
    _pad0: vec4f,
    _pad1: vec4f,
};

struct Shape {
    model: mat4x4f,
    diffuse: vec4f,
    specular: vec4f,
    ambient: vec4f,
    shadow_ambient: vec2f,
    bump_scale: vec2f,
    displace: f32,
    flags: u32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> frame: Globals;
@group(0) @binding(1) var shadow_tex: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> shape: Shape;
@group(2) @binding(0) var diff_tex: texture_2d<f32>;
@group(2) @binding(1) var bump_tex: texture_2d<f32>;
@group(2) @binding(2) var phong_tex: texture_2d<f32>;
@group(2) @binding(3) var material_sampler: sampler;

struct BumpIn {
    @location(0) pos: vec3f,
    @location(1) normal: vec3f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
    @location(4) occ: f32,
};

struct BumpOut {
    @builtin(position) clip: vec4f,
    @location(0) light_dir: vec3f,
    @location(1) half_angle: vec3f,
    @location(2) uv: vec2f,
    @location(3) shadow: vec4f,
    @location(4) occ: f32,
};

@vertex
fn vs_bump(in: BumpIn) -> BumpOut {
    var out: BumpOut;
    var p = in.pos + shape.displace * in.normal;
    let world = shape.model * vec4f(p, 1.0);
    let n = normalize((shape.model * vec4f(in.normal, 0.0)).xyz);
    var t = normalize((shape.model * vec4f(in.tangent.xyz, 0.0)).xyz);
    let b = in.tangent.w * cross(n, t);
    t = t * shape.bump_scale.x;
    let bb = b * shape.bump_scale.y;
    out.light_dir = vec3f(dot(t, frame.light_dir.xyz), dot(bb, frame.light_dir.xyz), dot(n, frame.light_dir.xyz));
    let eye_vec = normalize(frame.eye.xyz - world.xyz);
    let h = normalize(frame.light_dir.xyz + eye_vec);
    out.half_angle = vec3f(dot(t, h), dot(bb, h), dot(n, h));
    out.uv = in.uv;
    out.shadow = frame.shadow_proj * world;
    out.occ = in.occ;
    out.clip = frame.view_proj * world;
    return out;
}

fn shadowed(coord: vec4f) -> f32 {
    let projected = coord.xyz / coord.w;
    let uv = projected.xy * vec2f(0.5, -0.5) + vec2f(0.5, 0.5);
    return textureSampleCompare(shadow_tex, shadow_sampler, uv, projected.z);
}

@fragment
fn fs_bump(in: BumpOut) -> @location(0) vec4f {
    let n = normalize(textureSample(bump_tex, material_sampler, in.uv).xyz * 2.0 - vec3f(1.0));
    let l = normalize(in.light_dir);
    let h = normalize(in.half_angle);
    let lighting = textureSample(phong_tex, material_sampler, vec2f(dot(n, l), dot(n, h)));
    let color_map = textureSample(diff_tex, material_sampler, in.uv);
    let shadow = shadowed(in.shadow);
    let shadow_factor = shape.shadow_ambient.x + shape.shadow_ambient.y * shadow;
    var col = shape.ambient.xyz * color_map.xyz
        + lighting.xyz * color_map.xyz * shape.diffuse.xyz * shadow_factor
        + lighting.w * color_map.w * shape.specular.xyz * shadow;
    if ((shape.flags & 1u) != 0u && color_map.w < 0.5) {
        discard;
    }
    return vec4f(col, 1.0);
}

@fragment
fn fs_bump_occ(in: BumpOut) -> @location(0) vec4f {
    let n = normalize(textureSample(bump_tex, material_sampler, in.uv).xyz * 2.0 - vec3f(1.0));
    let l = normalize(in.light_dir);
    let h = normalize(in.half_angle);
    let lighting = textureSample(phong_tex, material_sampler, vec2f(dot(n, l), dot(n, h)));
    let color_map = textureSample(diff_tex, material_sampler, in.uv);
    let shadow = shadowed(in.shadow);
    let shadow_factor = shape.shadow_ambient.x + shape.shadow_ambient.y * shadow;
    var col = vec3f(in.occ) * shape.ambient.xyz * color_map.xyz
        + lighting.xyz * color_map.xyz * shape.diffuse.xyz * shadow_factor
        + lighting.w * color_map.w * shape.specular.xyz * shadow;
    return vec4f(col, 1.0);
}

@fragment
fn fs_bump_diffuse(in: BumpOut) -> @location(0) vec4f {
    let n = normalize(textureSample(bump_tex, material_sampler, in.uv).xyz * 2.0 - vec3f(1.0));
    let l = normalize(in.light_dir);
    let color_map = textureSample(diff_tex, material_sampler, in.uv);
    let shadow = shadowed(in.shadow);
    let shadow_factor = shape.shadow_ambient.x + shape.shadow_ambient.y * shadow;
    let diffuse = max(dot(n, l), 0.0);
    var col = shape.ambient.xyz * color_map.xyz
        + diffuse * color_map.xyz * shape.diffuse.xyz * shadow_factor;
    if ((shape.flags & 1u) != 0u && color_map.w < 0.5) {
        discard;
    }
    return vec4f(col, color_map.w);
}

@vertex
fn vs_depth(in: BumpIn) -> @builtin(position) vec4f {
    return frame.shadow_proj * shape.model * vec4f(in.pos, 1.0);
}

struct GroundOut {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
    @location(1) s0: vec4f,
    @location(2) s1: vec4f,
    @location(3) s2: vec4f,
    @location(4) s3: vec4f,
};

@vertex
fn vs_ground(in: BumpIn) -> GroundOut {
    var out: GroundOut;
    let world = shape.model * vec4f(in.pos, 1.0);
    let shadow = frame.shadow_proj * world;
    let d = (4.0 / 1024.0) * shadow.w;
    out.s0 = shadow + vec4f(-0.5, -1.0, 0.0, 0.0) * d;
    out.s1 = shadow + vec4f(1.0, -0.5, 0.0, 0.0) * d;
    out.s2 = shadow + vec4f(0.5, 1.0, 0.0, 0.0) * d;
    out.s3 = shadow + vec4f(-1.0, 0.5, 0.0, 0.0) * d;
    out.uv = in.uv;
    out.clip = frame.view_proj * world;
    return out;
}

fn pcf(coord: vec4f) -> f32 {
    let projected = coord.xyz / coord.w;
    let uv = projected.xy * vec2f(0.5, -0.5) + vec2f(0.5, 0.5);
    return textureSampleCompare(shadow_tex, shadow_sampler, uv, projected.z);
}

@fragment
fn fs_ground(in: GroundOut) -> @location(0) vec4f {
    let s = 0.25 * (pcf(in.s0) + pcf(in.s1) + pcf(in.s2) + pcf(in.s3));
    let c = textureSample(diff_tex, material_sampler, in.uv);
    return vec4f(c.xyz * (0.6 + 0.4 * s), c.w);
}

struct BgOut {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_bg(in: BumpIn) -> BgOut {
    var out: BgOut;
    out.clip = frame.view_proj * shape.model * vec4f(in.pos, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4f {
    return textureSample(diff_tex, material_sampler, in.uv);
}
