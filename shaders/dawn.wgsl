// The demo's vp30 and fp30 programs (`shaders_ultra`): one vertex program with everything that
// any of them does, and a fragment function for each fp30 program.
struct Frame {
    view: mat4x4f,
    projection: mat4x4f,
    eye: vec4f,
    // Oil, goose bumps, and the two offsets of cloth in units of depth.
    look: vec4f,
    // Width and height of the picture, of a hair, and the badge's share of them.
    size: vec4f,
    badge: vec4f,
    // Rows of the background's camera.
    sky: array<vec4f, 3>,
    // White over the picture.
    fade: vec4f,
}
struct Bones { m: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> bones: Bones;
@group(1) @binding(0) var color: texture_2d<f32>;
@group(1) @binding(1) var bump: texture_2d<f32>;
@group(1) @binding(2) var extra: texture_2d<f32>;
@group(1) @binding(3) var diffuse: texture_cube<f32>;
@group(1) @binding(4) var specular: texture_cube<f32>;
@group(1) @binding(5) var hilight: texture_cube<f32>;
@group(1) @binding(6) var linear: sampler;

struct Vertex {
    @location(0) position: vec3f,
    @location(1) normal: vec4f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
    @location(4) weights: vec4f,
    @location(5) bones: vec4f,
}

struct Point {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) tangent: vec3f,
    @location(2) binormal: vec3f,
    @location(3) normal: vec3f,
    @location(4) view: vec3f,
    // Occlusion, and how much the surface turns away.
    @location(5) edge: vec3f,
}

fn skin(v: Vertex, p: vec4f) -> vec3f {
    var out = vec3f(0.0);
    for (var i = 0; i < 4; i++) {
        let b = i32(v.bones[i]) * 3;
        out += v.weights[i] * vec3f(dot(bones.m[b], p), dot(bones.m[b + 1], p), dot(bones.m[b + 2], p));
    }
    return out;
}

fn transform(v: Vertex, offset: f32) -> Point {
    let world = skin(v, vec4f(v.position, 1.0));
    let n = normalize(v.normal.xyz);
    // Morphing turns the normal, so the tangent is straightened as the m5 programs do.
    let along = v.tangent.xyz - dot(v.tangent.xyz, n) * n;
    var p: Point;
    p.uv = v.uv;
    p.normal = normalize(skin(v, vec4f(n, 0.0)));
    p.tangent = normalize(skin(v, vec4f(along, 0.0)));
    p.binormal = v.tangent.w * normalize(cross(p.normal, p.tangent));
    p.view = normalize(world - frame.eye.xyz);
    let turned = 1.0 - abs(dot(p.view, p.normal));
    p.edge = vec3f(v.normal.w, turned * turned, turned);
    p.position = frame.projection * (frame.view * vec4f(world, 1.0));
    // As glPolygonOffset's units of a 24-bit depth buffer.
    p.position.z += offset / 16777216.0 * p.position.w;
    return p;
}

@vertex fn vs(v: Vertex) -> Point { return transform(v, 0.0); }
@vertex fn vs_under(v: Vertex) -> Point { return transform(v, frame.look.z); }
@vertex fn vs_over(v: Vertex) -> Point { return transform(v, frame.look.w); }

struct Lit {
    n: vec3f,
    r: vec3f,
    diffuse: vec4f,
    specular: vec4f,
    hilight: vec4f,
}

// The fixed-point registers hold -2 to 2.
fn fixed(v: vec3f) -> vec3f { return clamp(v, vec3f(-2.0), vec3f(2.0)); }

fn lit(p: Point, stretch: f32, unit: bool) -> Lit {
    var t = textureSample(bump, linear, p.uv).xyz * 2.0 - 1.0;
    if unit { t = normalize(vec3f(clamp(t.xy * stretch, vec2f(-2.0), vec2f(2.0)), t.z)); }
    var l: Lit;
    l.n = fixed(t.x * p.tangent + t.y * p.binormal + t.z * p.normal);
    l.r = p.view - 2.0 * dot(p.view, l.n) * l.n;
    l.diffuse = textureSample(diffuse, linear, l.n);
    l.specular = textureSample(specular, linear, l.r);
    l.hilight = textureSample(hilight, linear, p.view);
    return l;
}

const LUMINANCE = vec3f(0.299, 0.587, 0.114);

