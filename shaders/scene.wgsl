// Drawing, ported from the demo's Cg programs of the same names. Window
// coordinates run downwards here and upwards in the original.

struct Scene {
    view_proj: mat4x4f,
    eye: vec3f,
    // Window pixels to background pixels.
    scale: f32,
    forward: vec3f,
    // Nonzero: show the background through the glass.
    refract_on: f32,
    dim: vec3f,
    cols: f32,
}
@group(0) @binding(0) var<uniform> s: Scene;
@group(0) @binding(1) var smooth_clamp: sampler;
@group(0) @binding(2) var smooth_repeat: sampler;
@group(0) @binding(3) var env: texture_cube<f32>;
@group(0) @binding(4) var velvet: texture_2d<f32>;
@group(0) @binding(5) var shadow: texture_2d<f32>;
@group(0) @binding(6) var noise: texture_2d<f32>;
// The scene behind the glass, a volume image, or the image to blur.
@group(1) @binding(0) var image: texture_2d<f32>;
// The atlas: smoke or the level set.
@group(1) @binding(1) var atlas: texture_2d<f32>;

struct Mesh {
    @builtin(position) clip: vec4f,
    @location(0) p: vec3f,
    @location(1) n: vec3f,
    @location(2) uv: vec2f,
}
@vertex
fn mesh_vs(@location(0) pos: vec3f, @location(1) n: vec3f, @location(2) uv: vec2f) -> Mesh {
    return Mesh(s.view_proj * vec4f(pos, 1.0), pos, n, uv);
}

fn gold_phong(n: vec3f, l: vec3f, e: vec3f, diffuse: vec3f, kd: f32, ks: f32) -> vec3f {
    let r = 2.0 * dot(l, n) * n - l;
    return kd * max(0.0, dot(n, l)) * diffuse + ks * pow(max(0.0, dot(r, e)), 10.0);
}

@fragment
fn gold_fs(v: Mesh) -> @location(0) vec4f {
    let e = normalize(s.eye - v.p);
    let r = 2.0 * dot(v.n, e) * v.n - e;
    let gold = vec3f(1.0, 0.8, 0.4);
    var c = gold_phong(v.n, normalize(vec3f(100.0, 200.0, 300.0) - v.p), e, gold, 0.55, 0.9);
    c += gold_phong(v.n, normalize(vec3f(-80.0, 200.0, -300.0) - v.p), e, gold, 0.55, 0.9);
    c += 0.5 * textureSample(env, smooth_clamp, r).rgb;
    return vec4f(c * vec3f(0.8, 0.8, 1.0), 1.0);
}

@fragment
fn velvet_fs(v: Mesh) -> @location(0) vec4f {
    let e = normalize(s.eye - v.p);
    let l = normalize(vec3f(100.0, 200.0, -100.0) - v.p);
    var shiny = pow(max(dot(l, e), 0.0), 10.0) * 0.1;
    let cosine = max(dot(v.n, e), 0.0);
    shiny += pow(sqrt(max(1.0 - cosine * cosine, 0.0)), 10.0) * dot(l, v.n);
    let nap = textureSample(velvet, smooth_repeat, v.p.xz * 8.0).rgb * 0.4 + 0.6;
    let diffuse = vec3f(1.0, 0.2, 0.2) * 0.2 * max(0.0, dot(v.n, l));
    return vec4f(diffuse + 0.8 * shiny * nap, 1.0);
}

@fragment
fn shadow_fs(v: Mesh) -> @location(0) vec4f {
    let light = textureSample(shadow, smooth_clamp, v.uv) * 0.15 * max(0.0, 3.0 - length(v.p.xz));
    return light - textureSample(noise, smooth_repeat, v.p.xz).r * vec4f(0.2, 0.2, 0.2, 0.0);
}

@fragment
fn glass_fs(v: Mesh) -> @location(0) vec4f {
    let e = normalize(s.eye - v.p);
    let l = normalize(vec3f(100.0, 200.0, 300.0) - v.p);
    var n = v.n;
    if dot(e, n) < 0.0 {
        n = -n;
    }
    n = normalize(n + 0.02 * sin(v.p * 10.0));
    let r = 2.0 * dot(n, e) * n - e;
    let c = vec3f(0.6, 0.6, 1.0) * max(0.0, dot(n, l)) + 0.5 * textureSample(env, smooth_clamp, r).rgb;
    return vec4f(c * vec3f(0.8, 0.8, 1.0), 0.2);
}

