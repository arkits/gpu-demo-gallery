// What the texture and color combine units are set to (0x4048b0). Level of detail bias 0.5.
// i.color is the light, with the highlight in alpha.
fn a(i: V) -> vec4<f32> { return textureSampleBias(t0, s0, i.uv0.xy, 0.5); }
fn b(i: V) -> vec4<f32> { return textureSampleBias(t1, s1, i.uv1.xy, 0.5); }

@fragment
fn base(i: V) -> @location(0) vec4<f32> {
    return vec4(b(i).rgb * i.color.rgb + i.color.a, 1.0);
}
// The heights less the heights toward the light, about a middle gray. params.v[0].x:
// whether this is the only pass, and so lit.
@fragment
fn emboss(i: V) -> @location(0) vec4<f32> {
    let height = clamp(b(i).rgb - a(i).rgb + 127.0 / 255.0, vec3(0.0), vec3(1.0));
    return vec4(mix(height, height * i.color.rgb, params.v[0].x) + i.color.a, 1.0);
}
// params.v[0].y: how much of the base picture shows through the reflection.
@fragment
fn mirror(i: V) -> @location(0) vec4<f32> {
    return vec4(mix(a(i).rgb, b(i).rgb, params.v[0].y) * i.color.rgb, 1.0);
}
