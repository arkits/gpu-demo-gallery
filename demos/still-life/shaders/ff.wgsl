// Direct3D 6-8 fixed-function pipeline. Numbers are D3D's (see src/d3d.rs).
struct Stage {
    op: vec4<u32>,    // color op, arg1, arg2, arg0
    aop: vec4<u32>,   // alpha op, arg1, arg2, arg0
    misc: vec4<u32>,  // texcoord index (and generation), transform flags, result arg, kind
    bump: vec4<f32>,  // bump matrix 00 01 10 11
    bump2: vec4<f32>, // luminance scale and offset
    tm: mat4x4<f32>,
};
struct Draw {
    world: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    normal: mat4x4<f32>,
    flags: vec4<u32>,   // fvf, lighting bits, stages used, shade mode
    sources: vec4<u32>, // diffuse, specular, ambient, emissive material sources
    fog: vec4<f32>,     // start, end, density
    fog_color: vec4<f32>,
    fog_mode: vec4<u32>, // table, vertex, enabled, range
    ambient: vec4<f32>,
    tfactor: vec4<f32>,
    alpha: vec4<u32>,    // test on, function, reference
    viewport: vec4<f32>,
    m_diffuse: vec4<f32>,
    m_ambient: vec4<f32>,
    m_specular: vec4<f32>,
    m_emissive: vec4<f32>,
    m_power: vec4<f32>,
    stages: array<Stage, 8>,
};
struct Light {
    kind: vec4<u32>,
    diffuse: vec4<f32>,
    specular: vec4<f32>,
    ambient: vec4<f32>,
    position: vec4<f32>,  // view space, range in w
    direction: vec4<f32>, // view space, falloff in w
    attenuation: vec4<f32>,
    spot: vec4<f32>,      // cos(theta/2), cos(phi/2)
};
struct Lights { l: array<Light, 8> };

@group(0) @binding(0) var<uniform> d: Draw;
@group(0) @binding(1) var<uniform> lights: Lights;
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var s0: sampler;
@group(1) @binding(2) var t1: texture_2d<f32>;
@group(1) @binding(3) var s1: sampler;
@group(1) @binding(4) var t2: texture_2d<f32>;
@group(1) @binding(5) var s2: sampler;
@group(1) @binding(6) var t3: texture_2d<f32>;
@group(1) @binding(7) var s3: sampler;
@group(1) @binding(8) var t4: texture_2d<f32>;
@group(1) @binding(9) var s4: sampler;
@group(1) @binding(10) var t5: texture_2d<f32>;
@group(1) @binding(11) var s5: sampler;
@group(1) @binding(12) var t6: texture_2d<f32>;
@group(1) @binding(13) var s6: sampler;
@group(1) @binding(14) var t7: texture_2d<f32>;
@group(1) @binding(15) var s7: sampler;
@group(1) @binding(32) var c0: texture_cube<f32>;
@group(1) @binding(33) var c1: texture_cube<f32>;
@group(1) @binding(34) var c2: texture_cube<f32>;
@group(1) @binding(35) var c3: texture_cube<f32>;

struct In {
    @location(0) pos: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) diffuse: vec4<f32>,
    @location(3) specular: vec4<f32>,
    @location(4) uv0: vec2<f32>,
    @location(5) uv1: vec2<f32>,
    @location(6) uv2: vec2<f32>,
    @location(7) uv3: vec2<f32>,
    @location(8) uv4: vec2<f32>,
    @location(9) uv5: vec2<f32>,
    @location(10) uv6: vec2<f32>,
    @location(11) uv7: vec2<f32>,
};
struct V {
    @builtin(position) clip: vec4<f32>,
    @location(0) diffuse: vec4<f32>,
    @location(1) specular: vec4<f32>,
    @location(2) tc0: vec4<f32>,
    @location(3) tc1: vec4<f32>,
    @location(4) tc2: vec4<f32>,
    @location(5) tc3: vec4<f32>,
    @location(6) tc4: vec4<f32>,
    @location(7) tc5: vec4<f32>,
    @location(8) tc6: vec4<f32>,
    @location(9) tc7: vec4<f32>,
    @location(10) fog: vec2<f32>, // eye depth, vertex fog factor
};