@fragment
fn refract_fs(v: Mesh) -> @location(0) vec4f {
    let y = 0.5 - abs(v.p.y);
    let refractivity = min(1.0, 9000000.0 * pow(y, 15.0));
    let e = normalize(s.eye - v.p);
    let l = normalize(vec3f(-100.0, 200.0, -300.0) - v.p);
    var n = v.n;
    if dot(e, n) < 0.0 {
        n = -n;
    }
    n = normalize(n + 0.1 * sin(length(v.p) * 400.0) * refractivity);
    let r = 2.0 * dot(n, e) * n - e;
    var c = gold_phong(n, l, e, vec3f(0.5, 0.6, 1.0), 1.0, 2.9);
    c += 0.5 * textureSample(env, smooth_clamp, r).rgb;
    c *= vec3f(0.8, 0.8, 1.0);
    let at = v.clip.xy * s.scale + dot(e, n) * n.xy * vec2f(40.0, -40.0);
    let behind = textureSampleLevel(image, smooth_clamp, at / vec2f(textureDimensions(image)), 0.0).rgb;
    if s.refract_on > 0.0 {
        c = 0.2 * c + 0.8 * behind;
    }
    return vec4f(c, 0.2);
}

// Depth only.
@fragment
fn occlude_fs(v: Mesh) -> @location(0) vec4f {
    return vec4f(0.0);
}

// Volume ---------------------------------------------------------------------------

struct Ray {
    @builtin(position) clip: vec4f,
    // Where sampling starts, in cells; the +X face is moved inwards.
    @location(0) start: vec3f,
    // Where the ray really meets the box, in cells.
    @location(1) surface: vec3f,
}
@vertex
fn ray_vs(@location(0) pos: vec3f, @location(1) start: vec3f) -> Ray {
    return Ray(s.view_proj * vec4f(pos, 1.0), start, pos * 128.0 + s.dim * 0.5);
}
fn eye_cells() -> vec3f {
    return s.eye * 128.0 + s.dim * 0.5;
}
// Samples to take: the box's thickness along the view axis, one per cell. The
// original subtracts two depth images.
fn samples(surface: vec3f) -> i32 {
    let d = normalize(surface - eye_cells());
    let far = select(vec3f(0.0), s.dim, d > vec3f(0.0));
    let t = (far - surface) / select(d, vec3f(1e-6), abs(d) < vec3f(1e-6));
    let reach = max(min(t.x, min(t.y, t.z)), 0.0);
    return i32(reach * dot(d, s.forward));
}
fn volume(q: vec3f) -> vec4f {
    let z = clamp(floor(q.z), 0.0, s.dim.z - 1.0);
    let z1 = min(z + 1.0, s.dim.z - 1.0);
    let xy = clamp(q.xy, vec2f(0.5), s.dim.xy - 0.5);
    let size = vec2f(textureDimensions(atlas));
    let a = (xy + s.dim.xy * vec2f(z % s.cols, floor(z / s.cols))) / size;
    let b = (xy + s.dim.xy * vec2f(z1 % s.cols, floor(z1 / s.cols))) / size;
    return mix(textureSampleLevel(atlas, smooth_clamp, a, 0.0),
        textureSampleLevel(atlas, smooth_clamp, b, 0.0), clamp(q.z - z, 0.0, 1.0));
}

// Smoke, back to front.
@fragment
fn slicecast_fs(v: Ray) -> @location(0) vec4f {
    let d = normalize(v.start - eye_cells());
    let count = samples(v.surface);
    var at = v.start + f32(count) * d;
    var color = vec3f(0.0);
    var sum = 0.0;
    for (var i = 0; i < count; i++) {
        let c = volume(at);
        sum += c.a;
        color = (1.0 - c.a) * color + c.a * c.rgb;
        at -= d;
    }
    return vec4f(color, sum);
}

fn spline(x: f32) -> vec4f {
    let a = fract(x);
    let w1 = ((3.0 * a * a * a - 6.0 * a * a) + 4.0) / 6.0;
    let g0 = (((3.0 - a) * a * a - 3.0 * a) + 1.0) / 6.0 + w1;
    let w3 = a * a * a / 6.0;
    let g1 = (((3.0 - 3.0 * a) * a + 3.0) * a + 1.0) / 6.0 + w3;
    return vec4f(w1 / g0 - 1.0, w3 / g1 + 1.0, g0, g1);
}

