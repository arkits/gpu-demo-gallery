// Texture and color combine units, as `borg.txt` sets them. Level of detail bias 0.5.
// params.v[0]: whether the first and second pictures are on.
fn a(i: V) -> vec4<f32> { return textureSampleBias(t0, s0, i.uv0.xy, 0.5); }
fn b(i: V) -> vec4<f32> { return textureSampleBias(t1, s1, i.uv1.xy, 0.5); }

// Both pictures, multiplied, times the side's light.
@fragment
fn ship(i: V) -> @location(0) vec4<f32> {
    let c = mix(vec3(1.0), a(i).rgb, params.v[0].x) * mix(vec3(1.0), b(i).rgb, params.v[0].y);
    return vec4(c * i.color.rgb, 1.0);
}
// Both, added.
@fragment
fn glow(i: V) -> @location(0) vec4<f32> {
    let c = a(i).rgb * params.v[0].x + b(i).rgb * params.v[0].y;
    return vec4(c, 1.0);
}
@fragment
fn spot(i: V) -> @location(0) vec4<f32> {
    let uv = i.uv0.xy / i.uv0.w;
    return vec4(textureSampleLevel(t0, s0, uv, 0.0).rgb, 1.0);
}
// What the windows reflect, where they are.
@fragment
fn windows(i: V) -> @location(0) vec4<f32> {
    return vec4(a(i).rgb, b(i).a);
}