const FVF_XYZRHW: u32 = 4u;
const FVF_NORMAL: u32 = 16u;
const FVF_DIFFUSE: u32 = 64u;
const FVF_SPECULAR: u32 = 128u;

fn bgra(c: vec4<f32>) -> vec4<f32> {
    return c.zyxw;
}

fn fog_factor(mode: u32, dist: f32) -> f32 {
    switch mode {
        case 1u: { return exp(-d.fog.z * dist); }
        case 2u: { let e = d.fog.z * dist; return exp(-e * e); }
        case 3u: { return (d.fog.y - dist) / max(d.fog.y - d.fog.x, 1e-6); }
        default: { return 1.0; }
    }
}

// Material color from the source D3D names: 0 material, 1 vertex diffuse, 2 vertex specular.
fn source(which: u32, m: vec4<f32>, dif: vec4<f32>, spec: vec4<f32>, fvf: u32) -> vec4<f32> {
    if (d.flags.y & 2u) == 0u { return m; }
    if which == 1u && (fvf & FVF_DIFFUSE) != 0u { return dif; }
    if which == 2u && (fvf & FVF_SPECULAR) != 0u { return spec; }
    return m;
}

fn texcoord(s: u32, i: In, p: vec3<f32>, n: vec3<f32>) -> vec4<f32> {
    let st = d.stages[s];
    let index = st.misc.x & 0xffffu;
    let gen = st.misc.x >> 16u;
    var uv = i.uv0;
    switch index {
        case 1u: { uv = i.uv1; }
        case 2u: { uv = i.uv2; }
        case 3u: { uv = i.uv3; }
        case 4u: { uv = i.uv4; }
        case 5u: { uv = i.uv5; }
        case 6u: { uv = i.uv6; }
        case 7u: { uv = i.uv7; }
        default: {}
    }
    var c = vec4<f32>(uv, 1.0, 0.0);
    if gen == 1u {
        c = vec4<f32>(n, 1.0);
    } else if gen == 2u {
        c = vec4<f32>(p, 1.0);
    } else if gen == 3u {
        var e = vec3<f32>(0.0, 0.0, -1.0);
        if (d.flags.y & 16u) != 0u { e = -normalize(p); }
        c = vec4<f32>(2.0 * dot(n, e) * n - e, 1.0);
    }
    if (st.misc.y & 0xffu) != 0u {
        c = st.tm * c;
    }
    return c;
}

