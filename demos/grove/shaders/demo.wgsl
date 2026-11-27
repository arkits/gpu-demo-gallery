// OpenGL's lights at corners. uv1: normal.
// params.v[0..4]: matrix. v[4]: sign of the normal, number of lights. v[8..16]: places of the
// lights. v[16..24]: their colors.
@vertex
fn grown(i: In) -> V {
    var o: V;
    o.clip = mat4x4(params.v[0], params.v[1], params.v[2], params.v[3]) * i.position;
    let n = normalize(i.uv1.xyz) * params.v[4].x;
    var sum = vec3(0.0);
    for (var k = 0; k < 8; k++) {
        if f32(k) >= params.v[4].y { break; }
        let to = params.v[8 + k].xyz - i.position.xyz;
        let d = length(to);
        sum += max(dot(n, to) / d, 0.0) / (d + 0.1 * d * d) * params.v[16 + k].rgb;
    }
    o.color = vec4(min(sum * i.color.rgb, vec3(1.0)), 1.0);
    o.uv0 = i.uv0;
    o.uv1 = i.uv1;
    o.uv2 = i.uv2;
    o.uv3 = i.uv3;
    return o;
}
@fragment
fn leaf(i: V) -> @location(0) vec4<f32> {
    let t = textureSample(t0, s0, i.uv0.xy);
    if t.a <= 0.5 { discard; }
    return vec4(t.rgb * i.color.rgb, 1.0);
}
