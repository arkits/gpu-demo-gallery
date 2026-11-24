// Fluid solver passes, ported from the demo's Cg programs of the same names.
// The 3D grid is one 2D texture: slices laid out in rows of `cols`.
// Interior and boundary cells, separate draws in the original, are one pass here:
// a boundary cell reads as its nearest interior cell would.

struct Params {
    dim: vec3i,
    cols: i32,
    a: vec4f,
    b: vec4f,
    c: vec4f,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var t2: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    return vec4f(f32(i32(i & 1u) * 4 - 1), f32(i32(i & 2u) * 2 - 1), 0.0, 1.0);
}

struct Cell {
    at: vec3i,
    // Nearest interior cell.
    near: vec3i,
    boundary: bool,
}
fn cell_at(frag: vec4f) -> Cell {
    let px = vec2i(frag.xy);
    let tile = px / p.dim.xy;
    var c: Cell;
    c.at = vec3i(px - tile * p.dim.xy, tile.y * p.cols + tile.x);
    c.near = clamp(c.at, vec3i(1), p.dim - 2);
    c.boundary = any(c.at != c.near);
    return c;
}
fn texel(c: vec3i) -> vec2i {
    let k = clamp(c, vec3i(0), p.dim - 1);
    return k.xy + p.dim.xy * vec2i(k.z % p.cols, k.z / p.cols);
}
fn fetch(t: texture_2d<f32>, c: vec3i) -> vec4f {
    return textureLoad(t, texel(c), 0);
}
fn trilerp(t: texture_2d<f32>, s: vec3f) -> vec4f {
    let base = floor(s - 0.5);
    let f = s - 0.5 - base;
    let k = vec3i(base);
    let z0 = mix(
        mix(fetch(t, k), fetch(t, k + vec3i(1, 0, 0)), f.x),
        mix(fetch(t, k + vec3i(0, 1, 0)), fetch(t, k + vec3i(1, 1, 0)), f.x), f.y);
    let z1 = mix(
        mix(fetch(t, k + vec3i(0, 0, 1)), fetch(t, k + vec3i(1, 0, 1)), f.x),
        mix(fetch(t, k + vec3i(0, 1, 1)), fetch(t, k + vec3i(1, 1, 1)), f.x), f.y);
    return mix(z0, z1, f.z);
}
fn back(c: Cell, forward: f32) -> vec3f {
    return vec3f(c.near) + 0.5 - p.a.x * forward * fetch(t0, c.near).xyz;
}
const X = vec3i(1, 0, 0);
const Y = vec3i(0, 1, 0);
const Z = vec3i(0, 0, 1);

// t0 velocity, t1 advected. a: timestep, interior scale, boundary scale, direction.
@fragment
fn advect(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    return trilerp(t1, back(c, p.a.w)) * select(p.a.y, p.a.z, c.boundary);
}

// t0 velocity, t1 phi, t2 phi after a round trip. a as above; b.x: clamp.
@fragment
fn advect_bfecc(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    let pos = back(c, 1.0);
    let half_dim = vec3f(p.dim) * 0.5;
    let off = abs(half_dim - vec3f(c.near) - 0.5);
    var r = trilerp(t1, pos);
    if !any(off > half_dim - 2.0) {
        r = 1.5 * r - 0.5 * trilerp(t2, pos);
    }
    return clamp(r, vec4f(-p.b.x), vec4f(p.b.x)) * select(p.a.y, p.a.z, c.boundary);
}

// Copies t0; boundary cells take their interior neighbor's value.
@fragment
fn boundary(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    return fetch(t0, cell_at(frag).near);
}

@fragment
fn vorticity(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary {
        return vec4f(0.0);
    }
    let l = fetch(t0, c.at - X);
    let r = fetch(t0, c.at + X);
    let b = fetch(t0, c.at - Y);
    let t = fetch(t0, c.at + Y);
    let d = fetch(t0, c.at - Z);
    let u = fetch(t0, c.at + Z);
    return vec4f(0.5 * vec3f((t.z - b.z) - (u.y - d.y), (u.x - d.x) - (r.z - l.z),
        (r.y - l.y) - (t.x - b.x)), 0.0);
}

