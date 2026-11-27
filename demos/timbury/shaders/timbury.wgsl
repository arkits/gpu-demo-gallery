// Timbury's Cg programs, translated by hand: bumped environment lighting,
// hair, grass, sky, and the blur/glow composite.

struct Uniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    eye: vec4<f32>,
    time: vec4<f32>,
    look: vec4<f32>,
    size: vec4<f32>,
    light: vec4<f32>,
    fade: vec4<f32>,
}
struct Bones {
    m: array<mat4x4<f32>, 96>,
}
struct Params {
    spec: vec4<f32>,
    misc: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<uniform> b: Bones;
@group(0) @binding(2) var<uniform> p: Params;

@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var t2: texture_2d<f32>;
@group(1) @binding(3) var c0: texture_cube<f32>;
@group(1) @binding(4) var c1: texture_cube<f32>;
@group(1) @binding(5) var smp: sampler;

struct Vary {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) tan: vec3<f32>,
    @location(2) bin: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) eye: vec3<f32>,
    @location(5) occ: f32,
}

@vertex
fn vs(
    @location(0) pos: vec3<f32>,
    @location(1) nrm_occ: vec4<f32>,
    @location(2) tan: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) bw: vec4<f32>,
    @location(5) bi: vec4<f32>,
) -> Vary {
    let w0 = bw.x;
    let w1 = bw.y;
    let w2 = bw.z;
    let w3 = bw.w;
    let m = b.m[u32(bi.x)] * w0 + b.m[u32(bi.y)] * w1 + b.m[u32(bi.z)] * w2 + b.m[u32(bi.w)] * w3;
    let wp = m * vec4<f32>(pos, 1.0);
    var v: Vary;
    v.pos = u.proj * u.view * wp;
    v.nrm = (m * vec4<f32>(nrm_occ.xyz, 0.0)).xyz;
    v.tan = (m * vec4<f32>(tan.xyz, 0.0)).xyz;
    v.bin = cross(v.nrm, v.tan) * tan.w;
    v.uv = uv;
    v.eye = u.eye.xyz - wp.xyz;
    v.occ = nrm_occ.w;
    return v;
}

// bumpedEnv2Tex_NO_SHADOW.fp30, in outline: the diffuse and specular cubes
// light the bumped normal; the color map's alpha occludes.
@fragment
fn fs_lit(v: Vary) -> @location(0) vec4<f32> {
    let albedo = textureSample(t0, smp, v.uv);
    let bumpspec = textureSample(t1, smp, v.uv);
    let n = normalize(v.nrm) + p.misc.z * (bumpspec.x * normalize(v.tan) + bumpspec.y * normalize(v.bin));
    let wn = normalize(n);
    let diffuse = textureSample(c0, smp, wn).xyz;
    let refl = reflect(normalize(-v.eye), wn);
    let specular = textureSample(c1, smp, refl).xyz;
    let ambient = vec3<f32>(p.misc.x);
    let col = (diffuse + ambient) * albedo.xyz * 0.46 + specular * p.spec.w * p.spec.xyz;
    return vec4<f32>(col * (0.5 + 0.5 * v.occ), 1.0);
}

// glassLenses.fp30, in outline: a dim tinted reflection.
@fragment
fn fs_glass(v: Vary) -> @location(0) vec4<f32> {
    let albedo = textureSample(t0, smp, v.uv);
    let wn = normalize(v.nrm);
    let refl = reflect(normalize(-v.eye), wn);
    let specular = textureSample(c1, smp, refl).xyz;
    return vec4<f32>(albedo.xyz * 0.3 + specular * 0.5, 0.45);
}

// lambert1.fp30, in outline: one directional light and ambient.
@fragment
fn fs_lambert(v: Vary) -> @location(0) vec4<f32> {
    let albedo = textureSample(t0, smp, v.uv);
    let ndl = max(dot(normalize(v.nrm), -u.light.xyz), 0.0);
    return vec4<f32>(albedo.xyz * (0.08 + 0.9 * ndl), 1.0);
}

// _texture.fp30 and _bkg_split.fp30: the picture, brightened for the sky.
@fragment
fn fs_flat(v: Vary) -> @location(0) vec4<f32> {
    let tex = textureSample(t0, smp, v.uv);
    return vec4<f32>(0.03496 + 0.368 * tex.xyz, 1.0);
}

// hair.fp30, in outline: diffuse from the cube, anisotropic streaks.
@fragment
fn fs_hair(v: HairVary) -> @location(0) vec4<f32> {
    let albedo = textureSample(t0, smp, v.uv);
    let wn = normalize(v.nrm);
    let diffuse = textureSample(c0, smp, wn).xyz;
    let aniso = textureSample(t1, smp, v.uv);
    let col = (diffuse * aniso.xyz + vec3<f32>(0.08)) * albedo.xyz
        + aniso.w * vec3<f32>(0.06, 0.017, 0.009);
    return vec4<f32>(col, p.misc.w);
}

struct HairVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) eye: vec3<f32>,
}

