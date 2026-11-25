// OpenGL's lights, for each pixel. color: emission. uv0: picture. uv1: place. uv2: normal.
// uv3: diffuse, highlight. The power is 64 throughout.
// params.v[0]: eye, number of lights of the bolt. v[1..3]: those lights. v[4..7]: sparks and
// how bright each still is.
fn shine(i: V) -> vec3<f32> {
    let place = i.uv1.xyz;
    let n = normalize(i.uv2.xyz);
    let view = normalize(params.v[0].xyz - place);
    var sum = i.color.rgb;
    for (var k = 0; k < 8; k++) {
        var at = vec3(0.0, 10.0, 0.0);
        var diffuse = vec3(0.4);
        var specular = vec3(0.5);
        var fade = 1.0;
        if k >= 1 && k < 4 {
            if f32(k) > params.v[0].w { continue; }
            at = params.v[k].xyz;
            diffuse = vec3(0.1, 0.1, 0.13);
            specular = vec3(0.1, 0.1, 0.2);
            fade = 1.0 / (0.5 + 0.001 * distance(at, place));
        } else if k >= 4 {
            let l = params.v[k].w;
            at = params.v[k].xyz;
            diffuse = vec3(l * l, l * l * l, l * l * l * l) * 0.3;
            specular = vec3(l, l * l, l * l * l) * 0.6;
        }
        let to = normalize(at - place);
        let d = dot(n, to);
        if d <= 0.0 { continue; }
        let h = pow(max(dot(n, normalize(to + view)), 0.0), 64.0);
        sum += fade * (d * diffuse * i.uv3.rgb + h * specular * i.uv3.w);
    }
    return clamp(sum, vec3(0.0), vec3(1.0));
}
@fragment
fn lit(i: V) -> @location(0) vec4<f32> {
    let t = textureSample(t0, s0, i.uv0.xy);
    return vec4(shine(i), 1.0) * t;
}
@fragment
fn lit_wall(i: V) -> @location(0) vec4<f32> {
    let t = textureSample(t0, s0, i.uv0.xy);
    let c = vec4(shine(i), 1.0) * t;
    if t.a <= 0.1 { discard; }
    return c;
}
@fragment
fn dark_wall(i: V) -> @location(0) vec4<f32> {
    let t = textureSample(t0, s0, i.uv0.xy);
    if t.a <= 0.1 { discard; }
    return vec4(t.rgb * 0.7, t.a);
}
// The normal picture against the direction between light and eye, as the register
// combiners had it (0x41fce0): the product to the eighth, where the direction faces out.
@fragment
fn bump(i: V) -> @location(0) vec4<f32> {
    let n = textureSample(t0, s0, i.uv0.xy).xyz * 2.0 - 1.0;
    let h = normalize(i.uv1.xyz);
    let d = clamp(dot(n, h), 0.0, 1.0) * step(0.0, h.z);
    let d2 = d * d;
    let d4 = d2 * d2;
    return vec4(vec3(d4 * d4), 0.0);
}