// t0 vorticity; added to the velocity. a: timestep, epsilon.
@fragment
fn confinement(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary {
        discard;
    }
    let omega = fetch(t0, c.at).xyz;
    // The original takes the length of all four channels; the fourth is unwritten.
    let grad = 0.5 * vec3f(
        length(fetch(t0, c.at + X).xyz) - length(fetch(t0, c.at - X).xyz),
        length(fetch(t0, c.at + Y).xyz) - length(fetch(t0, c.at - Y).xyz),
        length(fetch(t0, c.at + Z).xyz) - length(fetch(t0, c.at - Z).xyz));
    let eta = normalize(grad + 0.001);
    return vec4f(p.a.x * p.a.y * cross(eta, omega), 0.0);
}

// a: center, size. b: color.
@fragment
fn gaussian(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary {
        discard;
    }
    let dist = length(vec3f(c.at) + 0.5 - p.a.xyz) * p.a.w;
    return vec4f(p.b.rgb, exp(-dist * dist));
}

@fragment
fn divergence(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let k = cell_at(frag).near;
    return vec4f(0.5 * (fetch(t0, k + X).x - fetch(t0, k - X).x
        + fetch(t0, k + Y).y - fetch(t0, k - Y).y
        + fetch(t0, k + Z).z - fetch(t0, k - Z).z));
}

// t0 pressure, t1 divergence. The original's alpha is -1 and rBeta 1/6.
@fragment
fn jacobi(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let k = cell_at(frag).near;
    return (fetch(t0, k - X) + fetch(t0, k + X) + fetch(t0, k - Y) + fetch(t0, k + Y)
        + fetch(t0, k + Z) + fetch(t0, k - Z) - fetch(t1, k)) / 6.0;
}

// t0 velocity, t1 pressure.
@fragment
fn project(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    let k = c.near;
    let grad = 0.5 * vec3f(fetch(t1, k + X).x - fetch(t1, k - X).x,
        fetch(t1, k + Y).x - fetch(t1, k - Y).x, fetch(t1, k + Z).x - fetch(t1, k - Z).x);
    return vec4f(fetch(t0, k).xyz - grad * select(1.0, -1.0, c.boundary), 0.0);
}

// Stamps the logo (t0) into slices a.x <= z < a.y.
@fragment
fn decal(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.at.z < i32(p.a.x) || c.at.z >= i32(p.a.y) || any(c.at.xy != c.near.xy) {
        discard;
    }
    let image = vec2f(textureDimensions(t0));
    let uv = (vec2f(c.at.xy) - 0.5) / vec2f(p.dim.xy - 2) * image - 0.5;
    let base = floor(uv);
    let f = uv - base;
    let top = vec2i(image) - 1;
    let k0 = clamp(vec2i(base), vec2i(0), top);
    let k1 = clamp(vec2i(base) + 1, vec2i(0), top);
    let row0 = mix(textureLoad(t0, k0, 0), textureLoad(t0, vec2i(k1.x, k0.y), 0), f.x);
    let row1 = mix(textureLoad(t0, vec2i(k0.x, k1.y), 0), textureLoad(t0, k1, 0), f.x);
    return mix(row0, row1, f.y) * 1.2;
}

// Water ---------------------------------------------------------------------------

// Initial level set: the signed height above a quarter of the box.
@fragment
fn level_init(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    return vec4f(f32(cell_at(frag).at.y - p.dim.y / 4));
}

// Constant downward velocity in the top two rows.
@fragment
fn lid(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    if cell_at(frag).at.y < p.dim.y - 2 {
        discard;
    }
    return vec4f(0.0, -0.01, 0.0, 0.0);
}

// Keeps the water up: adds part of the flat surface's level set below it.
@fragment
fn inject(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary {
        discard;
    }
    let d = f32(c.at.y) + 0.5 - 24.0;
    return vec4f(0.2 * select(d, 0.0, d > 1.0));
}

// t0 level set: its normalized gradient, keeping the value in alpha.
@fragment
fn gradient(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary {
        return vec4f(0.0, 0.0, 0.0, 1.0);
    }
    let g = vec3f(fetch(t0, c.at + X).a - fetch(t0, c.at - X).a,
        fetch(t0, c.at + Y).a - fetch(t0, c.at - Y).a,
        fetch(t0, c.at + Z).a - fetch(t0, c.at - Z).a);
    // The original's normalize of a zero vector is undefined.
    return vec4f(g / max(length(g), 1e-6), fetch(t0, c.at).a);
}

// t0 level set. a.x: downward acceleration; blended with source alpha.
@fragment
fn gravity(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let c = cell_at(frag);
    if c.boundary || fetch(t0, c.at).a >= 1.0 {
        discard;
    }
    return vec4f(0.0, p.a.x, 0.0, 1.0);
}