// Lines: each instance is a segment; the shader widens it into a quad [I].
// Width is a fraction of the window height, like the original's lines.
@vertex
fn vs_hair(
    @builtin(vertex_index) vi: u32,
    @location(0) p0: vec3<f32>,
    @location(1) t0: vec3<f32>,
    @location(2) n0: vec3<f32>,
    @location(3) c0: vec2<f32>,
    @location(4) p1: vec3<f32>,
    @location(5) t1: vec3<f32>,
    @location(6) n1: vec3<f32>,
    @location(7) c1: vec2<f32>,
) -> HairVary {
    let second = vi == 4u || vi == 5u;
    let base = select(p0, p1, second);
    let nrm = select(n0, n1, second);
    let col = select(c0, c1, second);
    // Two triangles of a quad across the segment: (A-, A+, B-) and (A+, B+, B-).
    let corner = vi % 3u;
    let side = select(-1.0, 1.0, corner == 1u || vi == 3u || vi == 4u);
    let wp = (b.m[0] * vec4<f32>(base, 1.0)).xyz;
    let c0p = u.proj * u.view * vec4<f32>((b.m[0] * vec4<f32>(p0, 1.0)).xyz, 1.0);
    let c1p = u.proj * u.view * vec4<f32>((b.m[0] * vec4<f32>(p1, 1.0)).xyz, 1.0);
    let n0p = c0p.xy / c0p.w;
    let n1p = c1p.xy / c1p.w;
    var perp = normalize(vec2<f32>(-(n1p.y - n0p.y), n1p.x - n0p.x));
    if (!all(perp == vec2<f32>(0.0))) {
        perp = normalize(perp);
    } else {
        perp = vec2<f32>(1.0, 0.0);
    }
    let half_w = 0.9 / u.size.y;
    let base_clip = select(c0p, c1p, second);
    let clip = vec4<f32>(base_clip.xy + perp * side * half_w * base_clip.w, base_clip.zw);
    var v: HairVary;
    v.pos = clip;
    v.nrm = nrm;
    v.uv = col;
    v.eye = u.eye.xyz - wp;
    return v;
}

// grass.vp30/fp30, in outline: tapered blades swaying in the wind [I].
struct GrassVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) shade: f32,
}

@vertex
fn vs_grass(
    @location(0) root: vec3<f32>,
    @location(1) dir: vec2<f32>,
    @location(2) tall: f32,
    @location(3) shape: vec2<f32>,
    @location(4) phase: f32,
    @location(5) color: vec3<f32>,
) -> GrassVary {
    let t = shape.x;
    let side = shape.y * 2.0 - 1.0;
    let sway = sin(u.time.x * 1.7 + phase) * 0.12 * t;
    // Cylindrical billboard: face the camera around the stalk [I].
    let to_eye = u.eye.xyz - root;
    let across = cross(vec3<f32>(0.0, 1.0, 0.0), to_eye);
    let right = across / max(length(across), 0.001);
    let width = 1.6 * (1.0 - t * 0.8);
    var wp = root + vec3<f32>(0.0, 1.0, 0.0) * tall * t;
    wp += right * side * width + vec3<f32>(sway, 0.0, sway * 0.6);
    var v: GrassVary;
    v.pos = u.proj * u.view * vec4<f32>(wp, 1.0);
    v.uv = vec2<f32>(shape.y, 1.0 - t);
    v.color = color;
    v.shade = 0.6 + 0.4 * t;
    return v;
}

@fragment
fn fs_grass(v: GrassVary) -> @location(0) vec4<f32> {
    let tex = textureSample(t0, smp, v.uv);
    return vec4<f32>(v.color * tex.xyz * v.shade * 0.29, 1.0);
}

// The menu overlay: textured quads in fractions of the window.
struct MenuVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_menu(@location(0) xy: vec2<f32>, @location(1) uv: vec2<f32>) -> MenuVary {
    var v: MenuVary;
    v.pos = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    v.uv = uv;
    return v;
}

@group(0) @binding(0) var menu_tex: texture_2d<f32>;
@group(0) @binding(1) var menu_smp: sampler;

@fragment
fn fs_menu(v: MenuVary) -> @location(0) vec4<f32> {
    return textureSample(menu_tex, menu_smp, v.uv);
}

struct FullVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Fullscreen triangle: texture row 0 is the top of the picture, so v=1
// is at NDC y=-1 for an upright copy [I].
@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullVary {
    var v: FullVary;
    let corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array<vec2<f32>, 3>(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    v.uv = uvs[vi];
    v.pos = vec4<f32>(corners[vi], 0.0, 1.0);
    return v;
}

@group(0) @binding(0) var post0: texture_2d<f32>;
@group(0) @binding(1) var post1: texture_2d<f32>;
@group(0) @binding(2) var<uniform> post: vec4<f32>;
@group(0) @binding(3) var post_smp: sampler;

// blur_xy.fp30, in outline: a small blur which also downsamples [I].
@fragment
fn fs_blur(v: FullVary) -> @location(0) vec4<f32> {
    let px = vec2<f32>(1.0 / 256.0);
    var col = vec3<f32>(0.0);
    col += textureSample(post0, post_smp, v.uv + vec2<f32>(-px.x, 0.0)).xyz * 0.2;
    col += textureSample(post0, post_smp, v.uv + vec2<f32>(px.x, 0.0)).xyz * 0.2;
    col += textureSample(post0, post_smp, v.uv + vec2<f32>(0.0, -px.y)).xyz * 0.2;
    col += textureSample(post0, post_smp, v.uv + vec2<f32>(0.0, px.y)).xyz * 0.2;
    col += textureSample(post0, post_smp, v.uv).xyz * 0.2;
    return vec4<f32>(col, 1.0);
}

// composite.fp30, in outline: the scene softened with its blur,
// scaled by the light level and the output slider.
@fragment
fn fs_composite(v: FullVary) -> @location(0) vec4<f32> {
    let scene = textureSample(post0, post_smp, v.uv).xyz;
    let blurred = textureSample(post1, post_smp, v.uv).xyz;
    let softened = mix(scene, blurred, post.z * 0.5);
    return vec4<f32>(softened * post.w, 1.0);
}