// Skin: light from around, light along the edge, the light behind it, and oil.
struct Skin { base: vec3f, oil: vec3f, rim: vec4f }
fn skin_terms(p: Point) -> Skin {
    let l = lit(p, frame.look.y, true);
    let c = textureSample(color, linear, p.uv);
    let side = textureSample(specular, linear, l.n).rgb * c.a;
    var s: Skin;
    s.base = fixed(l.diffuse.rgb * c.rgb * p.edge.x + side * p.edge.y);
    s.oil = l.specular.rgb * c.a * 0.02 * frame.look.x;
    s.rim = vec4f(l.hilight.rgb * p.edge.y * p.edge.x * 0.7, l.hilight.a);
    return s;
}

@fragment fn finalSkin(p: Point) -> @location(0) vec4f {
    let s = skin_terms(p);
    return vec4f(s.base + s.oil + s.rim.rgb + s.rim.a, 1.0);
}

@fragment fn finalSkinHead(p: Point) -> @location(0) vec4f {
    let s = skin_terms(p);
    let rim = s.rim * textureSample(extra, linear, p.uv).x;
    return vec4f(s.base + s.oil + rim.rgb + rim.a, 1.0);
}

@fragment fn finalSkinHeadBack(p: Point) -> @location(0) vec4f {
    let s = skin_terms(p);
    let rim = s.rim * textureSample(extra, linear, p.uv).y;
    return vec4f(s.base + s.oil + rim.rgb + rim.a, 1.0);
}

@fragment fn finalTearDucts(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let shine = textureSample(hilight, linear, l.r).rgb;
    return vec4f(l.diffuse.rgb * c.rgb * 0.7 + shine * 0.3, 1.0);
}

@fragment fn finalTeeth(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let shine = textureSample(hilight, linear, l.r).rgb;
    return vec4f(0.85 * c.rgb * dot(l.diffuse.rgb, LUMINANCE) + 0.3 * shine, 1.0);
}

@fragment fn finalTongue(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    return vec4f(0.8 * c.rgb * dot(l.diffuse.rgb, LUMINANCE), 1.0);
}

@fragment fn eye_inner(p: Point) -> @location(0) vec4f {
    let light = dot(textureSample(diffuse, linear, p.normal).rgb, LUMINANCE);
    return vec4f(0.8 * textureSample(color, linear, p.uv).rgb * light, 1.0);
}

@fragment fn eye_outer(p: Point) -> @location(0) vec4f {
    let r = reflect(p.view, p.normal);
    let shine = textureSample(hilight, linear, r).rgb;
    return vec4f(0.5 * shine + 0.1 * textureSample(specular, linear, r).rgb, 1.0);
}

@fragment fn finalLeavesWings(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    return l.specular * 0.1 * c.a + l.diffuse * c + l.hilight.a;
}

fn wings(p: Point) -> vec3f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let through = textureSample(extra, linear, p.uv).rgb * 0.3;
    let light = fixed(l.hilight.rgb * through + l.diffuse.rgb);
    return c.rgb * light + l.specular.rgb * 0.8 * c.a + l.hilight.a;
}

@fragment fn finalWingsOpaque(p: Point) -> @location(0) vec4f { return vec4f(wings(p), 1.0); }
@fragment fn finalWingsTransp(p: Point) -> @location(0) vec4f { return vec4f(wings(p), 0.8); }

@fragment fn finalDressTransp(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let shine = textureSample(hilight, linear, l.r).rgb * 0.3;
    return vec4f(shine * c.a + l.diffuse.rgb * c.rgb, 0.4);
}

@fragment fn finalLeavesTranslFR(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let through = textureSample(extra, linear, p.uv).rgb * 0.5;
    let light = fixed(l.hilight.rgb * through + l.diffuse.rgb);
    return vec4f(l.specular.rgb * 0.1 * c.a + c.rgb * light + l.hilight.a, 1.0);
}

@fragment fn finalLeavesTranslTranspFR(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let through = textureSample(extra, linear, p.uv).y * 0.5;
    // Its last instruction writes the alpha too.
    let light = l.hilight * through + l.diffuse;
    return l.specular * 0.1 * c.a + c * light + l.hilight.a;
}

@fragment fn finalLeavesTransp(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let light = l.diffuse.rgb + 0.3 * l.hilight.rgb;
    return vec4f(c.rgb * light + 0.1 * l.specular.rgb * c.a, 1.0);
}

@fragment fn finalBranch(p: Point) -> @location(0) vec4f {
    let l = lit(p, 1.0, false);
    let c = textureSample(color, linear, p.uv);
    let light = c.rgb * l.diffuse.rgb + l.hilight.rgb * p.edge.y;
    return vec4f(l.specular.rgb * 0.1 + light, 1.0);
}

