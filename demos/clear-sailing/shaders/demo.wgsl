// Clear Sailing programs, by hand from the demo's Cg 1.x `vp30`/`fp30`
// listings (never compiled here): `sky` from `_bkg`/`_texcube`, `ocean`
// from `ocean`, `ship` and `sail` from `sails` (Blinn, bump, cube, sun,
// water, fog), `rope` from `rope`, `smoke` from `smoke`, `spray` from
// `spray`, `bright`/`blur`/`comp` from `comp`/`blur1`/`blurxAlphaMul`,
// `ui` for the menus. Vertices arrive in clip space; see `Params` below.
//
// params:
//   0 camPos.xyz, hdr on
//   1 sunDir.xyz, sun intensity
//   2 fog color, fog amount
//   3 fog near, fog far, time, foaminess
//   4 lightingAmounts sun, cube, water, ambient
//   5 sails lightingAmounts sun, cube, water, ambient
//   6 smoke color, smoke alpha scale
//   7 rope color, rope alpha scale
//   8 reflection matrix row 0
//   9 reflection matrix row 1
//   10 reflection matrix row 2
//   11 reflection matrix row 3
//   12 view-projection row 0 (vsmotion)
//   13 view-projection row 1
//   14 view-projection row 2
//   15 view-projection row 3
//   16 ship motion row 0
//   17 ship motion row 1
//   18 ship motion row 2
//   19 blur step (xy horizontal, zw vertical)

// Static meshes ride the ship's rigid motion in the vertex program.
@vertex
fn vsmotion(i: In) -> V {
    var o: V;
    let m = vec4<f32>(
        dot(params.v[16], i.position),
        dot(params.v[17], i.position),
        dot(params.v[18], i.position),
        1.0);
    o.clip = vec4<f32>(
        dot(params.v[12], m),
        dot(params.v[13], m),
        dot(params.v[14], m),
        dot(params.v[15], m));
    o.color = i.color;
    o.uv0 = vec4<f32>(m.xyz, 1.0);
    let n = vec3<f32>(
        dot(params.v[16].xyz, i.uv1.xyz),
        dot(params.v[17].xyz, i.uv1.xyz),
        dot(params.v[18].xyz, i.uv1.xyz));
    let t = vec3<f32>(
        dot(params.v[16].xyz, i.uv2.xyz),
        dot(params.v[17].xyz, i.uv2.xyz),
        dot(params.v[18].xyz, i.uv2.xyz));
    let b = vec3<f32>(
        dot(params.v[16].xyz, i.uv3.xyz),
        dot(params.v[17].xyz, i.uv3.xyz),
        dot(params.v[18].xyz, i.uv3.xyz));
    o.uv1 = vec4<f32>(n, i.uv1.w);
    o.uv2 = vec4<f32>(t, 0.0);
    o.uv3 = vec4<f32>(b, 0.0);
    return o;
}

// Sky: the sunset cube in the view direction.
@fragment
fn sky(i: V) -> @location(0) vec4<f32> {
    let dir = normalize(i.uv0.xyz);
    return vec4<f32>(textureSample(cube, cube_s, dir).rgb, 1.0);
}

// Ocean: sky and ship reflection under chop, foam, and fog (`ocean.fp30`).
@fragment
fn ocean(i: V) -> @location(0) vec4<f32> {
    let world = i.uv0.xyz;
    var norm = normalize(i.uv1.xyz);
    let eyedir = normalize(params.v[0].xyz - world);
    // Fine ripple from the noise picture, as in `ocean.fp30`.
    let noise = textureSample(t2, s2, 0.0005 * world.xz).rgb * 2.0 - 1.0;
    norm = normalize(norm + 0.35 * noise);
    var refl = eyedir - 2.0 * dot(norm, eyedir) * norm;
    // Ship reflection, drawn beforehand into t0.
    let proj = vec4<f32>(
        dot(params.v[8], vec4<f32>(world, 1.0)),
        dot(params.v[9], vec4<f32>(world, 1.0)),
        dot(params.v[10], vec4<f32>(world, 1.0)),
        dot(params.v[11], vec4<f32>(world, 1.0)));
    var ship = textureSample(t0, s0, proj.xy / max(proj.w, 1e-4) * vec2<f32>(0.5, -0.5) + 0.5);
    ship.a = 4.0 * ship.a * ship.a;
    refl.y = -refl.y;
    var sky = textureSample(cube, cube_s, normalize(refl)).rgb;
    sky = ship.a * ship.rgb + clamp(1.0 - ship.a, 0.0, 1.0) * sky;
    let c = dot(norm, eyedir);
    let r = 0.5 * clamp(1.0 - c, 0.0, 1.0);
    var water = r * sky + (1.0 - r) * vec3<f32>(0.0, 0.1, 0.15);
    // Chop and foam. uv2.x carries the wake behind the ship.
    let chop = i.uv1.w;
    let turb = textureSample(t1, s1, 0.001 * world.xz).r;
    let foaminess = params.v[3].w;
    var foam = clamp(chop * turb * (0.5 + foaminess), 0.0, 1.0);
    let spray = textureSample(t2, s2, 0.02 * world.xz + 0.01 * params.v[3].z).g;
    foam = clamp(foam + i.uv2.x * spray * 2.0, 0.0, 1.0);
    var color = foam + (1.0 - foam) * water;
    let dist = length(world - params.v[0].xyz);
    let fog = clamp((dist - params.v[3].x) / max(params.v[3].y - params.v[3].x, 1e-3), 0.0, 1.0) * params.v[2].w;
    color = fog * params.v[2].xyz + (1.0 - fog) * color;
    return vec4<f32>(color, 1.0);
}

