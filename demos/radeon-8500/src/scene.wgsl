struct Frame {
    view_projection: mat4x4f,
    eye_time: vec4f,
    fog: vec4f,
    options: vec4f,
    lights: array<vec4f, 4>,
};
struct Material {
    tint: vec4f,
    params: vec4f,
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var base_tex: texture_2d<f32>;
@group(1) @binding(2) var detail_tex: texture_2d<f32>;
@group(1) @binding(3) var repeat_sampler: sampler;
@group(1) @binding(4) var third_tex: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
    @location(4) uv_extra: vec4f,
};
struct VertexOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
    @location(4) uv_extra: vec4f,
};

@vertex fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = frame.view_projection * vec4f(v.pos, 1.0);
    out.world = v.pos;
    out.uv = v.uv;
    out.normal = v.normal;
    out.color = v.color;
    out.uv_extra = v.uv_extra;
    return out;
}

fn point_light(p: vec3f, n: vec3f, v: vec3f, light: vec3f, tint: vec3f) -> vec3f {
    let offset = light - p;
    let distance = length(offset);
    let ray = offset / max(distance, 0.001);
    let cosine = max(dot(n, ray), 0.0);
    let specular = pow(max(dot(n, normalize(v + ray)), 0.0), 28.0);
    return (cosine + 0.36 * specular) * tint * (1.0 / (1.0 + 0.085 * distance * distance));
}

@fragment fn fs_main(v: VertexOut) -> @location(0) vec4f {
    let effect = u32(material.params.x);
    let time = frame.eye_time.w;
    let view = normalize(frame.eye_time.xyz - v.world);
    let chest = frame.options.x > 0.5;
    var uv = v.uv;
    if effect == 3u {
        uv += vec2f(0.035, 0.021) * time;
    } else if effect == 4u {
        uv += vec2f(0.007, 0.002) * time;
    }
    let base = textureSample(base_tex, repeat_sampler, uv) * material.tint;
    let detail = textureSample(detail_tex, repeat_sampler, uv);
    let second = textureSample(detail_tex, repeat_sampler, v.uv_extra.xy);
    let bump_map = textureSample(third_tex, repeat_sampler, v.uv_extra.zw);
    let caustic_uv = v.world.xy * 0.004 + vec2f(0.18, -0.13) * time;
    let caustic_a = textureSample(detail_tex, repeat_sampler, caustic_uv).r;
    let caustic_b = textureSample(detail_tex, repeat_sampler, caustic_uv.yx * 1.13 - vec2f(0.18, 0.04) * time).r;
    let dp_x = dpdx(v.world);
    let dp_y = dpdy(v.world);
    let duv_x = dpdx(uv);
    let duv_y = dpdy(uv);
    let tangent = normalize(dp_x * duv_y.y - dp_y * duv_x.y + vec3f(0.00001, 0.0, 0.0));
    let n0 = normalize(v.normal + vec3f(0.00001, 0.0, 0.0));
    let bitangent = cross(n0, tangent);
    let bump = (detail.xyz * 2.0 - 1.0) * material.params.z;
    let n = normalize(n0 + tangent * bump.x + bitangent * bump.y);
    if base.a < material.params.y { discard; }

    if frame.options.z > 0.5 && v.world.z < frame.options.w && effect != 4u && effect != 11u {
        discard;
    }
    var color = base.rgb;
    let key = max(dot(n, normalize(vec3f(-0.20, -0.63, 0.73))), 0.0);
    if effect == 0u || effect == 2u {
        color *= max(vec3f(0.075), v.color.rgb * 2.0);
        if effect == 2u {
            if material.params.z > 0.0 {
                color = base.rgb * (0.48 + 0.45 * key) * vec3f(0.62, 0.90, 1.0);
            }
            color *= 0.85 + 0.80 * min(caustic_a, caustic_b);
        }
    } else if effect == 3u {
        let a = textureSample(detail_tex, repeat_sampler, uv * 2.2 - time * vec2f(0.053, 0.027)).rg - 0.5;
        let b = textureSample(detail_tex, repeat_sampler, uv.yx * 0.87 + time * vec2f(-0.031, 0.064)).rg - 0.5;
        let wave_normal = normalize(vec3f((a + b) * 0.28, 1.0));
        let fresnel = pow(1.0 - abs(dot(wave_normal, view)), 4.0);
        let sheen = pow(max(dot(wave_normal, normalize(view + vec3f(0.3, -0.4, 0.85))), 0.0), 30.0);
        color = mix(vec3f(0.08, 0.19, 0.25) * (0.70 + base.g * 0.60),
            vec3f(0.34, 0.43, 0.52), fresnel * 0.80) + sheen * 0.24;
    } else if effect == 8u {
        let fine = max(dot(normalize(bump_map.rgb * 2.0 - 1.0), normalize(vec3f(0.35, 0.2, 0.9))), 0.0);
        color = mix(base.rgb, second.rgb, v.color.a) * max(vec3f(0.085), 2.0 * v.color.rgb + fine * 0.15);
    } else if effect == 11u {
        let a_warp = textureSample(base_tex, repeat_sampler, v.uv * 0.9 + vec2f(-0.04, 0.01) * time).rg;
        let b_warp = textureSample(base_tex, repeat_sampler, v.uv * vec2f(2.17, 2.01) + vec2f(-0.05, 0.01) * time).rg;
        let a = textureSample(base_tex, repeat_sampler, v.uv + vec2f(0.05 * time, 0.0) + (a_warp * 2.0 - 1.0) * vec2f(0.10, 0.20));
        let b = textureSample(base_tex, repeat_sampler, v.uv * vec2f(1.7, 1.5) + vec2f(0.1 * time, 0.0) + (b_warp * 2.0 - 1.0) * vec2f(0.10, 0.20));
        let layer_a = min(vec3f(1.0), a.a * 2.0 * vec3f(1.0, 0.7, 0.5));
        let layer_b = min(vec3f(1.0), b.a * 2.0 * vec3f(0.3, 0.25, 0.2));
        color = mix(second.rgb, mix(layer_a, layer_b, b.a), clamp(a.a + b.r, 0.0, 1.0));
    } else if effect == 10u {
        color = vec3f(0.85, 0.89, 0.87);
    } else if effect == 4u {
        let clouds = textureSample(detail_tex, repeat_sampler, uv * 0.8 - vec2f(0.006, 0.002) * time);
        color = clamp(base.rgb * 0.83 + clouds.rgb * 0.17, vec3f(0.0), vec3f(1.0));
    } else if effect == 5u {
        let face_key = max(dot(n, normalize(vec3f(-0.44, -0.63, 0.64))), 0.0);
        let fill = max(dot(n, normalize(vec3f(0.71, -0.52, 0.30))), 0.0);
        let gloss = pow(max(dot(n, normalize(view + vec3f(-0.4, -0.48, 0.65))), 0.0), 28.0);
        color = base.rgb * (vec3f(0.46, 0.38, 0.34) + face_key * vec3f(0.58, 0.53, 0.47)
            + fill * vec3f(0.23, 0.20, 0.20)) + gloss * 0.13;
    } else if effect == 6u || effect == 1u {
        if chest {
            let s = sin(time * 0.67);
            let c = cos(time * 0.67);
            let lights = point_light(v.world, n, view, frame.lights[0].xyz, vec3f(1.27, 0.85, 0.43))
                + point_light(v.world, n, view, frame.lights[1].xyz, vec3f(0.68, 0.31, 0.26))
                + point_light(v.world, n, view, frame.lights[2].xyz, vec3f(0.31, 0.43, 0.78))
                + point_light(v.world, n, view, frame.lights[3].xyz, vec3f(0.26, 0.52, 0.23));
            color = base.rgb * (vec3f(0.22, 0.20, 0.18) + lights);
            if effect == 6u {
                let edge = pow(1.0 - abs(dot(n, view)), 2.5);
                let reflection = reflect(-view, n);
                let highlight = pow(max(dot(reflection, normalize(vec3f(c, 1.0, s))), 0.0), 22.0);
                color += base.rgb * (0.20 + edge * 0.35) + highlight * vec3f(0.56, 0.39, 0.19);
            }
        } else {
            color *= 0.52 + 0.50 * key;
        }
    }

    let fog = select(min(0.93, 1.0 - exp(-frame.fog.w * distance(v.world, frame.eye_time.xyz))), 0.0, effect == 4u || effect == 11u);
    color = mix(color, frame.fog.rgb, fog);
    if frame.options.y > 0.5 {
        color = pow(max(color, vec3f(0.0)), vec3f(2.2));
    }
    let alpha = select(base.a, base.r * 0.55, effect == 10u);
    return vec4f(color, select(1.0, alpha, material.params.w > 0.5));
}

