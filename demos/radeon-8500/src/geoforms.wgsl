struct Frame {
    centers: array<vec4f, 6>,
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
        q *= mix(1.0, floor(r * 11.0) / max(r * 11.0, 0.0001), audio * 0.55);
    } else if mode == 3.0 {
        q += 0.10 * sin(p.yzx * 5.3 + p.zxy * 2.1 + t * 1.5);
    }
    var d = 100.0;
    for (var i = 0u; i < 6u; i += 1u) {
        let ball = frame.centers[i];
        d = smooth_union(d, length(q - ball.xyz) - ball.w, 0.38 + audio * 0.18);
    }
    return d;
}

fn contour(screen: vec2f, t: f32) -> vec3f {
    let uv = vec2f(screen.x, -screen.y) * 0.53 + 0.5;
    let filtered = textureSampleLevel(stencil, linear_sampler, clamp(uv, vec2f(0.0), vec2f(1.0)), 0.0).r;
    let inside = all(uv > vec2f(0.0)) && all(uv < vec2f(1.0));
    let mask = select(0.0, smoothstep(0.29, 0.62, filtered + 0.07 * sin(t * 1.9 + uv.y * 16.0)), inside);
    return vec3f(mask, filtered, mask * (1.0 - filtered));
}

@fragment fn fs_geo(v: Out) -> @location(0) vec4f {
    let t = frame.options.x;
    let audio = frame.options.y;
    let screen = v.screen * vec2f(frame.aspect_fov.x, 1.0);
    let ray = normalize(vec3f(screen * tan(frame.aspect_fov.y * 0.5), -1.0));
    let eye = vec3f(0.0, 0.0, 6.1);
    let bg = background(ray, t);
    var travel = 3.15;
    var hit = false;
    for (var i = 0; i < 64; i += 1) {
        let step = field(eye + ray * travel);
        if step < 0.004 {
            hit = true;
            break;
        }
        travel += max(step * 0.60, 0.005);
        if travel > 9.0 { break; }
    }
    var col = bg;
    if hit {
        let p = eye + ray * travel;
        let e = 0.007;
        let dx = vec3f(e, 0.0, 0.0);
        let dy = dx.yxz;
        let dz = dx.yzx;
        let n0 = normalize(vec3f(field(p + dx) - field(p - dx), field(p + dy) - field(p - dy), field(p + dz) - field(p - dz)));
        let texture_color = noise3(p * 0.35 + vec3f(0.0, 0.019 * t, 0.05 * t));
        let n = normalize(n0 + (texture_color - 0.5) * 0.13);
        let edge = 1.0 - clamp(dot(n, -ray), 0.0, 1.0);
        let fresnel = pow(edge, 1.7);
        let refracted = background(normalize(ray + vec3f(n.xy, 0.0) * (0.15 + edge * 0.37)), t);
        let reflected = background(reflect(ray, n), t);
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
    if frame.options.w > 0.5 {
        let mask = contour(screen, t);
        let rim = mask.z * vec3f(0.32, 0.85, 0.96);
        col = mix(bg, col + rim + mask.y * vec3f(0.12, 0.22, 0.32), mask.x);
    }
    col *= 1.07 - 0.22 * dot(v.screen, v.screen);
    col = clamp(col, vec3f(0.0), vec3f(1.0));
    if frame.aspect_fov.z > 0.5 { col = pow(col, vec3f(2.2)); }
    return vec4f(col, 1.0);
}
