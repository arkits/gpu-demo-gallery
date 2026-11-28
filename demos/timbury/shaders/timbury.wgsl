// Timbury's Cg programs (`shaders/*.fp30`, `*.vp30`), translated line by line
// where the scenes use them: the environment-lit materials and their shadow
// lookups, the refracting lenses, hair, grass, sky, the shadow casters, and
// the HDR blur / light-level / composite chain from `blur.nma`.
//
// Conventions: world space is the file's (Maya, y up). Render targets are
// sampled with v flipped from the original's OpenGL coordinates
// (`gl_v -> 1 - gl_v`); pictures loaded from files keep their rows as the
// original uploads them, so their UVs are used as filed.

struct Uniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    eye: vec4<f32>,
    time: vec4<f32>,
    look: vec4<f32>,
    size: vec4<f32>,
    light: vec4<f32>,
    fade: vec4<f32>,
    // viewProjWin_Target0: the ShadowRenderTargetProxy camera's view-proj.
    shadow: mat4x4<f32>,
    // g_timLookDir / g_timSideDir / g_timUpDir from LensCamera (0x41a870).
    tim_look: vec4<f32>,
    tim_side: vec4<f32>,
    tim_up: vec4<f32>,
    // x g_bitten, y bkg g_brightnessScale, z g_grassBrightScale.
    runtime: vec4<f32>,
}
struct Bones {
    m: array<mat4x4<f32>, 96>,
}
// Each shape's filed ConstantParams, with the Cg defaults where none is filed.
struct Params {
    diffuse: vec4<f32>,  // diffuseColor, specularScale
    ambient: vec4<f32>,  // ambientColor, bump_scale
    light: vec4<f32>,    // worldShadowCameraVec, shadow_str
    spec: vec4<f32>,     // specularColor, transparency
    extra: vec4<f32>,    // rim_ndotv_exp_mult, rim_color_scale, hard_shadow_inv_texsize, modulate_by
    more: vec4<f32>,     // flipUCoord, _blur2D invTexSize.xy, blurAxis packed in .w (0 x, 1 y)
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<uniform> b: Bones;
@group(0) @binding(2) var<uniform> p: Params;

// Per material: colorTex, bump, specularTex, specialTex, rampTex and the
// diffuse / specular environment cubes.
@group(1) @binding(0) var t_color: texture_2d<f32>;
@group(1) @binding(1) var t_bump: texture_2d<f32>;
@group(1) @binding(2) var t_spec: texture_2d<f32>;
@group(1) @binding(3) var t_special: texture_2d<f32>;
@group(1) @binding(4) var t_ramp: texture_2d<f32>;
@group(1) @binding(5) var c_diffuse: texture_cube<f32>;
@group(1) @binding(6) var c_specular: texture_cube<f32>;
@group(1) @binding(7) var smp: sampler;

// Shared by the scene passes: the hard shadow depth map, the blurred tree
// shadow, the lens render target and the lens reflection cube.
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_cmp: sampler_comparison;
@group(2) @binding(2) var soft_tree: texture_2d<f32>;
@group(2) @binding(3) var lens_scene: texture_2d<f32>;
@group(2) @binding(4) var c_reflection: texture_cube<f32>;
@group(2) @binding(5) var clamp_smp: sampler;

struct Vary {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) tan: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world: vec3<f32>,
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
    let m = b.m[u32(bi.x)] * bw.x + b.m[u32(bi.y)] * bw.y + b.m[u32(bi.z)] * bw.z + b.m[u32(bi.w)] * bw.w;
    let wp = m * vec4<f32>(pos, 1.0);
    var v: Vary;
    v.pos = u.proj * u.view * wp;
    v.nrm = (m * vec4<f32>(nrm_occ.xyz, 0.0)).xyz;
    v.tan = (m * vec4<f32>(tan.xyz, 0.0)).xyz;
    v.uv = uv;
    v.world = wp.xyz / wp.w;
    return v;
}

