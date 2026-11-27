// The ViRGE's modes: Gouraud color, texture with perspective, and texture whose
// coordinates go straight across the screen (UNLIT_TEXTURE).

// `plain` and `textured` are in the prelude.

// Screen-straight coordinates without `@interpolate(linear)`, which WebGL lacks: with
// uv w and w interpolated for perspective, their quotient is uv interpolated across the
// screen.
@vertex
fn flat(i: In) -> V {
    var o: V;
    o.clip = i.position;
    o.color = i.color;
    o.uv0 = vec4<f32>(i.uv0.xy * i.position.w, i.position.w, 0.0);
    return o;
}

@fragment
fn affine(i: V) -> @location(0) vec4<f32> {
    return i.color * textureSample(t0, s0, i.uv0.xy / i.uv0.z);
}
