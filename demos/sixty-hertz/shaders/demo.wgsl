// Chrome ball: base texture tinted, with a second texture blended in for a
// cheap reflective sheen. `params.v[0].x` is the blend amount.
@fragment
fn surface(i: V) -> @location(0) vec4<f32> {
    let base = textureSample(t0, s0, i.uv0.xy);
    let shine = textureSample(t1, s1, i.uv0.yx);
    let amount = clamp(params.v[0].x, 0.0, 1.0);
    let rgb = mix(base.rgb * i.color.rgb, shine.rgb * 0.6 + base.rgb * 0.4, amount);
    return vec4(rgb, 1.0);
}
