// Surface: base texture modulated by light, optional env map blend, simple fog.
fn fog_mix(color: vec3<f32>, fog: f32) -> vec3<f32> {
    let fc = params.v[1].rgb;
    return mix(color, fc, clamp(fog, 0.0, 1.0));
}

@fragment
fn surface(i: V) -> @location(0) vec4<f32> {
    let base = textureSample(t0, s0, i.uv0.xy);
    let env = textureSample(t1, s1, i.uv1.xy);
    let env_on = params.v[0].x;
    let lit = base.rgb * i.color.rgb;
    let mixed = mix(lit, mix(lit, env.rgb, 0.45), env_on);
    let fog = i.uv2.x * params.v[0].y;
    return vec4(fog_mix(mixed, fog), 1.0);
}
