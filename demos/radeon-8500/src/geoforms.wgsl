struct Frame {
    centers: array<vec4f, 8>,
    options: vec4f,
    aspect_fov: vec4f,
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var volume: texture_2d<f32>;
@group(0) @binding(2) var warp: texture_2d<f32>;
@group(0) @binding(3) var stencil: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;

struct Out {
    @builtin(position) clip: vec4f,
    @location(0) screen: vec2f,
};

@vertex fn vs_geo(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return Out(vec4f(uv * 2.0 - 1.0, 0.0, 1.0), uv * 2.0 - 1.0);
}

fn atlas(p: vec3f, z: f32) -> vec2f {
    let tile = fract(p.xy) * 31.0 + vec2f(0.5);
    return vec2f(tile.x / 32.0, (tile.y + 32.0 * (z % 32.0)) / 1024.0);
}

fn noise3(p: vec3f) -> vec3f {
    let depth = fract(p.z) * 32.0;
    let a = textureSampleLevel(volume, linear_sampler, atlas(p, floor(depth)), 0.0).rgb;
    let b = textureSampleLevel(volume, linear_sampler, atlas(p, floor(depth) + 1.0), 0.0).rgb;
    return mix(a, b, fract(depth));
}

fn background(ray: vec3f, t: f32) -> vec3f {
    let u = atan2(ray.x, ray.z) * 0.15915494 + 0.5;
    let v = acos(clamp(ray.y, -1.0, 1.0)) * 0.31830989;
    let w = textureSampleLevel(warp, linear_sampler, vec2f(u * 1.7 + t * 0.007, v * 2.0 - t * 0.009), 0.0);
    let q = ray + (w.rgb - 0.5) * 0.22;
    let ridge = sin(q.y * (14.0 + 11.0 * sin(t * 0.071)) + sin(q.x * 11.0 + t * 0.39) * 2.4 + q.z * 7.0);
    let bands = pow(0.5 + 0.5 * ridge, 3.4);
    let hue = 0.5 + 0.5 * cos(vec3f(0.1, 2.3, 4.1) + 5.5 * q.x + q.z * 2.4 + t * 0.23);
    let luminous = pow(max(dot(ray, normalize(vec3f(sin(t * 0.13), 0.67, -0.9))), 0.0), 16.0);
    return 0.021 + hue * (0.10 + bands * 0.36) + luminous * vec3f(0.82, 0.43, 0.21);
}

fn smooth_union(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn field(p: vec3f) -> f32 {
    let t = frame.options.x;
    let audio = frame.options.y;
    let mode = frame.options.z;
    var q = p;
    if mode == 1.0 {
        q *= 1.0 + (0.06 * audio + 0.025) * sin(length(p) * 10.0 - t * 3.1 - 1.0);
    } else if mode == 2.0 {
        let r = length(p);
        let bands = r * 11.0;
        let steps = floor(bands) + smoothstep(0.15, 0.85, fract(bands));
        q *= mix(1.0, steps / max(bands, 0.0001), 0.22 + audio * 0.24);
    } else if mode == 3.0 {
        q += 0.10 * sin(p.yzx * 5.3 + p.zxy * 2.1 + t * 1.5);
    }
    if frame.options.w > 0.5 {
        let turn = sin(t * 0.39) * 0.45;
        let xy = vec3f(cos(turn) * q.x + sin(turn) * q.z, q.y, cos(turn) * q.z - sin(turn) * q.x);
        let plane = vec2f(xy.x / max(frame.aspect_fov.w, 0.35), -xy.y);
        let uv = plane / 2.5 + 0.5;
        let border = max(abs(uv.x - 0.5), abs(uv.y - 0.5)) - 0.485;
        let map = textureSampleLevel(stencil, linear_sampler, clamp(uv, vec2f(0.003), vec2f(0.997)), 0.0);
        let silhouette = max((map.r - 0.5) * 0.84, border * 2.5);
        let half_depth = (0.105 + 0.24 * map.g) * (1.0 + 0.22 * audio);
        let z = abs(xy.z + 0.10 * sin(5.0 * xy.y - t * 1.7)) - half_depth;
        let d = vec2f(silhouette, z) + 0.035;
        return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0) - 0.035;
    }
    var d = 100.0;
    for (var i = 0u; i < 8u; i += 1u) {
        let ball = frame.centers[i];
        d = smooth_union(d, length(q - ball.xyz) - ball.w, 0.38 + audio * 0.18);
    }
    return d;
}

@fragment fn fs_geo(v: Out) -> @location(0) vec4f {
    let t = frame.options.x;
    let audio = frame.options.y;
    let aspect = frame.aspect_fov.x;
    let screen = v.screen * vec2f(aspect, 1.0) * max(1.0, 1.05 / aspect);
    let ray = normalize(vec3f(screen * tan(frame.aspect_fov.y * 0.5), -1.0));
    let eye = vec3f(0.0, 0.0, 6.1);
    let bg = background(ray, t);
    let relative = dot(eye, ray);
    let discriminant = relative * relative - dot(eye, eye) + 3.15 * 3.15;
    var travel = -relative - sqrt(max(discriminant, 0.0));
    let end = -relative + sqrt(max(discriminant, 0.0));
    var hit = false;
    if discriminant > 0.0 {
        for (var i = 0; i < 62; i += 1) {
            let step = field(eye + ray * travel);
            if step < 0.003 {
                hit = true;
                break;
            }
            travel += max(step * 0.69, 0.004);
            if travel > end { break; }
        }
    }
    var col = bg;
    if hit {
        let p = eye + ray * travel;
        let e = 0.008;
        let a = vec3f(1.0, -1.0, -1.0);
        let b = vec3f(-1.0, -1.0, 1.0);
        let c = vec3f(-1.0, 1.0, -1.0);
        let d = vec3f(1.0, 1.0, 1.0);
        let n0 = normalize(a * field(p + e * a) + b * field(p + e * b)
            + c * field(p + e * c) + d * field(p + e * d));
        let texture_color = noise3(p * 0.35 + vec3f(0.0, 0.019 * t, 0.05 * t));
        let n = normalize(n0 + (texture_color - 0.5) * 0.13);
        let edge = 1.0 - clamp(dot(n, -ray), 0.0, 1.0);
        let fresnel = pow(edge, 1.7);
        let refracted = background(normalize(ray + vec3f(n.xy, 0.0) * (0.15 + edge * 0.37)), t);
        let reflection = reflect(ray, n);
        let reflected = mix(background(reflection, t),
            background(normalize(reflection + vec3f(0.14, 0.08, -0.05)), t), 0.35);
        let hue = 0.52 + 0.46 * cos(vec3f(2.0, 0.2, 3.8) + t * 0.26);
        let ramp = hue * (0.72 + 0.28 * texture_color.x);
        let light = normalize(vec3f(sin(t * 0.53), 0.65, 1.0));
        var thickness = 0.05;
        for (var j = 0; j < 16; j += 1) {
            if field(p + ray * thickness) > 0.005 { break; }
            thickness += 0.09;
        }
        let scatter = pow(max(dot(n, light), 0.0), 2.0) * (0.12 + 0.11 * audio);
        let spec = pow(max(dot(reflect(ray, n), light), 0.0), 42.0);
        let absorption = exp(-(vec3f(1.0) - ramp) * thickness * 1.5);
        let body = ramp * (0.24 + scatter) * (1.0 - exp(-thickness * 3.1));
        col = refracted * absorption * (0.73 - fresnel * 0.54) + body * (0.89 - fresnel * 0.53)
            + reflected * (0.15 + fresnel * 0.91) + spec * vec3f(1.0, 0.92, 0.78);
    }
    col *= 1.07 - 0.22 * dot(v.screen, v.screen);
    col = clamp(col, vec3f(0.0), vec3f(1.0));
    if frame.aspect_fov.z > 0.5 { col = pow(col, vec3f(2.2)); }
    return vec4f(col, 1.0);
}