// --- shared pieces of the material programs -------------------------------

struct Frame3 {
    n: vec3<f32>,     // normalize(worldNormal)
    t: vec3<f32>,     // normalize(worldTangent)
    bn: vec3<f32>,    // normalize(worldBinormal) = crossNormalize(T, N)
    eye_dir: vec3<f32>, // computeEyeDir: from the eye to the point
}

fn frame3(v: Vary) -> Frame3 {
    var f: Frame3;
    f.n = normalize(v.nrm);
    f.t = normalize(v.tan);
    f.bn = normalize(cross(v.tan, v.nrm));
    f.eye_dir = normalize(v.world - u.eye.xyz);
    return f;
}

// "bump" samples arrive in [-1, 1] (signed texture), as the programs expect.
fn bumped(f: Frame3, bump: vec2<f32>, scale: f32) -> vec3<f32> {
    return normalize(f.n + scale * (bump.x * f.t + bump.y * f.bn));
}

// viewProjWin_Target0 * worldCoord, divided: xy in texture space (v down),
// z the shadow camera's window depth.
fn shadow_coord(world: vec3<f32>) -> vec3<f32> {
    let c = u.shadow * vec4<f32>(world, 1.0);
    let n = c.xyz / c.w;
    return vec3<f32>(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5, n.z);
}

// h1texcompare2D: 1 where lit. Outside the map counts as lit.
fn hard1(s: vec3<f32>) -> f32 {
    return textureSampleCompareLevel(shadow_map, shadow_cmp, s.xy, s.z);
}

// The 5-tap hard shadow of the _SKIN, _CLOTHES and _BITE programs. The
// offsets are in the original's (v up) texel space.
fn hard5(s: vec3<f32>) -> f32 {
    let d = vec2<f32>(p.extra.z, -p.extra.z);
    return 0.2 * (hard1(s)
        + hard1(vec3<f32>(s.xy + d * vec2<f32>(-0.5, -1.5), s.z))
        + hard1(vec3<f32>(s.xy + d * vec2<f32>(-1.5, 0.5), s.z))
        + hard1(vec3<f32>(s.xy + d * vec2<f32>(0.5, 1.5), s.z))
        + hard1(vec3<f32>(s.xy + d * vec2<f32>(1.5, -0.5), s.z)));
}

// The soft tree shadow: blurred coverage, clipped where the occluder lies
// behind the point.
fn soft_clip(s: vec3<f32>) -> f32 {
    let soft3 = textureSampleLevel(soft_tree, clamp_smp, s.xy, 0.0).xyz;
    return saturate(soft3.x + saturate((soft3.z - s.z) * 99.0));
}

// env0/1/2Tex read only the blurred coverage, unclipped.
fn soft_plain(s: vec3<f32>) -> f32 {
    return textureSampleLevel(soft_tree, clamp_smp, s.xy, 0.0).x;
}

fn darken(light_visible: f32) -> f32 {
    return p.light.w * light_visible + (1.0 - p.light.w);
}

// bumpedEnv2Tex.fp30: the bump map's alpha is the specular ramp, read back
// from its signed remap with frac(w * 0.5 + 1).
fn ramp(w: f32) -> f32 {
    return fract(w * 0.5 + 1.0);
}

// --- bumpedEnv2Tex family ----------------------------------------------------