@group(2) @binding(0) var refraction_tex: texture_2d<f32>;
@group(2) @binding(1) var reflection_tex: texture_2d<f32>;
@group(2) @binding(2) var clamp_sampler: sampler;

@vertex fn vs_copy(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn fs_copy(@builtin(position) clip: vec4f) -> @location(0) vec4f {
    return textureLoad(refraction_tex, vec2i(clip.xy), 0);
}

@fragment fn fs_portal_water(v: VertexOut) -> @location(0) vec4f {
    let time = frame.eye_time.w;
    let xy = v.clip.xy / vec2f(textureDimensions(refraction_tex));
    let a = textureSample(base_tex, repeat_sampler, v.uv * 1.7 + vec2f(0.10, 0.06) * time);
    let b = textureSample(base_tex, repeat_sampler, v.uv * 0.8 + vec2f(-0.07, 0.04) * time);
    let ripple = a.rg + b.rg - vec2f(1.0);
    let reflected = textureSample(reflection_tex, clamp_sampler, xy + ripple * 0.033).rgb;
    let refracted = textureSample(refraction_tex, clamp_sampler, xy + ripple * 0.025).rgb;
    let n = normalize(v.normal + vec3f(ripple * 0.50, 0.0));
    let view = normalize(frame.eye_time.xyz - v.world);
    let facing = abs(dot(n, view));
    let fresnel = 0.17 + 0.83 * pow(1.0 - facing, 2.0);
    let light = normalize(vec3f(0.20, -0.34, 0.91));
    let specular = pow(max(dot(light, reflect(-view, n)), 0.0), 22.0);
    let scatter = vec3f(0.040, 0.077, 0.067);
    let transmission = refracted * vec3f(0.72, 0.84, 0.80) + scatter;
    let color = mix(transmission, reflected * 1.08, fresnel) + specular * vec3f(0.42, 0.40, 0.32);
    let steepness = 1.0 - abs(normalize(v.normal).z);
    let film = smoothstep(0.40, 0.92, steepness) * (0.16 + 0.73 * pow(clamp(a.r + b.g - 0.32, 0.0, 1.0), 1.3));
    return vec4f(mix(color, vec3f(0.81, 0.86, 0.81), film), 1.0);
}
