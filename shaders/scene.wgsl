struct U {
    view_proj: mat4x4f,
    view: mat4x4f,
    inverse: mat4x4f,
    // Direction to the light, in the world.
    light: vec4f,
    // Color, and where it hides everything.
    fog: vec4f,
    // Caustic frame, beam's texture offset, size of a point in clip space over w.
    misc: vec4f,
    // Two currents for plankton, pixels.
    drift: vec4f,
    eye: vec4f,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var caustics: texture_2d_array<f32>;
@group(0) @binding(3) var water: texture_cube<f32>;
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;

struct Vertex {
    @location(0) pos: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    // Per object.
    @location(3) m0: vec4f,
    @location(4) m1: vec4f,
    @location(5) m2: vec4f,
    @location(6) m3: vec4f,
    @location(7) diffuse: vec4f,
    @location(8) ambient: vec4f,
}
struct V {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
    @location(1) primary: vec3f,
    @location(2) world: vec3f,
    // Direction to the light in the object's space.
    @location(3) light: vec3f,
    @location(4) clear: f32,
}

// Linear fog by depth, per vertex.
fn clear(world: vec3f) -> f32 {
    let z = -(u.view * vec4f(world, 1.0)).z;
    return saturate((u.fog.w - z) / u.fog.w);
}
fn fogged(c: vec3f, clear: f32) -> vec3f {
    return mix(u.fog.rgb, c, clear);
}
fn caustic(world: vec3f) -> vec3f {
    return textureSample(caustics, samp, vec2f(world.x, -world.z) * 0.4, i32(u.misc.x)).rgb;
}

@vertex
fn vs(v: Vertex) -> V {
    let m = mat4x4f(v.m0, v.m1, v.m2, v.m3);
    let world = (m * vec4f(v.pos, 1.0)).xyz;
    let normal = (m * vec4f(v.normal, 0.0)).xyz;
    var o: V;
    o.clip = u.view_proj * vec4f(world, 1.0);
    o.uv = v.uv;
    o.primary = saturate(v.diffuse.rgb * max(dot(normal, u.light.xyz), 0.0) + v.ambient.rgb);
    o.world = world;
    o.light = vec3f(dot(m[0].xyz, u.light.xyz), dot(m[1].xyz, u.light.xyz), dot(m[2].xyz, u.light.xyz));
    o.clear = clear(world);
    return o;
}
@fragment
fn lit_fs(v: V) -> @location(0) vec4f {
    return vec4f(fogged(v.primary, v.clear), 1.0);
}
@fragment
fn textured_fs(v: V) -> @location(0) vec4f {
    let base = textureSample(t0, samp, v.uv).rgb * v.primary;
    return vec4f(fogged(base * caustic(v.world), v.clear), 1.0);
}
@fragment
fn bump_fs(v: V) -> @location(0) vec4f {
    let n = textureSample(t0, samp, v.uv).xyz * 2.0 - 1.0;
    let d = saturate(dot(normalize(v.light), n));
    return vec4f(fogged(textureSample(t1, samp, v.uv).rgb * d, v.clear), 1.0);
}
@fragment
fn beam_fs(v: V) -> @location(0) vec4f {
    let t = textureSample(t0, samp, v.uv + u.misc.yz);
    return vec4f(fogged(t.rgb, v.clear), t.a);
}

struct Sky {
    @builtin(position) clip: vec4f,
    @location(0) ray: vec3f,
}
@vertex
fn sky_vs(@builtin(vertex_index) i: u32) -> Sky {
    let p = vec2f(f32(i & 1u) * 4.0 - 1.0, f32(i >> 1u) * 4.0 - 1.0);
    let far = u.inverse * vec4f(p, 1.0, 1.0);
    return Sky(vec4f(p, 1.0, 1.0), far.xyz / far.w - u.eye.xyz);
}
@fragment
fn sky_fs(v: Sky) -> @location(0) vec4f {
    return vec4f(textureSample(water, samp, v.ray).rgb, 1.0);
}

struct P {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) clear: f32,
}
@vertex
fn plankton_vs(@builtin(vertex_index) i: u32, @location(0) p: vec4f, @location(1) cell: vec4f) -> P {
    let along = mix(vec3f(0.0, u.drift.y, 0.0), vec3f(u.drift.x, 0.0, 0.0), p.w);
    let world = p.xyz + cell.xyz + along;
    let c = u.view_proj * vec4f(world, 1.0);
    let corner = vec2f(f32(i & 1u), f32(i >> 1u)) * 2.0 - 1.0;
    return P(vec4f(c.xy + corner * u.drift.zw * c.w, c.zw), world, clear(world));
}
@fragment
fn plankton_fs(v: P) -> @location(0) vec4f {
    return vec4f(fogged(caustic(v.world), v.clear), 1.0);
}

struct Q {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
}
@vertex
fn image_vs(@location(0) pos: vec2f, @location(1) uv: vec2f) -> Q {
    return Q(vec4f(pos, 0.0, 1.0), uv);
}
@fragment
fn image_fs(q: Q) -> @location(0) vec4f {
    return textureSample(t0, samp, q.uv);
}