// Water: finds the level set's surface, then shades it (`lsraycast`, `watershade`).
@fragment
fn water_fs(v: Ray) -> @location(0) vec4f {
    let eye = eye_cells();
    let d = normalize(v.start - eye);
    let count = samples(v.surface);
    var o = v.start;
    var near = o;
    var far = o;
    var f_near = 0.0;
    var f_far = 0.0;
    var found = false;
    for (var i = 0; i < count; i++) {
        let here = volume(o).a;
        if here < 0.0 {
            near = o - d;
            far = o;
            f_near = volume(near).a;
            f_far = here;
            found = true;
            break;
        }
        o += d;
    }
    if !found {
        return vec4f(0.0);
    }
    var hit = far;
    var value = f_far;
    for (var i = 0; i < 3; i++) {
        hit = near + (far - near) * -f_near / (f_far - f_near);
        value = volume(hit).a;
        if value > 0.0 {
            near = hit;
        } else {
            far = hit;
        }
    }
    hit = near + (far - near) * -f_near / (f_far - f_near);
    if !(abs(value) < 32.0 && (d.x < 0.0 || hit.x < 125.0)) {
        return vec4f(0.0);
    }
    let tx = spline(hit.x);
    let ty = spline(hit.y);
    let tz = spline(hit.z);
    let q = floor(hit);
    var n = vec3f(0.0);
    for (var i = 0; i < 8; i++) {
        let b = vec3i(i & 1, (i >> 1) & 1, i >> 2);
        let h = vec3f(select(tx.x, tx.y, b.x == 1), select(ty.x, ty.y, b.y == 1), select(tz.x, tz.y, b.z == 1));
        let g = select(tx.z, tx.w, b.x == 1) * select(ty.z, ty.w, b.y == 1) * select(tz.z, tz.w, b.z == 1);
        n += g * volume(q + h).xyz;
    }
    n = n / max(length(n), 1e-6);
    let l = normalize(vec3f(200.0, 500.0, 200.0) - hit);
    let e = normalize(eye - hit);
    let r = 2.0 * dot(l, n) * n - l;
    var c = max(0.0, dot(n, l)) * vec3f(0.0, 0.0, 0.3);
    let up = vec3f(0.0, 1.0, 0.0);
    let b1 = normalize(up - dot(up, e) * e);
    let b2 = cross(-e, b1);
    let at = v.clip.xy - vec2f(dot(n, b2), -dot(n, b1)) * 30.0;
    let behind = textureSampleLevel(image, smooth_clamp, at / vec2f(textureDimensions(image)), 0.0).rgb;
    c = 0.15 * c + 0.85 * behind;
    let facing = max(0.0, dot(e, n));
    let mirror = textureSampleLevel(env, smooth_clamp, 2.0 * dot(e, n) * n - e, 0.0).rgb;
    c = mix(mirror, c, facing) + 0.5 * pow(max(0.0, dot(e, r)), 150.0);
    return vec4f(c, 1.0);
}

// Full-screen ----------------------------------------------------------------------

struct Quad {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
}
@vertex
fn quad_vs(@builtin(vertex_index) i: u32) -> Quad {
    let c = vec2f(f32(i32(i & 1u) * 4 - 1), f32(i32(i & 2u) * 2 - 1));
    return Quad(vec4f(c, 0.0, 1.0), vec2f(c.x, -c.y) * 0.5 + 0.5);
}
@fragment
fn copy_fs(v: Quad) -> @location(0) vec4f {
    return textureSampleLevel(image, smooth_clamp, v.uv, 0.0);
}
@fragment
fn glow_fs(v: Quad) -> @location(0) vec4f {
    return vec4f(0.8 * textureSampleLevel(image, smooth_clamp, v.uv, 0.0).rgb, 0.0);
}
fn tap(v: Quad, x: f32, y: f32) -> vec4f {
    return textureSampleLevel(image, smooth_clamp, v.uv + vec2f(x, -y) / vec2f(textureDimensions(image)), 0.0);
}
@fragment
fn blurh_fs(v: Quad) -> @location(0) vec4f {
    return (tap(v, -4.0, 0.5) + tap(v, -2.0, -0.5) + tap(v, 0.0, 0.5) + tap(v, 2.0, -0.5)
        + tap(v, 4.0, 0.5)) / 5.0 - 0.02;
}
@fragment
fn blurv_fs(v: Quad) -> @location(0) vec4f {
    return (tap(v, -0.5, -4.0) + tap(v, 0.5, -2.0) + tap(v, -0.5, 0.0) + tap(v, 0.5, 2.0)
        + tap(v, -0.5, 4.0)) / 5.0 - 0.02;
}
