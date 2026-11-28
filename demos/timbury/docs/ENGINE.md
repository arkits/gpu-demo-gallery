# Timbury engine notes

From static analysis of `bin/BugLooker.exe` (SHA-256
`14dfe6e6c90f5a93d2e624448ef0f8e987bf29c9915db7b3a0ee6f0b0b9d7`) with Ghidra
through `npx -y rea-agents@6.10.1 inspect` and `decompile-all` (8561 functions
in `/tmp/timbury-decompiled`), and from the demo's files. The program was never
run; `JAVA_HOME=/opt/homebrew/opt/openjdk@25` and
`GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec` were needed for REA.
[I] marks inference; the rest was read in code or checked against the data.

## Files

`args.txt` (and the identical `args_lite.txt` but for the window size and the
lite flag) lists the scene files in order: Timbury (`models/test.nmb`), his
hair (`models/hair.nma`), the animation (`models/animation.nmb`), grass
(`models/grass.nma`), the blur passes (`models/blur.nma`), the background
(`models/bkg_split.nma`), the menu (`models/menu.nma`), the swarm bug model
(`models/swarmbug.nma`), whether to combine bug geometry with animation,
the swarm bug animation (`models/swarmbug_anim.nma`), and the bug count (40).
The grass terrain (`models/grass_terrain_geom.nma`) and hair guides
(`models/hair_cvs.txt`) are named by geometries. `src/demo.rs` holds this list.

`src/tree.rs` reads the strict `.nma` text and `.nmb` binary readers, each to
the last byte/word and validated against each other, from the viewer's own
registrations (`FUN_004b6ea0` and friends). The binary uses kind, numbered
fields, -3 names and -4 ends; strips (5) restart at 0xffff, and bone index -1
is unused.

- Matrix properties: 7 carries a mesh, 23 bends it (joint matrix times the
  inverse bind); 26/9 is modelViewProj, 1 proj, 4 view, 5 view_proj_win,
  6 viewInv, 43 eyeToEye.
- Vertex arrays name the shader input they feed (`a2v.coord`,
  `a2v.coordMorph17`); morph weights stream as `morphWeightN` and map
  index-wise onto `objCoordDiffN`. Shapes with no weights stay rigid.
