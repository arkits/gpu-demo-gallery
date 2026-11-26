// Glide's fixed-function pixel pipeline, driven by an ATB material
// (`atrMaterialRealize`): the texture unit's combine, then the color and
// alpha combine, alpha test, chroma key and fog. Enumerations are Glide 2's.
//
// params: 0 color combine function, factor, local, other; 1 color invert,
// alpha function, factor, local; 2 alpha other, alpha invert, texture rgb
// function, factor; 3 texture alpha function, factor, rgb invert, alpha
// invert; 4 constant color; 5 alpha test function, reference, chroma key on,
// projected coordinates; 6 chroma key color; 7 fog on, near, far; 8 fog color.
// Vertex: color is the iterated color, uv0 the texture coordinates (q in w),
// uv1.x the eye depth.

// One unit's `grTexCombine` with no unit above it, so "other" is zero.
// Detail and LOD-fraction factors drive Glide's two-pass trilinear blend;
// the port samples with mipmaps instead, so those functions keep the texel.
fn tmu_factor(f: i32, local: vec4<f32>, alpha: bool) -> vec4<f32> {
    switch f {
        case 1: { if alpha { return vec4(local.a); } return local; }
        case 3: { return vec4(local.a); }
        case 8: { return vec4(1.0); }
        case 9: { if alpha { return vec4(1.0 - local.a); } return 1.0 - local; }
        case 10: { return vec4(1.0); }
        case 11: { return vec4(1.0 - local.a); }
        default: { return vec4(0.0); }
    }
}

fn tmu(fun: i32, factor: i32, invert: f32, local: vec4<f32>, alpha: bool) -> vec4<f32> {
    let l = select(local, vec4(local.a), alpha);
    let k = tmu_factor(factor, local, alpha);
    var r = l;
    switch fun {
        case 0, 3: { r = vec4(0.0); }
        case 1, 4: { r = l; }
        case 2, 5: { r = vec4(local.a); }
        case 6: { r = -k * l; }
        case 7: { r = (1.0 - k) * l; }
        case 8: { r = -k * l + vec4(local.a); }
        case 16: { r = vec4(local.a); }
        default: { r = l; }
    }
    if invert > 0.5 { r = 1.0 - r; }
    return clamp(r, vec4(0.0), vec4(1.0));
}

fn color_factor(f: i32, local: vec4<f32>, other: vec4<f32>, tex: vec4<f32>) -> vec3<f32> {
    switch f {
        case 0: { return vec3(0.0); }
        case 1: { return local.rgb; }
        case 2: { return vec3(other.a); }
        case 3: { return vec3(local.a); }
        case 4: { return vec3(tex.a); }
        case 8: { return vec3(1.0); }
        case 9: { return 1.0 - local.rgb; }
        case 10: { return vec3(1.0 - other.a); }
        case 11: { return vec3(1.0 - local.a); }
        case 12: { return vec3(1.0 - tex.a); }
        default: { return vec3(0.0); }
    }
}

fn alpha_factor(f: i32, local: f32, other: f32, tex: f32) -> f32 {
    switch f {
        case 0: { return 0.0; }
        case 1, 3: { return local; }
        case 2: { return other; }
        case 4: { return tex; }
        case 8: { return 1.0; }
        case 9, 11: { return 1.0 - local; }
        case 10: { return 1.0 - other; }
        case 12: { return 1.0 - tex; }
        default: { return 0.0; }
    }
}

fn combine3(fun: i32, k: vec3<f32>, local: vec3<f32>, local_a: f32, other: vec3<f32>) -> vec3<f32> {
    switch fun {
        case 0: { return vec3(0.0); }
        case 1: { return local; }
        case 2: { return vec3(local_a); }
        case 3: { return k * other; }
        case 4: { return k * other + local; }
        case 5: { return k * other + local_a; }
        case 6: { return k * (other - local); }
        case 7: { return k * (other - local) + local; }
        case 8: { return k * (other - local) + local_a; }
        case 9: { return -k * local + local; }
        case 16: { return -k * local + local_a; }
        default: { return local; }
    }
}

fn combine1(fun: i32, k: f32, local: f32, other: f32) -> f32 {
    switch fun {
        case 0: { return 0.0; }
        case 1, 2: { return local; }
        case 3: { return k * other; }
        case 4, 5: { return k * other + local; }
        case 6: { return k * (other - local); }
        case 7, 8: { return k * (other - local) + local; }
        case 9, 16: { return -k * local + local; }
        default: { return local; }
    }
}

fn pick(which: i32, iterated: vec4<f32>, tex: vec4<f32>, constant: vec4<f32>) -> vec4<f32> {
    switch which {
        case 0: { return iterated; }
        case 1: { return tex; }
        default: { return constant; }
    }
}

fn compare(fun: i32, a: f32, b: f32) -> bool {
    switch fun {
        case 0: { return false; }
        case 1: { return a < b; }
        case 2: { return a == b; }
        case 3: { return a <= b; }
        case 4: { return a > b; }
        case 5: { return a != b; }
        case 6: { return a >= b; }
        default: { return true; }
    }
}

@fragment
fn glide(i: V) -> Out {
    let p = params.v;
    var uv = i.uv0.xy;
    var outside = false;
    if p[5].w > 0.5 {
        // Projected: divide by q; nothing is lit behind the projector.
        outside = i.uv0.w <= 0.0;
        uv = i.uv0.xy / max(i.uv0.w, 1e-6);
    }
    var texel = textureSample(t0, s0, uv);
    if outside { texel = vec4(0.0, 0.0, 0.0, texel.a); }

    let tex = vec4(
        tmu(i32(p[2].z), i32(p[2].w), p[3].z, texel, false).rgb,
        tmu(i32(p[3].x), i32(p[3].y), p[3].w, texel, true).a,
    );

    let iterated = i.color;
    let constant = p[4];
    // Color combine.
    let c_local = select(iterated, constant, i32(p[0].z) == 1);
    let c_other = pick(i32(p[0].w), iterated, tex, constant);
    // Alpha combine: local 0 iterated, 1 constant.
    let a_local = select(iterated.a, constant.a, i32(p[1].w) == 1);
    let a_other = pick(i32(p[2].x), iterated, tex, constant).a;
    let ck = color_factor(i32(p[0].y), vec4(c_local.rgb, a_local), c_other, tex);
    var rgb = combine3(i32(p[0].x), ck, c_local.rgb, a_local, c_other.rgb);
    if p[1].x > 0.5 { rgb = 1.0 - rgb; }
    let ak = alpha_factor(i32(p[1].z), a_local, a_other, tex.a);
    var a = combine1(i32(p[1].y), ak, a_local, a_other);
    if p[2].y > 0.5 { a = 1.0 - a; }
    rgb = clamp(rgb, vec3(0.0), vec3(1.0));
    a = clamp(a, 0.0, 1.0);

    if !compare(i32(p[5].x), a, p[5].y) { discard; }
    if p[5].z > 0.5 && all(abs(c_other.rgb - p[6].rgb) < vec3(1.0 / 64.0)) { discard; }
    if p[7].x > 0.5 {
        let f = clamp((i.uv1.x - p[7].y) / max(p[7].z - p[7].y, 1e-3), 0.0, 1.0);
        rgb = mix(rgb, p[8].rgb, f);
    }
    return glide_out(vec4(rgb, a), i.clip);
}