@vertex
fn vs(i: In) -> V {
    var o: V;
    let fvf = d.flags.x;
    var dif = vec4<f32>(1.0);
    var spec = vec4<f32>(0.0);
    if (fvf & FVF_DIFFUSE) != 0u { dif = bgra(i.diffuse); }
    if (fvf & FVF_SPECULAR) != 0u { spec = bgra(i.specular); }
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    var depth = 0.0;
    if (fvf & FVF_XYZRHW) != 0u {
        var w = 1.0;
        if i.pos.w != 0.0 { w = 1.0 / i.pos.w; }
        let vp = d.viewport;
        let x = (i.pos.x - vp.x) / vp.z * 2.0 - 1.0;
        let y = 1.0 - (i.pos.y - vp.y) / vp.w * 2.0;
        o.clip = vec4<f32>(x * w, y * w, i.pos.z * w, w);
        depth = w;
        o.diffuse = dif;
        o.specular = spec;
    } else {
        let wp = d.world * vec4<f32>(i.pos.xyz, 1.0);
        let ep = d.view * wp;
        o.clip = d.proj * ep;
        p = ep.xyz;
        depth = ep.z;
        n = (d.normal * vec4<f32>(i.normal, 0.0)).xyz;
        if (d.flags.y & 8u) != 0u { n = normalize(n); }
        if (d.flags.y & 1u) != 0u {
            let md = source(d.sources.x, d.m_diffuse, dif, spec, fvf);
            let ms = source(d.sources.y, d.m_specular, dif, spec, fvf);
            let ma = source(d.sources.z, d.m_ambient, dif, spec, fvf);
            let me = source(d.sources.w, d.m_emissive, dif, spec, fvf);
            var acc_a = vec3<f32>(0.0);
            var acc_d = vec3<f32>(0.0);
            var acc_s = vec3<f32>(0.0);
            var eye = vec3<f32>(0.0, 0.0, -1.0);
            if (d.flags.y & 16u) != 0u { eye = -normalize(p); }
            for (var k = 0u; k < 8u; k++) {
                let l = lights.l[k];
                if l.kind.x == 0u { continue; }
                var dir = -l.direction.xyz;
                var att = 1.0;
                if l.kind.x != 3u {
                    let v = l.position.xyz - p;
                    let dist = length(v);
                    if dist > l.position.w { continue; }
                    dir = v / max(dist, 1e-9);
                    att = 1.0 / max(l.attenuation.x + l.attenuation.y * dist
                        + l.attenuation.z * dist * dist, 1e-9);
                    if l.kind.x == 2u {
                        let rho = dot(-dir, l.direction.xyz);
                        if rho <= l.spot.y {
                            att = 0.0;
                        } else if rho < l.spot.x {
                            att *= pow((rho - l.spot.y) / max(l.spot.x - l.spot.y, 1e-6),
                                l.direction.w);
                        }
                    }
                }
                let ndl = dot(n, dir);
                acc_a += l.ambient.rgb * att;
                if ndl > 0.0 {
                    acc_d += l.diffuse.rgb * ndl * att;
                    if (d.flags.y & 4u) != 0u {
                        let h = normalize(dir + eye);
                        let nh = max(dot(n, h), 0.0);
                        if nh > 0.0 {
                            acc_s += l.specular.rgb * pow(nh, d.m_power.x) * att;
                        }
                    }
                }
            }
            let c = me.rgb + ma.rgb * d.ambient.rgb + ma.rgb * acc_a + md.rgb * acc_d;
            o.diffuse = vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), md.a);
            o.specular = vec4<f32>(clamp(ms.rgb * acc_s, vec3<f32>(0.0), vec3<f32>(1.0)), spec.a);
        } else {
            o.diffuse = dif;
            o.specular = spec;
        }
    }
    var vfog = spec.a;
    if d.fog_mode.y != 0u && (fvf & FVF_XYZRHW) == 0u {
        var dist = depth;
        if d.fog_mode.w != 0u { dist = length(p); }
        vfog = clamp(fog_factor(d.fog_mode.y, dist), 0.0, 1.0);
    }
    o.fog = vec2<f32>(depth, vfog);
    o.tc0 = texcoord(0u, i, p, n);
    o.tc1 = texcoord(1u, i, p, n);
    o.tc2 = texcoord(2u, i, p, n);
    o.tc3 = texcoord(3u, i, p, n);
    o.tc4 = texcoord(4u, i, p, n);
    o.tc5 = texcoord(5u, i, p, n);
    o.tc6 = texcoord(6u, i, p, n);
    o.tc7 = texcoord(7u, i, p, n);
    return o;
}

fn coord(s: u32, c: vec4<f32>) -> vec2<f32> {
    let f = d.stages[s].misc.y;
    if (f & 256u) != 0u {
        let k = f & 0xffu;
        var q = c.w;
        if k == 3u { q = c.z; }
        if k == 2u { q = c.y; }
        return c.xy / q;
    }
    return c.xy;
}

fn is_bump(s: u32) -> bool {
    let o = d.stages[s].op.x;
    return o == 22u || o == 23u;
}