- Key frames are 8 floats: time, value, two tangent kinds, then in/out
  tangents as slopes (Hermite, as in Dawn's `FUN_00468d60`). Curve time is
  seconds; `timeScale` scales it.
- DDS and TGA rows start at the bottom and are given to the GPU as they are.
  The UI `.dds` files display mirrored in PIL/ffmpeg but render correctly in
  the GPU path with plain UVs.
- `exr.rs` reads PIZ EXR through the `exr` and `half` crates rather than
  hand-rolling PIZ.

## Drawing

`vp30` programs ship as Cg source and `fp30` ones as assembly or Cg (in the
installer's `shaders/`); `shaders/timbury.wgsl` translates each fragment
program the scenes use line by line. `fragment()` in `src/scene.rs` maps the
filed program name to its entry point (longest prefix first) and a unit test
pins the table:

| Program | Entry | Notes |
|---|---|---|
| bumpedEnv2Tex | fs_env2 | 1-tap hard shadow, clipped tree shadow, spec ramp in bump alpha |
| bumpedEnv2Tex_CLOTHES | fs_env2_clothes | 5-tap hard shadow, no tree shadow, specularColor |
| bumpedEnv2Tex_TREE | fs_env2_tree | no shadow map, facing term only |
| bumpedEnv2Tex_BITE | fs_env2_bite | bite opens (alpha) once g_bitten is on |
| bumpedEnv2Tex_NO_SHADOW | fs_env2_noshadow | swarm bugs |
| bumpedEnv3Tex(_transp) | fs_env3(_transp) | separate specular map; transp alpha = color alpha |
| bumpedEnv3Tex_SKIN | fs_env3_skin | 5-tap shadow, pink silhouette rim, no occlusion term |
| bumpedEnv3Tex_BUG | fs_env3_bug | iridescent shell from specialTex and rampTex |
| env0Tex / env1Tex_transp / env2Tex | fs_env0 / fs_env1_transp / fs_env2tex | unbumped; plain tree coverage |
| eyeballGloss / eyeballShadow | fs_gloss / fs_shadow | additive cornea, multiplicative lid card |
| glassLenses | fs_glass | refraction of the lens picture, see below |
| hair / grass / _bkg_split | fs_hair / fs_grass / fs_flat | |

- Each shape's ConstantParams become `Params` (diffuseColor, specularScale,
  ambientColor, bump_scale, worldShadowCameraVec, shadow_str, specularColor,
  transparency, rim terms, hard_shadow_inv_texsize, modulate_by, flipUCoord,
  _blur2D steps), with the Cg defaults where none is filed. They are written
  in draw order (sky, set, Timbury, bugs); writing them in another order
  handed every shape another shape's constants.
- Signed pictures (2DTexture format != 24, the `_norm.tga` bump maps) store
  x, y, z and the specular ramp in file byte order: byte 2 (z) averages +1 in
  every one (`signed_maps_face_out_in_file_order`). They upload unswapped
  (`tga::read_raw`) as `Rgba8Snorm`; the programs re-read the ramp with
  `frac(w * 0.5 + 1)`. Their mip levels average x, y, z as signed bytes
  (`gpu::half_level`): averaged unsigned, near-zero values (0 and 255) mean
  +1 and every distant surface turned to noise.
- Joint matrix: T · jointOrientation · rotation · rotationOrientation · S, as
  in the earlier viewers. `test.nmb` holds no animation of its own; the dance
  (283 transform channels, 67 morph-weight streams) comes from
  `animation.nmb`.
- Transform params: `model` (map 7) and a plain `modelViewProj` (9) carry a
  rigid mesh; `model[n]` (23) and `modelViewProj[n]` (26) bend a skinned one.
  The pass copies of rigid shapes name only `modelViewProj`, so 9 bent them
  through the inverse bind pose and, among others, threw a hoop of shadow
  over Timbury's head (`pass_copies_coincide_with_the_visible_shapes`
  checks every shadow and lens copy against its visible twin at t = 25).
- Timbury's hair (`hair.nma` + `hair_cvs.txt` guides) is absorbed into his
  model (`Model::absorb`), rebinding its missing transform to the head joint
  `BL_Head_J` [I]. Hair lines (`FUN_0049b310`) widen into quads in the vertex
  shader with a screen-space width, one pixel-ish tall at 768p.
- Grass (`grass.nma`) roots on the ground mesh its geometry names
  (`grassTerrainGeom`); the life-map gates density, white meaning alive [I].
  The blade geometry is the port's [I]; `grass.fp30` is exact (blade color ×
  picture, both shadows, g_grassBrightScale).
- The swarm draws all 40 bugs from the body's `default_group`, each placed by
  `Show::bugs` along its own flight loop and facing along it [I].
- Cube maps are crosses cut as `FUN_00460a60` cuts them: +X, -X, +Y, -Y, +Z,
  -Z.

- Face culling follows the filed `cullParams`: the built-in `_CullDefault`
  culls back faces, `_CullNoCull` (158 shapes: shirt, shorts, legs, leaves,
  glasses) draws both sides. The shape's own `cull` field is view culling;
  reading it as face culling punched holes in every double-sided surface
  (`double_sided_shapes_draw_both_sides`).
- Draw order: sortSet, then opaque shapes before blended ones, then by the
  distance to the shape's sphere center, opaque near to far (sortType 2 is
  `front_to_back` in the binary's enum: 0 none, 1 back_to_front, 2
  front_to_back) and blended far to near. In file order the left cornea
  (`BL_EYE_Wetball_Left`, additive, depth writing) drew before its ball and
  hid it, leaving that lens showing an empty socket; the right eye is filed
  ball first. Opaque-before-blended is the port's reading [I], matching the
  sibling Mad Mod Mike port.
