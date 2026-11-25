// Room surface: the 3DF texture modulated by baked per-face lighting (vertex
// colour) and a little distance fog driven by params.v[0].
@fragment
fn lit(i: V) -> @location(0) vec4<f32> {
    let base = textureSample(t0, s0, i.uv0.xy);
    let lit = base.rgb * i.color.rgb;
    let fog = clamp(i.color.a * 0.0 + params.v[0].x, 0.0, 1.0);
    return vec4(mix(lit, vec3<f32>(0.03, 0.04, 0.06), fog), 1.0);
}