// The perturbation a bump stage gives the next, and its luminance.
fn bump(s: u32, t: vec4<f32>) -> vec3<f32> {
    let st = d.stages[s];
    let du = (t.r * 255.0 - 128.0) / 127.0;
    let dv = (t.g * 255.0 - 128.0) / 127.0;
    let off = vec2<f32>(st.bump.x * du + st.bump.z * dv, st.bump.y * du + st.bump.w * dv);
    var lum = 1.0;
    if st.op.x == 23u { lum = clamp(t.b * st.bump2.x + st.bump2.y, 0.0, 1.0); }
    return vec3<f32>(off, lum);
}

fn arg(a: u32, cur: vec4<f32>, tex: vec4<f32>, dif: vec4<f32>, spec: vec4<f32>,
    temp: vec4<f32>) -> vec4<f32> {
    var v = cur;
    switch a & 15u {
        case 0u: { v = dif; }
        case 2u: { v = tex; }
        case 3u: { v = d.tfactor; }
        case 4u: { v = spec; }
        case 5u: { v = temp; }
        default: {}
    }
    if (a & 0x20u) != 0u { v = vec4<f32>(v.a); }
    if (a & 0x10u) != 0u { v = vec4<f32>(1.0) - v; }
    return v;
}

fn op(o: u32, a1: vec4<f32>, a2: vec4<f32>, a0: vec4<f32>, cur: vec4<f32>, tex: vec4<f32>,
    dif: vec4<f32>) -> vec4<f32> {
    var r = cur;
    switch o {
        case 2u: { r = a1; }
        case 3u: { r = a2; }
        case 4u: { r = a1 * a2; }
        case 5u: { r = a1 * a2 * 2.0; }
        case 6u: { r = a1 * a2 * 4.0; }
        case 7u: { r = a1 + a2; }
        case 8u: { r = a1 + a2 - 0.5; }
        case 9u: { r = (a1 + a2 - 0.5) * 2.0; }
        case 10u: { r = a1 - a2; }
        case 11u: { r = a1 + a2 - a1 * a2; }
        case 12u: { r = mix(a2, a1, dif.a); }
        case 13u: { r = mix(a2, a1, tex.a); }
        case 14u: { r = mix(a2, a1, d.tfactor.a); }
        case 15u: { r = a1 + a2 * (1.0 - tex.a); }
        case 16u: { r = mix(a2, a1, cur.a); }
        case 17u: { r = a1; }
        case 18u: { r = vec4<f32>(a1.rgb + a1.a * a2.rgb, a1.a * a2.a); }
        case 19u: { r = vec4<f32>(a1.rgb * a2.rgb + a1.a, a1.a * a2.a); }
        case 20u: { r = vec4<f32>((1.0 - a1.a) * a2.rgb + a1.rgb, a1.a * a2.a); }
        case 21u: { r = vec4<f32>((1.0 - a1.rgb) * a2.rgb + a1.a, a1.a * a2.a); }
        case 24u: { r = vec4<f32>(dot((a1.rgb - 0.5) * 2.0, (a2.rgb - 0.5) * 2.0)); }
        case 25u: { r = a0 + a1 * a2; }
        case 26u: { r = mix(a2, a1, a0); }
        default: {}
    }
    return clamp(r, vec4<f32>(0.0), vec4<f32>(1.0));
}

fn passes(f: u32, a: f32, r: f32) -> bool {
    switch f {
        case 1u: { return false; }
        case 2u: { return a < r; }
        case 3u: { return a == r; }
        case 4u: { return a <= r; }
        case 5u: { return a > r; }
        case 6u: { return a != r; }
        case 7u: { return a >= r; }
        default: { return true; }
    }
}

fn pick(kind: u32, flat: vec4<f32>, cube: vec4<f32>) -> vec4<f32> {
    if kind == 2u { return cube; }
    if kind == 0u { return vec4<f32>(1.0); }
    return flat;
}

