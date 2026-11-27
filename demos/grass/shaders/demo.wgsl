// params.v[0..4]: matrix. v[4]: light, time of the clouds' shadows. v[5]: to the eye's back,
// how much of the shadows. v[6]: eye, whether lit. v[7]: fog, how much of it.
// v[8..12]: place in the eye's space, for what mirrors.
fn matrix() -> mat4x4<f32> {
    return mat4x4(params.v[0], params.v[1], params.v[2], params.v[3]);
}

struct Blade {
    // Foot, width.
    @location(6) foot: vec4<f32>,
    // Color, change of the width.
    @location(7) color: vec4<f32>,
    // Direction, height of it, and their changes, for each part.
    @location(8) turns: vec4<f32>,
    // Length of a part.
    @location(9) length: vec4<f32>,
    @location(10) unused0: vec4<f32>,
    @location(11) unused1: vec4<f32>,
};

// position: part, side, number of parts (0x403080).
@vertex
fn blade(i: In, b: Blade) -> V {
    let parts = i32(i.position.x);
    var at = b.foot.xyz;
    var turn = b.turns.xy;
    for (var k = 0; k < parts; k++) {
        at += b.length.x * vec3(cos(turn.y) * cos(turn.x), sin(turn.y), cos(turn.y) * sin(turn.x));
        turn += b.turns.zw;
    }
    let along = vec3(cos(turn.y) * cos(turn.x), sin(turn.y), cos(turn.y) * sin(turn.x));
    let across = cross(params.v[5].xyz, along);
    let width = b.foot.w + b.color.w * i.position.x;
    at += across * width * i.position.y;
    var o: V;
    o.clip = matrix() * vec4(at, 1.0);
    // A hair is lit as if its normal leaned to the light as far as it can.
    let light = params.v[4].xyz;
    let n = normalize(light - along * dot(light, along));
    let shine = max(dot(n, normalize(light + params.v[5].xyz)), 0.0);
    o.color = vec4(min(vec3(0.3) + max(dot(n, light), 0.0) * b.color.rgb, vec3(1.0)), 1.0);
    o.uv0 = vec4(i.position.x / i.position.z, i.position.y * 0.5 + 0.5, 0.0, 1.0);
    o.uv1 = vec4(at, 1.0);
    o.uv2 = vec4(vec3(0.5) * pow(shine, 128.0), 0.0);
    o.uv3 = vec4(0.0);
    if params.v[6].w == 0.0 {
        o.color = vec4(1.0);
        o.uv2 = vec4(0.0);
    }
    return o;
}

// What stands still. uv1: normal.
@vertex
fn ground(i: In) -> V {
    var o: V;
    o.clip = matrix() * i.position;
    let d = max(dot(normalize(i.uv1.xyz), params.v[4].xyz), 0.0);
    o.color = vec4(mix(vec3(1.0), min(vec3(0.3 + d), vec3(1.0)), params.v[6].w), i.color.a);
    o.uv0 = i.uv0;
    o.uv1 = i.position;
    o.uv2 = vec4(0.0);
    o.uv3 = vec4(0.0);
    return o;
}

// `GL_SPHERE_MAP`.
@vertex
fn mirror(i: In) -> V {
    var o: V;
    o.clip = matrix() * i.position;
    let eye = mat4x4(params.v[8], params.v[9], params.v[10], params.v[11]);
    let u = normalize((eye * i.position).xyz);
    let n = normalize((eye * vec4(i.uv1.xyz, 0.0)).xyz);
    let r = reflect(u, n);
    o.color = vec4(1.0);
    o.uv0 = vec4(r.xy / (2.0 * length(r + vec3(0.0, 0.0, 1.0))) + 0.5, 0.0, 1.0);
    o.uv1 = vec4(0.0);
    o.uv2 = vec4(0.0);
    o.uv3 = vec4(0.0);
    return o;
}

// t0: picture, t1: shadows of clouds. uv1: place, uv2: highlight.
fn outdoors(i: V, t: vec4<f32>) -> vec4<f32> {
    let place = i.uv1.xyz;
    let move_by = params.v[4].w * 40.0;
    let cloud = textureSample(t1, s1, 0.01 * (place.xz + vec2(move_by, -move_by))).rgb;
    let shade = mix(vec3(1.0), cloud, params.v[5].w);
    let c = t.rgb * i.color.rgb * shade + i.uv2.rgb;
    let fog = clamp((distance(place, params.v[6].xyz) - 25.0) / 25.0, 0.0, 1.0) * params.v[7].w;
    return vec4(mix(c, params.v[7].rgb, fog), t.a * i.color.a);
}
@fragment
fn field(i: V) -> @location(0) vec4<f32> {
    return outdoors(i, textureSample(t0, s0, i.uv0.xy));
}
@fragment
fn flower(i: V) -> @location(0) vec4<f32> {
    let t = textureSample(t0, s0, i.uv0.xy);
    let c = outdoors(i, t);
    if t.a <= 0.5 { discard; }
    return vec4(c.rgb, 1.0);
}
@fragment
fn round(i: V) -> @location(0) vec4<f32> {
    let d = length(i.uv0.xy);
    return vec4(i.color.rgb, smoothstep(1.0, 0.7, d));
}

// Four layers of noise, each half as strong and twice as fine (0x4086b0).
// params.v[4 + k]: cosine, sine and the move of a layer.
@fragment
fn clouds(i: V) -> @location(0) vec4<f32> {
    var sum = vec3(0.0);
    var strength = 0.5;
    var scale = 1.0;
    for (var k = 0; k < 4; k++) {
        let m = params.v[4 + k];
        let p = i.uv0.xy + m.zw;
        let uv = scale * vec2(m.x * p.x - m.y * p.y, m.y * p.x + m.x * p.y);
        sum += strength * textureSample(t0, s0, uv).rgb;
        strength *= 0.5;
        scale *= 2.0;
    }
    return vec4(sum, 1.0);
}