- The island beetle (`bug__Utar_Bug_*`) is keyed ~14,500 units below the
  island (`bug__Main2` translation y ≈ −14590 under its root's pivot) under
  every transform composition tried, and neither movie camera looks at it:
  it is staged offscreen in the file, not misplaced by the port.

### Passes (test.nmb `renderPasses`, then blur.nma)

1. `ShadowRenderTargetProxy`: `ShadowRenderTargetProxy_group` from
   `ShadowCameraShape` (the root `ShadowCamera` transform, fov 0.307, near
   1400, far 2300) into the 2048² depth map `bumpedEnv2Tex_f_shadowtex`.
   Compared with linear filtering (NV3x shadow PCF), with the filed
   `ShadowRenderTargetProxy_depth` polygon offset (factor 8, units 11).
2. `TreeShadowRenderTargetProxy`: the tree group into a 256² target cleared
   white, writing (0, 0, window depth) (`shadows.fp30` reads WPOS.z).
3. `BlurRenderTargetProxy_A/B`: `_blur2D` along the filed invTexSize ×
   blurAxis, x then y, into `bumpedEnv2Tex_f_softTreeShadowTex`. Programs
   read `soft = saturate(cov + saturate((occluderZ - z) * 99))`.
4. `LensRenderTargetProxy`: `LensRenderTargetProxy_group` (head, brows,
   lashes, wet lines, eye muscles, balls) from `LensCameraShape` (child of
   `BL_Head_J`, fov 0.337, aspect 2, near 0.1, far 100) into the 1024×512
   `glassLenses_f_sceneTex`.
5. `nvPrimaryRenderPass`: sky, grass, `nvPrimaryRenderPass_group` plus hair,
   then the swarm, into a float target (`float4_16` in blur.nma) where the
   adapter can draw, blend and multisample `Rgba16Float`, else `Rgba8Unorm`.
6. blur.nma: `Initial2DBlurPass` (blur_xy, 9 taps at thirds of 1/256, 1/128
   into the 256×128 PrimaryBlurY), PrimaryBlurPassX/Y twice (blur_05 and
   blur_07 with their filed weights), `GetLightLevelPass1` (lightlev, 8×8),
   `GetLightLevelPass2` (lightlev2 into LightLevelTarget2a on odd frames,
   2b on even ones, each reading the other) and `CompositePass`
   (composite.fp30: scene·(1 − g_softness) + blur·g_softness, times the
   measured outputScale and g_userOutputScale). The 2×2 exposure targets
   keep log2 of their values so they survive an 8-bit fallback.

Render targets are sampled with v flipped from the original's OpenGL
coordinates; pictures from files keep the rows the original uploads, so
their UVs are used as filed. The fullscreen passes work in OpenGL texture
coordinates and flip at the sample (`at0`, `at1`).

### The lenses

`glassLenses.fp30` ("refaketion") samples the lens picture at a
per-lens remap of its own UVs: panned by the view direction against
`g_timSideDir`/`g_timUpDir`, ballooned toward the middle when seen head-on
(`g_timLookDir`), mirrored for the right lens (`flipUCoord`), squeezed into
u 0.145–0.375 / v 0.41–0.63, then `v * 2 - 0.5`. The result is
`refl · fresnel + (1 − fresnel)(refracted · 0.8 + add_refl)` with the
reflection cube `envReflectionCubemap`, written opaque (`_BlendDefault`).
0x41a870 sets g_timLookDir/SideDir/UpDir from the normalized, negated Z, X
and Y axes of LensCamera's world matrix, and g_worldCameraPos from the
watched camera.

### Per-frame values (0x41a870, 0x41c840, 0x4115b0)

- Sliders start at 0.465 (output scale), 0.35 (AGC damping), 0.565 (light
  angle), 0.5 (softening amount), 0.5 (softening radius), in menu order.
- Fade f: t/2 over the first two seconds, 1 to 57.3, then (60.9 − t)/3.6;
  the show restarts with its music at 63 s (0x41c7d0).
- g_userOutputScale = (3f² − 2f³)(1 + 0.32·low)(out^2.322·0.83 + 0.05)·4.6296,
  where low = clamp((−100 − cameraY)/492, 0, 1).
- g_softness = soft^1.12·f^2.7 + (1 − f^2.7), so the fades blur too.
- g_mixAmount = 1 on the first frame, then (1 − agc)²·0.5 + 0.12;
  g_wellBehavedness = agc (unused by lightlev2's math).
- _bkg_split g_brightnessScale = 1/(1 + 0.32·low); grass g_grassBrightScale =
  dot(normalize(camera), normalize(shadow camera))·0.12 + 0.21; g_bitten = 1
  after 4.77 s.
- Not ported: the light-angle slider turns a node by (2s − 1)·3.14 about y
  (0x41c840) that the decompile does not name, and 0x413b10 repositions a
  node on camera cuts; the softening-radius slider's use was not found. The
  port keeps the filed ShadowCamera and worldShadowCameraVec [I].
- The player's unseen warm-up draw does not count as the first frame
  (`Scene::reset_exposure`), so the first frame shown sets the exposure
  outright as in the original.

### Other filed state

- The eyeball joints (`BL_EYE_Ball_*`, `BL_EYE_Wetball_*`) rotate about
  their filed pivots at the ball centers: the geometry sits centered on the
  pivot with live rotation tracks, so composing about the joint origin
  swings the whole eye out of its socket mid-dance. Every other transform
  keeps the offset composition: the camera rig's keyed aim matches it
  (Camera1's forward is within a degree of the head at the sampled times),
  while honoring the rig pivots would move Camera1 ~450 units and break
  the authored framing [I].
- Blending follows the filed `BlendParams`, resolved by name: (4, 5) is
  standard alpha (`AlphaBlendProxy`, bug wings), (1, 1) is additive
  (`wetballBlendParamsProxy`, the cornea gloss), (0, 2) is multiplicative
  (`darkballBlendParamsProxy`, the lid shadow); files without a proxy keep
  the old `_BlendAlpha`/`_BlendAdd` name checks; hair always blends.
- Depth follows the filed `depthParams`: the skybox tests but never writes
  (`_DepthNoWrite`), the blur passes skip the test, everything else tests
  and writes.
- The skybox follows the camera: `_bkg_split.vp20` adds `cameraPosition`
  to its vertices, so the ±1000 box stays around the eye. Drawn static, its
  edges show the clear color on the finale's pull-back (the camera key
  jumps ~3000 units out at t≈57); drawn following, the finale wide shot of
  the island stays covered.

## Running

- Movie cameras: `Camera__1_Shape1` (movie 0, lens 0.873) and
  `Camera__1_CUShape` (movie 1, lens 1.222), chosen by keys `1`/`2` and the
  menu's `Camera1CB`/`Camera2CB` (whose button reads "free camera"). `f` frees
  the camera around `BL_Head_J`; dragging takes hold the same way. `Home`
  resets. The render passes name proxy cameras (`DumbCamera` et al.); the
  movie cameras above are the watched ones [I].
- A movie camera that loses the head gets turned back toward it, keeping its
  keyed position: the CU rig (under `Camera_BUG`) and the main rig's keys
  otherwise stare at the sky for seconds at a time, so the original must aim
  them at run time [I].
- The menu overlay (logo, action buttons, five blur/exposure sliders, about
  box) renders in-scene from `menu.nma` and takes clicks and drags; `SlidersCB`
  toggles the slider panel, `PauseCB`/`RestartCB`/`ResetCB`/`QuitCB`/`AboutCB`
  do what they say. `camera2_texture` is `freecamera.dds`.
- Sound: `audio/soundtrack.wav` (packed as FLAC for the web), cued on repeat.
  The web player follows the white level at a tenth where it is zero.
- After 120 s without input the movie returns [I].
- Free camera: yaw/pitch around the head joint with distance clamp
  (nearest 2, farthest 1500); middle-drag pans, right-drag and wheel zoom.
