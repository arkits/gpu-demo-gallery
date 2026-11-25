fn placed_at(p: vec3<f32>) -> vec4<f32> {
    return mat4x4<f32>(params.v[0], params.v[1], params.v[2], params.v[3]) * vec4<f32>(p, 1.0);
}

@vertex
fn placed(i: In) -> V {
    var o: V;
    o.clip = placed_at(i.position.xyz);
    o.color = i.color;
    o.uv0 = i.uv0;
    return o;
}

// A square of the water: a vertex is one of four corners, an instance holds what is at them.
struct Square {
    @location(6) height: vec4<f32>,
    @location(7) nx: vec4<f32>,
    @location(8) nz: vec4<f32>,
    @location(9) light: vec4<f32>,
    @location(10) u: vec4<f32>,
    @location(11) v: vec4<f32>,
};

@vertex
fn water(i: In, s: Square, @builtin(instance_index) at: u32) -> V {
    let k = u32(i.position.x);
    let place = vec2<f32>(f32(at % 80u + k % 2u), f32(at / 80u + k / 2u)) * 0.025 - 1.0;
    let p = vec3<f32>(place.x, s.height[k], place.y);
    let n = vec3<f32>(s.nx[k], 1.0, s.nz[k]);
    let toward = normalize(p - params.v[4].xyz);
    let bent = dot(toward - 0.25 * n, n);
    let see = clamp(120.0 * bent * bent, 0.0, 255.0) / 255.0;
    var o: V;
    o.clip = placed_at(p);
    o.color = vec4<f32>(vec3<f32>(s.light[k]), see);
    o.uv0 = vec4<f32>(reflect(toward, n), 0.0);
    o.uv1 = vec4<f32>(s.u[k], s.v[k], place * 0.5 + 0.5);
    return o;
}

// The surroundings, or the water's reflection of them.
@fragment
fn around(i: V) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(cube, cube_s, i.uv0.xyz).rgb, params.v[5].x);
}

@fragment
fn basin(i: V) -> @location(0) vec4<f32> {
    let normal = textureSample(t1, s1, i.uv0.xy).rgb * 2.0 - 1.0;
    let sun = params.v[6];
    let light = saturate(dot(normal, sun.xyz * sun.w) + 0.1 * sun.w);
    return vec4<f32>(textureSample(t0, s0, i.uv0.xy).rgb * light, 1.0);
}

// What lies below the water and on it, over the reflection.
@fragment
fn below(i: V) -> @location(0) vec4<f32> {
    let sun = params.v[6].w;
    let pool = textureSample(t0, s0, i.uv1.xy).rgb;
    let dirt = textureSample(t1, s1, i.uv1.zw);
    let color = i.color.a * sun * (pool + i.color.rgb - 0.5);
    let all = mix(max(color, vec3<f32>(0.0)), dirt.rgb * sun, dirt.a);
    return vec4<f32>(all, (1.0 - i.color.a) * (1.0 - dirt.a));
}

@fragment
fn wires(i: V) -> @location(0) vec4<f32> {
    return vec4<f32>(0.75, 0.8, 0.7, 1.0);
}
