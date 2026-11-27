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

`vp30` programs ship as Cg source and `fp30` ones as assembly or Cg;
`shaders/timbury.wgsl` hand-ports the ones the scenes use, one vertex program
per use and a fragment function per program.

- Joint matrix: T · jointOrientation · rotation · rotationOrientation · S, as
  in the earlier viewers. `test.nmb` holds no animation of its own; the dance
  (283 transform channels, 67 morph-weight streams) comes from
  `animation.nmb`.
- Timbury's hair (`hair.nma` + `hair_cvs.txt` guides) is absorbed into his
  model (`Model::absorb`), rebinding its missing transform to the head joint
  `BL_Head_J` [I]. Hair lines (`FUN_0049b310`) widen into quads in the vertex
  shader with a screen-space width, one pixel-ish tall at 768p.
- Grass (`grass.nma`) roots on the ground mesh its geometry names
  (`grassTerrainGeom`), falling back to the set's earth; the life-map gates
  density, white meaning alive [I]. Blade count scales with the file's
  density, sizes with its blade size.
- The swarm draws all 40 bugs from the body's `default_group`, each placed by
  `Show::bugs` along its own flight loop and facing along it.
- HDR is approximated: the scene renders LDR `Rgba8Unorm` throughout (for
  WebGL2 safety) with MSAA resolve, a fixed exposure instead of measured
  auto-exposure, then quarter-size downsample + blur passes and an additive
  glow composite. Bump maps upload signed.
- Texture row 0 is the top of the picture, so the fullscreen triangle samples
  v=1 at NDC y=-1 for an upright copy; the blur kernel is symmetric, so the
  mirrored sample there does not matter [I].
- Cube maps are crosses cut as `FUN_00460a60` cuts them: +X, -X, +Y, -Y, +Z,
  -Z. The fullscreen passes use a fullscreen triangle.
- Order: sky and set draw as themselves, then Timbury's `nvPrimaryRenderPass`
  group plus absorbed hair, then the swarm.
- Blending follows the filed `BlendParams`, resolved by name: (4, 5) is
  standard alpha (`AlphaBlendProxy`, bug wings), (1, 1) is additive
  (`wetballBlendParamsProxy`, the cornea gloss), (0, 2) is multiplicative
  (`darkballBlendParamsProxy`, the lid shadow); files without a proxy keep
  the old `_BlendAlpha`/`_BlendAdd` name checks (hair). The wet cornea
  layer draws `eyeballGloss` (specular highlight only) additively over the
  textured ball, and the lid-bits card draws `eyeballShadow`
  (`modulate_by` + (1 - `modulate_by`) * bits) multiplicatively over it;
  rendering either opaque covers the iris with a white shell. [I] for the
  exact original order.
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
