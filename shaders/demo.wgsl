@fragment
fn cubed(i: V) -> @location(0) vec4<f32> {
    return textureSample(cube, cube_s, i.uv0.xyz);
}