// bumpedEnv2Tex.fp30: 1-tap hard shadow and the clipped tree shadow.
@fragment
fn fs_env2(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bumpspec = textureSample(t_bump, smp, v.uv);
    let n = bumped(f, bumpspec.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_clip(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * ramp(bumpspec.w);
    return vec4<f32>(col, 1.0);
}

// bumpedEnv2Tex_CLOTHES.fp30: 5-tap hard shadow, no tree shadow, specularColor.
@fragment
fn fs_env2_clothes(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bumpspec = textureSample(t_bump, smp, v.uv);
    let n = bumped(f, bumpspec.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard5(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * ramp(bumpspec.w) * p.spec.xyz;
    return vec4<f32>(col, 1.0);
}

// bumpedEnv2Tex_TREE.fp30: the tree casts the soft shadow, so it reads no
// shadow map, only the facing term.
@fragment
fn fs_env2_tree(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bumpspec = textureSample(t_bump, smp, v.uv);
    let n = bumped(f, bumpspec.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    diffuse *= darken(saturate(dot(n, p.light.xyz)));
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * ramp(bumpspec.w) * p.spec.xyz;
    return vec4<f32>(col, 1.0);
}

// bumpedEnv2Tex_BITE.fp30: the leaf with the bite; where the color map's
// alpha is nearly 1 the bite opens once g_bitten turns on (after 4.77 s).
@fragment
fn fs_env2_bite(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    var colormap = textureSample(t_color, smp, v.uv);
    let bumpspec = textureSample(t_bump, smp, v.uv);
    let n = bumped(f, bumpspec.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard5(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let bite_area = saturate(256.0 * (colormap.w - 0.996));
    let alpha = 1.0 - bite_area * u.runtime.x;
    colormap.w = colormap.w * (1.0 - bite_area) + bite_area * 0.955;
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * ramp(bumpspec.w);
    // The original leaves its alpha test at the default; the bite shows by
    // discarding the bitten texels [I].
    if (alpha < 0.5) {
        discard;
    }
    return vec4<f32>(col, alpha);
}

// bumpedEnv2Tex_NO_SHADOW.fp30: the swarm bugs.
@fragment
fn fs_env2_noshadow(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bumpspec = textureSample(t_bump, smp, v.uv);
    let n = bumped(f, bumpspec.xy, p.ambient.w);
    let diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let col = (diffuse + p.ambient.xyz) * colormap.xyz * 0.46
        + specular * p.diffuse.w * p.spec.xyz;
    return vec4<f32>(col, 1.0);
}

// --- bumpedEnv3Tex family (separate specular map) ----------------------------

// bumpedEnv3Tex.fp30
@fragment
fn fs_env3(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bump3 = textureSample(t_bump, smp, v.uv).xyz;
    let specmap = textureSample(t_spec, smp, v.uv);
    let n = bumped(f, bump3.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_clip(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * specmap.xyz;
    return vec4<f32>(col, 1.0);
}

// bumpedEnv3Tex_SKIN.fp30: 5-tap hard shadow, no tree shadow, a pink rim at
// the silhouette, and no occlusion term.
@fragment
fn fs_env3_skin(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bump3 = textureSample(t_bump, smp, v.uv).xyz;
    let specmap = textureSample(t_spec, smp, v.uv);
    let n = bumped(f, bump3.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let t = 1.0 - saturate(dot(f.n, -f.eye_dir));
    let pink = pow(t, 4.7 * p.extra.x) * 0.2 * p.extra.y;
    diffuse = vec3<f32>(0.98, 0.61, 0.77) * pink + (1.0 - pink) * diffuse;
    let s = shadow_coord(v.world);
    let lit = hard5(s) * saturate(-0.1 + 1.1 * dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let col = (p.diffuse.xyz * diffuse + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * specmap.xyz;
    return vec4<f32>(col, 1.0);
}

// bumpedEnv3Tex_BUG.fp30: the beetle's iridescent shell.
@fragment
fn fs_env3_bug(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bump3 = textureSample(t_bump, smp, v.uv).xyz;
    let specmap = textureSample(t_spec, smp, v.uv);
    let special = textureSample(t_special, smp, v.uv).xyz;
    var current = colormap.xyz * p.diffuse.xyz;
    let n = bumped(f, bump3.xy, p.ambient.w * 3.0);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let n_dot_v = dot(n, -f.eye_dir);
    let n_dot_l = dot(n, p.light.xyz);
    let n_dot_h = saturate(dot(n, normalize(n + p.light.xyz)));
    var t = 0.5 + 0.5 * sin(n_dot_v * 7.0 + n_dot_l * 3.0);
    t = t * t * (3.0 - 2.0 * t);
    t = t * t * (3.0 - 2.0 * t);
    let mixed = special.x * 0.85;
    current = current * (1.0 - mixed)
        + mixed * 0.69 * mix(vec3<f32>(0.6, 0.4, 0.1), vec3<f32>(0.85, 0.85, 0.3), t);
    let irid = vec3<f32>(1.0, 1.0, 0.0) * textureSample(t_ramp, smp, vec2<f32>(n_dot_h, 0.5)).xyz;
    let irid_mix = (1.0 - 0.25 * special.y) * (0.75 + 0.25 * special.x) * (1.0 - current.x);
    current = current * (1.0 - irid_mix) + irid_mix * vec3<f32>(0.99, 0.70, 0.70) * irid;
    current = current * 1.07 - 0.09;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (diffuse * occ + p.ambient.xyz) * current + specular * p.diffuse.w * specmap.xyz;
    return vec4<f32>(col, 1.0);
}

// bumpedEnv3Tex_transp.fp30: the color map's alpha is the opacity.
@fragment
fn fs_env3_transp(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let bump3 = textureSample(t_bump, smp, v.uv).xyz;
    let specmap = textureSample(t_spec, smp, v.uv);
    let n = bumped(f, bump3.xy, p.ambient.w);
    var diffuse = textureSample(c_diffuse, smp, n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_clip(s) * saturate(dot(n, p.light.xyz));
    diffuse *= darken(lit);
    let col = (p.diffuse.xyz * diffuse + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * specmap.xyz;
    return vec4<f32>(col, colormap.w);
}

// --- unbumped environment programs -----------------------------------------

// env0Tex.fp30: no pictures at all (the eye muscles).
@fragment
fn fs_env0(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    var diffuse = textureSample(c_diffuse, smp, f.n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, f.n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_plain(s) * saturate(dot(f.n, p.light.xyz));
    diffuse *= darken(lit);
    let col = p.diffuse.xyz * diffuse + p.ambient.xyz + specular * p.diffuse.w * p.spec.xyz;
    return vec4<f32>(col, 1.0);
}

// env1Tex_transp.fp30: lashes, brows, mustache; constant transparency.
@fragment
fn fs_env1_transp(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    var diffuse = textureSample(c_diffuse, smp, f.n).xyz;
    var specular = textureSample(c_specular, smp, reflect(f.eye_dir, f.n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_plain(s) * saturate(dot(f.n, p.light.xyz));
    let k = darken(lit);
    diffuse *= k;
    specular *= k;
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (diffuse * occ + p.ambient.xyz) * colormap.xyz + specular * p.diffuse.w * p.spec.xyz;
    return vec4<f32>(col, p.spec.w);
}

// env2Tex.fp30: the eyeballs (color and specular maps, no bump).
@fragment
fn fs_env2tex(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    let colormap = textureSample(t_color, smp, v.uv);
    let specmap = textureSample(t_spec, smp, v.uv);
    var diffuse = textureSample(c_diffuse, smp, f.n).xyz;
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, f.n)).xyz;
    let s = shadow_coord(v.world);
    let lit = hard1(s) * soft_plain(s) * saturate(dot(f.n, p.light.xyz));
    diffuse *= darken(lit);
    let occ = 0.2 + 0.8 * colormap.w;
    let col = (p.diffuse.xyz * diffuse * occ + p.ambient.xyz) * colormap.xyz
        + specular * p.diffuse.w * specmap.xyz;
    return vec4<f32>(col, 1.0);
}

// eyeballGloss.fp30: the wet cornea, drawn additively; its bump alpha is
// remapped with w * 0.5 + 0.5 and the frame is not renormalized.
@fragment
fn fs_gloss(v: Vary) -> @location(0) vec4<f32> {
    let f = frame3(v);
    var bumpspec = textureSample(t_bump, smp, v.uv);
    let w = bumpspec.w * 0.5 + 0.5;
    let n = normalize(f.n + p.ambient.w * (bumpspec.x * v.tan + bumpspec.y * cross(v.tan, v.nrm)));
    let specular = textureSample(c_specular, smp, reflect(f.eye_dir, n)).xyz;
    return vec4<f32>(specular * p.diffuse.w * w, 1.0);
}

// eyeballShadow.fp30: the lid-bits card, drawn multiplicatively.
@fragment
fn fs_shadow(v: Vary) -> @location(0) vec4<f32> {
    let bits = textureSample(t_color, smp, v.uv).xyz;
    return vec4<f32>(vec3<f32>(p.extra.w) + (1.0 - p.extra.w) * bits, 1.0);
}

// glassLenses.fp30 ("refaketion"): each lens shows the LensRenderTargetProxy
// picture of the eyes, panned and ballooned by the view direction relative
// to the head, under a fresnel reflection. It writes opaque.
@fragment
fn fs_glass(v: Vary) -> @location(0) vec4<f32> {
    let world_norm = normalize(v.nrm);
    let eye_dir = normalize(v.world - u.eye.xyz);
    let refl = reflect(eye_dir, world_norm);
    let lerp_refl = saturate(textureSample(c_reflection, clamp_smp, refl).xyz) * 1.5;
    let add_refl = dot(lerp_refl, vec3<f32>(0.35, 0.35, 0.3));
    let flip = p.more.x;
    var uu = v.uv.x;
    var vv = v.uv.y;
    let view_angle = 1.0 - dot(eye_dir, u.tim_look.xyz);
    let du = dot(-eye_dir, -u.tim_side.xyz);
    let dv = dot(-eye_dir, -u.tim_up.xyz);
    if (du * (flip * 2.0 - 1.0) < 0.0) {
        uu -= sign(du) * pow(abs(du), 2.0) * 0.8;
    } else {
        uu -= sign(du) * pow(abs(du), 2.971) * 1.529;
    }
    if (dv < 0.0) {
        vv -= dv * 0.6;
    } else {
        vv -= dv * 0.4;
    }
    uu -= 0.5;
    vv -= 0.35;
    let rad_squared = (uu * uu + vv * vv) * 2.0;
    var balloon = 0.8 + 0.8 * rad_squared;
    balloon = balloon * (1.0 - view_angle) + view_angle;
    uu = uu * balloon + 0.5;
    vv = vv * balloon + 0.35;
    uu = uu + (1.0 - flip) * (1.0 - 2.0 * uu);
    uu = 0.145 + 0.23 * uu;
    vv = 0.41 + 0.22 * vv;
    uu = uu + (1.0 - flip) * (1.0 - 2.0 * uu);
    let fresnel = 0.7 * pow(1.0 - saturate(dot(-eye_dir, world_norm)), 3.0);
    let add_amt = 0.07 + 0.11 * (1.0 - saturate(dot(world_norm, u.tim_look.xyz)));
    let v_up = vv * 2.0 - 0.5;
    let refracted = textureSampleLevel(lens_scene, clamp_smp, vec2<f32>(uu, 1.0 - v_up), 0.0).xyz;
    let col = lerp_refl * fresnel + (1.0 - fresnel) * (refracted * 0.8 + add_amt * add_refl);
    return vec4<f32>(col, 1.0);
}

// lambert1.fp30, in outline: one directional light and ambient.
@fragment
fn fs_lambert(v: Vary) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_color, smp, v.uv);
    let ndl = max(dot(normalize(v.nrm), -u.light.xyz), 0.0);
    return vec4<f32>(albedo.xyz * (0.08 + 0.9 * ndl), 1.0);
}

// _bkg_split.fp30: the sky faces, scaled by g_brightnessScale.
@fragment
fn fs_flat(v: Vary) -> @location(0) vec4<f32> {
    let tex = textureSample(t_color, smp, v.uv);
    return vec4<f32>(u.runtime.y * (0.03496 + 0.368 * tex.xyz), 1.0);
}

// --- shadow casters ----------------------------------------------------------

struct CastVary {
    @builtin(position) pos: vec4<f32>,
}

@vertex
fn vs_cast(
    @location(0) pos: vec3<f32>,
    @location(4) bw: vec4<f32>,
    @location(5) bi: vec4<f32>,
) -> CastVary {
    let m = b.m[u32(bi.x)] * bw.x + b.m[u32(bi.y)] * bw.y + b.m[u32(bi.z)] * bw.z + b.m[u32(bi.w)] * bw.w;
    var v: CastVary;
    v.pos = u.proj * u.view * (m * vec4<f32>(pos, 1.0));
    return v;
}

// shadows.fp30 for TreeShadowRenderTargetProxy: no coverage in red, the
// occluder's window depth in blue, over a white clear.
@fragment
fn fs_cast(v: CastVary) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, v.pos.z, 1.0);
}

// --- hair ----------------------------------------------------------------------

struct HairVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world: vec3<f32>,
}

// hair.fp30: diffuse from the cube darkened by the hard shadow, anisotropic
// streaks from anisoTex, a constant transparency.
@fragment
fn fs_hair(v: HairVary) -> @location(0) vec4<f32> {
    let colormap = textureSample(t_color, smp, v.uv);
    let wn = normalize(v.nrm);
    let aniso = textureSample(t_bump, smp, v.uv);
    var diffuse = textureSample(c_diffuse, smp, wn).xyz;
    let s = shadow_coord(v.world);
    diffuse *= darken(hard1(s) * saturate(dot(wn, p.light.xyz)));
    let col = (diffuse * aniso.xyz + p.ambient.xyz) * colormap.xyz
        + aniso.w * vec3<f32>(0.06, 0.017, 0.009);
    return vec4<f32>(col, p.spec.w);
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
    var perp = vec2<f32>(-(n1p.y - n0p.y), n1p.x - n0p.x);
    if (dot(perp, perp) > 0.0) {
        perp = normalize(perp);
    } else {
        perp = vec2<f32>(1.0, 0.0);
    }
    let half_w = 0.9 / u.size.y;
    let base_clip = select(c0p, c1p, second);
    let clip = vec4<f32>(base_clip.xy + perp * side * half_w * base_clip.w, base_clip.zw);
    var v: HairVary;
    v.pos = clip;
    v.nrm = (b.m[0] * vec4<f32>(nrm, 0.0)).xyz;
    v.uv = col;
    v.world = wp;
    return v;
}

// --- grass ---------------------------------------------------------------------

// grass.vp30 in outline (the blade geometry is the port's [I]); grass.fp30
// exactly: blade color times the picture, darkened by both shadows, times
// g_grassBrightScale.
struct GrassVary {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
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
    v.world = root;
    return v;
}

@fragment
fn fs_grass(v: GrassVary) -> @location(0) vec4<f32> {
    var diffuse = v.color * textureSample(t_color, smp, v.uv).xyz;
    let s = shadow_coord(v.world);
    diffuse *= darken(hard1(s) * soft_clip(s));
    return vec4<f32>(diffuse * u.runtime.z, 1.0);
}

// --- the menu overlay ---------------------------------------------------------

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

// --- fullscreen passes ----------------------------------------------------------

struct FullVary {
    @builtin(position) pos: vec4<f32>,
    // The original's (OpenGL) texture coordinate: v = 0 at the bottom.
    @location(0) tc: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullVary {
    var v: FullVary;
    let corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let c = corners[vi];
    v.pos = vec4<f32>(c, 0.0, 1.0);
    v.tc = c * 0.5 + 0.5;
    return v;
}

@group(0) @binding(0) var post0: texture_2d<f32>;
@group(0) @binding(1) var post1: texture_2d<f32>;
// x, y: scale (or invTexSize / blur axis); z, w: g_outer_and_inner_weights
// or the composite / light-level settings.
@group(0) @binding(2) var<uniform> post: vec4<f32>;
@group(0) @binding(3) var post_smp: sampler;

// Samples with the original's texture coordinates.
fn at0(tc: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(post0, post_smp, vec2<f32>(tc.x, 1.0 - tc.y), 0.0);
}
fn at1(tc: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(post1, post_smp, vec2<f32>(tc.x, 1.0 - tc.y), 0.0);
}

// blur_xy.fp30 with blur_xy.vp30's nine taps at thirds of `scale`
// (Initial2DBlurPass: the scene into the 256x128 PrimaryBlurY).
@fragment
fn fs_blur_xy(v: FullVary) -> @location(0) vec4<f32> {
    let s = post.xy;
    var sum = vec4<f32>(0.0);
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            sum += at0(v.tc + vec2<f32>(f32(i), f32(j)) * 0.3333 * s);
        }
    }
    return sum * 0.111111111;
}

// blur_05.fp30 / blur_05.vp30: taps at -2, -0.5 (twice), 1 and 2 times `scale`.
@fragment
fn fs_blur_05(v: FullVary) -> @location(0) vec4<f32> {
    let s = post.xy;
    return post.z * (at0(v.tc - 2.0 * s) + at0(v.tc + 2.0 * s))
        + post.w * (at0(v.tc - 0.5 * s) * 2.0 + at0(v.tc + 1.0 * s));
}

// blur_07.fp30 / blur_07.vp30: taps at -3, -2, -0.5, 1.5 and 3.
@fragment
fn fs_blur_07(v: FullVary) -> @location(0) vec4<f32> {
    let s = post.xy;
    return post.z * (at0(v.tc - 3.0 * s) + at0(v.tc + 3.0 * s))
        + post.w * (2.0 * (at0(v.tc - 2.0 * s) + at0(v.tc + 1.5 * s)) + at0(v.tc - 0.5 * s));
}

// _blur2D.fp30: the tree shadow blur; post.xy is invTexSize * blurAxis.
@fragment
fn fs_blur2d(v: FullVary) -> @location(0) vec4<f32> {
    let f = post.xy;
    var t: vec3<f32>;
    let c = v.tc;
    let v1 = at0(c + f * -3.0).xyz;
    let v2 = at0(c + f * -2.0).xyz;
    let v3 = at0(c + f * -1.0).xyz;
    let v4 = at0(c).xyz;
    let v5 = at0(c + f * 1.0).xyz;
    let v6 = at0(c + f * 2.0).xyz;
    let v7 = at0(c + f * 3.0).xyz;
    t.x = 0.17 * v2.x + 0.21 * v3.x + 0.24 * v4.x + 0.21 * v5.x + 0.17 * v6.x;
    t.y = v4.y;
    t.z = min(min(min(v1.z, v2.z), min(v3.z, v4.z)), min(min(v5.z, v6.z), v7.z));
    return vec4<f32>(t, 1.0);
}

// lightlev.fp30: the 256x128 blur averaged into 8x8 (16 taps each).
@fragment
fn fs_lightlev(v: FullVary) -> @location(0) vec4<f32> {
    let delta = vec2<f32>(0.25 / 8.0);
    let offset = v.tc * 255.0 / 256.0 + vec2<f32>(0.5 / 256.0);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        for (var j = 0; j < 4; j++) {
            sum += at0(offset + delta * vec2<f32>(f32(i), f32(j))).xyz;
        }
    }
    return vec4<f32>(sum * 0.0625, 1.0);
}

// The 2x2 exposure targets keep their values as log2(x) mapped to 0..1 so
// that they survive an 8-bit target when float ones are missing.
fn pack_scale(x: f32) -> f32 {
    return saturate((log2(max(x, 1e-6)) + 8.0) / 16.0);
}
fn unpack_scale(e: f32) -> f32 {
    return exp2(e * 16.0 - 8.0);
}

// lightlev2.fp30's sample(): luminance and its natural log, weight 1.
fn lev_sample(i: f32, j: f32) -> vec3<f32> {
    let tc = vec2<f32>(0.25, 0.25) * vec2<f32>(i, j) + vec2<f32>(0.0625);
    let x = dot(vec3<f32>(0.30, 0.48, 0.22), at0(tc).xyz);
    return vec3<f32>(x, log(max(x, 1e-6)), 1.0);
}

// lightlev2.fp30: the exposure (outputScale) from the 8x8 level, mixed over
// time with the previous frame's (post1). post.z is g_mixAmount.
@fragment
fn fs_lightlev2(v: FullVary) -> @location(0) vec4<f32> {
    var sum1 = vec3<f32>(0.0);
    var sum2 = vec3<f32>(0.0);
    sum1 += 0.6 * lev_sample(0.0, 0.0);
    sum2 += lev_sample(0.0, 1.0);
    sum1 += lev_sample(0.0, 2.0);
    sum2 += 0.6 * lev_sample(0.0, 3.0);
    sum1 += lev_sample(1.0, 0.0);
    sum2 += 1.7 * lev_sample(1.0, 1.0);
    sum1 += 1.7 * lev_sample(1.0, 2.0);
    sum2 += lev_sample(1.0, 3.0);
    sum1 += lev_sample(2.0, 0.0);
    sum2 += 1.7 * lev_sample(2.0, 1.0);
    sum1 += 1.7 * lev_sample(2.0, 2.0);
    sum2 += lev_sample(2.0, 3.0);
    sum1 += 0.6 * lev_sample(3.0, 0.0);
    sum2 += lev_sample(3.0, 1.0);
    sum1 += lev_sample(3.0, 2.0);
    sum2 += 0.6 * lev_sample(3.0, 3.0);
    sum1 += sum2;
    let packed = textureSampleLevel(post1, post_smp, vec2<f32>(0.25, 0.25), 0.0).xy;
    let prev = vec2<f32>(unpack_scale(packed.x), unpack_scale(packed.y));
    let mix_amount = post.z;
    let avg_lin = sum1.x / sum1.z;
    let avg_log = pow(2.0, sum1.y / sum1.z);
    var loglum = 1.0 / avg_log;
    if (prev.y > 0.001 && prev.y < 1000.0) {
        loglum = loglum * mix_amount + prev.y * (1.0 - mix_amount);
    }
    loglum = clamp(loglum, 0.04, 100.0);
    var scale = 0.75 / (avg_lin + 0.1);
    let exag = scale * scale / 3.65;
    scale = exag * 0.7 + scale * 0.3;
    if (prev.x > 0.001 && prev.x < 1000.0) {
        scale = scale * mix_amount + prev.x * (1.0 - mix_amount);
    }
    scale = clamp(scale, 0.04, 100.0);
    return vec4<f32>(pack_scale(scale), pack_scale(loglum), saturate(avg_lin), 1.0);
}

@group(0) @binding(4) var post2: texture_2d<f32>;

// composite.fp30: the scene softened with its blur, times the measured
// exposure (post2) and g_userOutputScale. post.z g_softness, post.w the
// output scale.
@fragment
fn fs_composite(v: FullVary) -> @location(0) vec4<f32> {
    let scene = at0(v.tc).xyz;
    let blurred = at1(v.tc).xyz;
    let auto_scale = unpack_scale(textureSampleLevel(post2, post_smp, vec2<f32>(0.25, 0.25), 0.0).x);
    let softened = scene * (1.0 - post.z) + blurred * post.z;
    return vec4<f32>(auto_scale * softened * post.w, 1.0);
}