@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    var tex: array<vec4<f32>, 8>;
    var b = vec3<f32>(0.0, 0.0, 1.0);
    // Each stage's picture, in stage order: a bump stage moves the next one's coordinates.
    let u0 = coord(0u, v.tc0);
    tex[0] = pick(d.stages[0].misc.w, textureSample(t0, s0, u0),
        textureSample(c0, s0, v.tc0.xyz));
    if is_bump(0u) { b = bump(0u, tex[0]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u1 = coord(1u, v.tc1) + b.xy;
    tex[1] = pick(d.stages[1].misc.w, textureSample(t1, s1, u1),
        textureSample(c1, s1, v.tc1.xyz)) * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(1u) { b = bump(1u, tex[1]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u2 = coord(2u, v.tc2) + b.xy;
    tex[2] = pick(d.stages[2].misc.w, textureSample(t2, s2, u2),
        textureSample(c2, s2, v.tc2.xyz)) * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(2u) { b = bump(2u, tex[2]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u3 = coord(3u, v.tc3) + b.xy;
    tex[3] = pick(d.stages[3].misc.w, textureSample(t3, s3, u3),
        textureSample(c3, s3, v.tc3.xyz)) * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(3u) { b = bump(3u, tex[3]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u4 = coord(4u, v.tc4) + b.xy;
    tex[4] = pick(d.stages[4].misc.w, textureSample(t4, s4, u4), vec4<f32>(1.0))
        * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(4u) { b = bump(4u, tex[4]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u5 = coord(5u, v.tc5) + b.xy;
    tex[5] = pick(d.stages[5].misc.w, textureSample(t5, s5, u5), vec4<f32>(1.0))
        * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(5u) { b = bump(5u, tex[5]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u6 = coord(6u, v.tc6) + b.xy;
    tex[6] = pick(d.stages[6].misc.w, textureSample(t6, s6, u6), vec4<f32>(1.0))
        * vec4<f32>(vec3<f32>(b.z), 1.0);
    if is_bump(6u) { b = bump(6u, tex[6]); } else { b = vec3<f32>(0.0, 0.0, 1.0); }
    let u7 = coord(7u, v.tc7) + b.xy;
    tex[7] = pick(d.stages[7].misc.w, textureSample(t7, s7, u7), vec4<f32>(1.0))
        * vec4<f32>(vec3<f32>(b.z), 1.0);

    let dif = v.diffuse;
    let spec = v.specular;
    var cur = dif;
    var temp = vec4<f32>(0.0);
    for (var s = 0u; s < d.flags.z; s++) {
        let st = d.stages[s];
        if st.op.x == 1u { break; }
        if st.op.x == 22u || st.op.x == 23u { continue; }
        let t = tex[s];
        let c = op(st.op.x, arg(st.op.y, cur, t, dif, spec, temp), arg(st.op.z, cur, t, dif,
            spec, temp), arg(st.op.w, cur, t, dif, spec, temp), cur, t, dif);
        var a = cur.a;
        if st.op.x == 24u {
            a = c.a;
        } else if st.aop.x != 1u {
            a = op(st.aop.x, arg(st.aop.y, cur, t, dif, spec, temp), arg(st.aop.z, cur, t,
                dif, spec, temp), arg(st.aop.w, cur, t, dif, spec, temp), cur, t, dif).a;
        }
        let r = vec4<f32>(c.rgb, a);
        if st.misc.z == 5u { temp = r; } else { cur = r; }
    }
    var out = cur;
    if (d.flags.y & 4u) != 0u {
        out = vec4<f32>(min(out.rgb + spec.rgb, vec3<f32>(1.0)), out.a);
    }
    if d.fog_mode.z != 0u {
        var f = v.fog.y;
        if d.fog_mode.x != 0u { f = clamp(fog_factor(d.fog_mode.x, v.fog.x), 0.0, 1.0); }
        out = vec4<f32>(mix(d.fog_color.rgb, out.rgb, f), out.a);
    }
    if d.alpha.x != 0u && !passes(d.alpha.y, round(out.a * 255.0), f32(d.alpha.z)) {
        discard;
    }
    return out;
}