@fragment fn _texture(p: Point) -> @location(0) vec4f {
    return textureSample(color, linear, p.uv);
}

// Hair: lines, which the original draws one pixel wide.
struct Segment {
    @location(0) position_a: vec3f,
    @location(1) tangent_a: vec3f,
    @location(2) normal_a: vec3f,
    @location(3) color_a: vec2f,
    @location(4) position_b: vec3f,
    @location(5) tangent_b: vec3f,
    @location(6) normal_b: vec3f,
    @location(7) color_b: vec2f,
}

struct Strand {
    @builtin(position) position: vec4f,
    @location(0) uv: vec4f,
    @location(1) view: vec3f,
    @location(2) edge: f32,
}

fn carried(p: vec4f) -> vec3f {
    return vec3f(dot(bones.m[0], p), dot(bones.m[1], p), dot(bones.m[2], p));
}

@vertex fn vs_hair(@builtin(vertex_index) i: u32, s: Segment) -> Strand {
    let ends = array(0u, 0u, 1u, 1u, 0u, 1u);
    let sides = array(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
    let second = ends[i] == 1u;
    let a = carried(vec4f(s.position_a, 1.0));
    let b = carried(vec4f(s.position_b, 1.0));
    let world = select(a, b, second);
    let tangent = carried(vec4f(select(s.tangent_a, s.tangent_b, second), 0.0));
    let normal = carried(vec4f(select(s.normal_a, s.normal_b, second), 0.0));
    let view_projection = frame.projection * frame.view;
    let clip_a = view_projection * vec4f(a, 1.0);
    let clip_b = view_projection * vec4f(b, 1.0);
    let line = (clip_b.xy / clip_b.w - clip_a.xy / clip_a.w) * frame.size.xy;
    let across = vec2f(-line.y, line.x) / max(length(line), 1e-6);
    var out: Strand;
    out.position = select(clip_a, clip_b, second);
    out.position += vec4f(across * sides[i] * frame.size.z / frame.size.xy * out.position.w, 0.0, 0.0);
    out.view = normalize(world - frame.eye.xyz);
    let t = 0.5 * dot(tangent, -out.view);
    out.uv = vec4f(1.0 - t, 1.0 + t, select(s.color_a, s.color_b, second));
    let facing = 1.0 - max(dot(normal, -out.view), 0.001);
    let squared = facing * facing;
    out.edge = squared * squared;
    return out;
}

@fragment fn hair(s: Strand) -> @location(0) vec4f {
    let d = textureSample(bump, linear, s.uv.zw);
    let rim = textureSample(hilight, linear, s.view).rgb * d.y * s.edge * 0.3;
    return vec4f(textureSample(color, linear, s.uv.xy).rgb + rim, d.x);
}

// The background, the badge and the white at the ends of a scene.
struct Screen {
    @builtin(position) position: vec4f,
    @location(0) at: vec3f,
}

fn corner(i: u32) -> vec2f {
    return vec2f(f32(i & 1u) * 4.0 - 1.0, f32(i >> 1u) * 4.0 - 1.0);
}

@vertex fn vs_sky(@builtin(vertex_index) i: u32) -> Screen {
    let c = corner(i);
    let d = frame.sky[0] * c.x + frame.sky[1] * c.y + frame.sky[2];
    // The background's texture coordinates are upside down (`FUN_00451e20`).
    return Screen(vec4f(c, 1.0, 1.0), vec3f(d.x, -d.y, d.z));
}

@fragment fn sky(s: Screen) -> @location(0) vec4f {
    return textureSample(diffuse, linear, s.at);
}

@vertex fn vs_fade(@builtin(vertex_index) i: u32) -> Screen {
    return Screen(vec4f(corner(i), 0.0, 1.0), vec3f(0.0));
}

@fragment fn fade(s: Screen) -> @location(0) vec4f {
    return vec4f(1.0, 1.0, 1.0, frame.fade.x);
}

@vertex fn vs_badge(@builtin(vertex_index) i: u32) -> Screen {
    let at = vec2f(array(0.0, 1.0, 0.0, 0.0, 1.0, 1.0)[i], array(0.0, 0.0, 1.0, 1.0, 0.0, 1.0)[i]);
    let share = frame.badge.xy + at * frame.badge.zw;
    return Screen(vec4f(share * 2.0 - 1.0, 0.0, 1.0), vec3f(at, 0.0));
}

@fragment fn badge(s: Screen) -> @location(0) vec4f {
    return textureSample(color, linear, s.at.xy);
}
