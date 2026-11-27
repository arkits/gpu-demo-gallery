// ScatterVS10.vsh, SkyPS10.psh and TerrainPS10.psh of the original, instruction by
// instruction. params.v[i] is the vertex program's c[i]; v[14] is the pixel program's c[1].

fn c(i: u32) -> vec4<f32> {
    return params.v[i];
}
fn m(i: u32) -> mat4x4<f32> {
    return mat4x4<f32>(params.v[i], params.v[i + 1u], params.v[i + 2u], params.v[i + 3u]);
}

// Extinction goes out as the first color and inscattering as the second; colors from a
// vertex program are kept between 0 and 1.
@vertex
fn scatter(i: In) -> V {
    let p = vec4<f32>(i.position.xyz, 1.0);
    var o: V;
    o.clip = m(0u) * p;
    // 0: normals and horizons, 1: ground, 2: clouds.
    o.uv0 = vec4<f32>(p.xz * c(16u).xy + c(16u).zw, 0.0, 0.0);
    o.uv1 = vec4<f32>(p.xz * c(16u).xy + c(16u).zw, 0.0, 0.0);
    o.uv2 = vec4<f32>(p.xz * c(18u).xy + c(18u).zw, 0.0, 0.0);
    // V, from the point to the eye.
    let world = m(8u) * p;
    let v = normalize(c(22u).xyz - world.xyz);
    let cos_theta = dot(v, c(12u).xyz);
    let phase1 = cos_theta * cos_theta + c(31u).x;
    // Distance: depth in the view.
    let s = (m(4u) * p).z;
    let e1 = exp2(c(24u).xyz * -s * c(31u).y);
    var extinction = e1 * c(13u).xyz;
    // Henyey-Greenstein with c28 = (1 - g², 1 + g, 2g).
    let x = c(28u).z * cos_theta + c(28u).y;
    let r = inverseSqrt(abs(x));
    let phase2 = r * r * r * c(28u).x;
    var inscattering = (c(27u).xyz * phase1 + c(26u).xyz * phase2) * (c(31u).x - e1) * c(23u).xyz;
    inscattering = inscattering * c(25u).y * c(15u).xyz * c(15u).w;
    extinction = extinction * c(15u).xyz * c(15u).w;
    o.color = vec4<f32>(saturate(extinction), 1.0);
    o.uv3 = vec4<f32>(saturate(inscattering), 1.0);
    return o;
}

// The sky is the inscattering alone.
@fragment
fn sky(i: V) -> @location(0) vec4<f32> {
    return vec4<f32>(i.uv3.rgb, 1.0);
}

// (N.L) * ground * shadow from the horizon * cloud cover, times extinction, plus
// inscattering.
@fragment
fn terrain(i: V) -> @location(0) vec4<f32> {
    let n = textureSample(t0, s0, i.uv0.xy);
    let ground = textureSample(t1, s1, i.uv1.xy);
    let cloud = textureSample(t2, s2, i.uv2.xy);
    let sun = c(14u);
    var r = saturate(dot(sun.xyz, n.rgb * 2.0 - 1.0)) * ground.rgb;
    let shadow = saturate(sun.w - n.a);
    r = r * (1.0 - shadow);
    r = r * (1.0 - cloud.rgb);
    r = r * i.color.rgb + i.uv3.rgb;
    return vec4<f32>(saturate(r), 1.0);
}