// Ship and sails: Blinn with bump and cube maps, sun, water bounce, fog.
fn lit(i: V, amounts: vec4<f32>) -> vec4<f32> {
    let world = i.uv0.xyz;
    let eyedir = normalize(params.v[0].xyz - world);
    var norm = normalize(i.uv1.xyz);
    if (dot(norm, eyedir) < 0.0) {
        norm = -norm;
    }
    let bump = textureSample(t1, s1, i.color.xy).rgb - 0.5;
    norm = normalize(norm + 5.0 * (bump.x * normalize(i.uv2.xyz) + bump.y * normalize(i.uv3.xyz)));
    let refl = reflect(-eyedir, norm);
    let fix = vec3<f32>(-1.0, 1.0, -1.0);
    let skydiff = textureSample(cube, cube_s, fix * norm).rgb * amounts.y;
    let skyspec = textureSample(cube, cube_s, fix * refl).rgb * amounts.y;
    let sundir = normalize(params.v[1].xyz);
    let suncol = vec3<f32>(0.8, 0.6, 0.4) * amounts.x * params.v[1].w;
    var sundotnorm = dot(norm, sundir);
    let vdotl = dot(eyedir, sundir);
    let through = -0.5 - 2.5 * vdotl * vdotl * vdotl * vdotl * vdotl * vdotl;
    sundotnorm = select(sundotnorm, sundotnorm * through, sundotnorm < 0.0);
    let sundotrefl = max(dot(refl, sundir), 0.0);
    let sunlit = vec3<f32>(1.0, max(sundotnorm, 0.0), pow(sundotrefl, 10.0));
    let watercol = vec3<f32>(0.4, 0.4, 0.5) * amounts.z;
    let waterdiff = watercol * clamp(dot(norm, vec3<f32>(0.0, -1.0, 0.0)), 0.0, 1.0);
    let colormap = textureSample(t0, s0, i.color.xy);
    let ambmap = textureSample(t2, s2, i.color.xy).rgb * amounts.w;
    let diffuse = skydiff + sunlit.y * suncol + waterdiff;
    let specular = 0.1 * skyspec + sunlit.z * suncol;
    var color = colormap.rgb * (ambmap + diffuse) + colormap.a * specular;
    color *= 0.5;
    let dist = length(world - params.v[0].xyz);
    let fog = clamp((dist - params.v[3].x) / max(params.v[3].y - params.v[3].x, 1e-3), 0.0, 1.0) * params.v[2].w;
    color = fog * params.v[2].xyz + (1.0 - fog) * color;
    return vec4<f32>(color, 1.0);
}

@fragment
fn ship(i: V) -> @location(0) vec4<f32> {
    return lit(i, params.v[4]);
}

@fragment
fn sail(i: V) -> @location(0) vec4<f32> {
    return lit(i, params.v[5]);
}

// Ropes: flat color, fading with distance (`rope.fp30`).
@fragment
fn rope(i: V) -> @location(0) vec4<f32> {
    return vec4<f32>(params.v[7].rgb, i.color.w * params.v[7].w);
}

// Smoke: the sprite, aging out (`smoke.fp30`).
@fragment
fn smoke(i: V) -> @location(0) vec4<f32> {
    let tex = textureSample(t0, s0, i.uv0.xy);
    let a = tex.r * (1.0 - i.uv0.z) * params.v[6].w;
    return vec4<f32>(params.v[6].rgb * (1.0 - i.uv0.y), a);
}

// Spray: the foam picture, additive-ish (`spray.fp30`).
@fragment
fn spray(i: V) -> @location(0) vec4<f32> {
    let tex = textureSample(t0, s0, i.uv0.xy);
    return vec4<f32>(tex.rgb, 0.237 * tex.a * i.color.w);
}

// Bright pass: what is over 1, for the HDR glow.
@fragment
fn bright(i: V) -> @location(0) vec4<f32> {
    let c = textureSample(t0, s0, i.uv0.xy).rgb;
    let over = clamp((c - 0.85) * 4.0, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(over * c, 1.0);
}

// One 5-tap Gaussian line; params.v[19] holds the step.
@fragment
fn blur(i: V) -> @location(0) vec4<f32> {
    let d = params.v[19].xy + params.v[19].zw;
    let uv = i.uv0.xy;
    var c = 6.0 * textureSample(t0, s0, uv).rgb;
    c += 4.0 * (textureSample(t0, s0, uv - d).rgb + textureSample(t0, s0, uv + d).rgb);
    c += textureSample(t0, s0, uv - 2.0 * d).rgb + textureSample(t0, s0, uv + 2.0 * d).rgb;
    return vec4<f32>(c / 16.0, 1.0);
}

// Composite: screen blend of the scene and its glow (`comp.fp30`).
@fragment
fn comp(i: V) -> @location(0) vec4<f32> {
    let scene = textureSample(t0, s0, i.uv0.xy).rgb;
    let glow = 0.75 * textureSample(t1, s1, i.uv0.xy).rgb;
    return vec4<f32>(scene + glow - scene * glow, 1.0);
}

// Menus: a picture, tinted.
@fragment
fn ui(i: V) -> @location(0) vec4<f32> {
    return i.color * textureSample(t0, s0, i.uv0.xy);
}
