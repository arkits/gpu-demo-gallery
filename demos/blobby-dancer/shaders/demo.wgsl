// The port's own programs. The looks and the floor's are the demo's, translated from its
// fp30 listings when it starts (`src/fp30.rs`), and follow this.

fn matrix(at: u32) -> mat4x4<f32> {
    return mat4x4<f32>(params.v[at], params.v[at + 1u], params.v[at + 2u], params.v[at + 3u]);
}

fn unit(v: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    return select(vec3<f32>(0.0, 0.0, 1.0), v / l, l > 1e-20);
}

// fresnel.vp30: the normal and the reflected eye ray, both in the eye's space.
@vertex
fn body(i: In) -> V {
    let mv = matrix(4u);
    let p = vec4<f32>(i.position.xyz, 1.0);
    let n = unit((mv * vec4<f32>(i.uv0.xyz, 0.0)).xyz);
    let e = unit((mv * p).xyz);
    var o: V;
    o.clip = matrix(0u) * p;
    o.color = vec4<f32>(1.0);
    o.uv0 = vec4<f32>(n, 0.0);
    o.uv1 = vec4<f32>(unit(2.0 * dot(e, n) * n - e), 0.0);
    o.uv3 = p;
    return o;
}

// floor.vp30: the normal in the eye's space, and the place on the floor.
@vertex
fn ground(i: In) -> V {
    let mv = matrix(4u);
    let p = vec4<f32>(i.position.xyz, 1.0);
    var o: V;
    o.clip = matrix(0u) * p;
    o.color = vec4<f32>(1.0);
    o.uv0 = vec4<f32>((mv * vec4<f32>(i.uv0.xyz, 0.0)).xyz, 0.0);
    o.uv1 = p;
    o.uv3 = p;
    return o;
}

// The finished frame on the window.
@fragment
fn shown(i: V) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(t0, s0, i.uv0.xy).rgb, 1.0);
}
