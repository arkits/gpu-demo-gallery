# Mad Mod Mike engine notes

From static analysis of `bin/MadModMike.exe` (2.9 MB, 32-bit x86, NVDemo
"RocketBubba", PE timestamp 2003) with REA and Ghidra
(`npx -y rea-agents@6.10.1 inspect` then `decompile-all`; evidence
`ev_2007577b6122b1231f90713e711df56c143b8fcba13c5a2b811c30000`; 10,779
functions decompiled, PDB `RocketBubba.pdb` referenced but not loaded). The
program was never run. Addresses below are file addresses (`0x401000` image
base). [I] marks inference; the rest was read in code or checked against the
shipped files.

## Files

`MadModMikeSetup.exe` (153 MB, SHA-256
`9d99e32451ccd93ac8cf784f95eb50df8dc6b239ff1f3530b82864357905eb2d`) is an
InstallShield package. Unpacked, `files/` holds `bin/MadModMike.exe`
(SHA-256 `37fa2497caaa8b3fec7471835cd7199976fbf01768ce417d7c7a01393cf0b875`),
`models/*.nmb` (binary scenes) and `*.nma` (text scene fragments),
`shaders/*.fp30|*.vp30` plus `shaders/custom/*`, 448 textures (DDS, TGA, T3D),
`audio/*.wav`, and `bin/*.txt` parameters. The port reads every scene file to
its last byte (`src/tree.rs`).

Binary nodes start with a kind id, then fields tagged by schema index, `-3`
name, `-4` end. The tables come from the viewer's registration code, e.g.
`0x4d17d0` (`Scene`), `0x50f190` (`PointLight`), `0x4f07f0` (`ProcessedMesh`,
id `0x70000038`), `0x504270` (`MayaFloatChannel`), `0x4f6f80` (`Shader`);
field-type vtables cluster as: strings (`0x5fa79c`), floats (`0x5f4b58`),
ints (`0x5fae0`, `0x5fc214`), bools (`0x5f4b80`), vectors (`0x5f4b30`,
`0x5faf9c`), single nodes (`0x5fe5a4`), node lists (`0x5fd2xx`), words
(`0x5fda28`, `0x5fda00`), enums by registration order.

- `Scene` fields, in order: sounds, backgrounds, lights, cameras, shaders,
  textures, renderParams, renderSettings, renderPasses, transforms, shapes,
  geometries, animations, menus, deformers. `scene2.nmb` … `scene26.nmb` hold
  one `Animation` each under `animations`; `geom.nmb` holds the world.
- `PointLight` (`0x30000002`): transform, color (3 floats), intensity, isEyespace,
  position, attenuation.
- `ConstantParam` (`0x41000001`): paramName, then an animation-set index and a
  four-float value (`SetVec`), then min and max vectors.
- `TransformParam` `map` values, cross-checked between `.nma` words and binary
  ints: 1 proj, 4 view_proj, 5 view_proj_win, 6 viewInv, 7 xform, 9
  xform_view_proj, 23 skinned model, 26 skinned model-view-proj, 43 eyeToEye [I
  for view, xform array variants].
- `LightParam` property: 0 color, 1 position, 2 direction [I for 2]; space is 1
  for world.
- `MayaFloatChannel` is `(targetNodes, targetFields, curve)` in text but has a
  trailing `index` in binary; `ConstantParam.value` is a bare vector in text and
  `(set, vector)` in binary. `src/tree.rs` keeps both (`TEXTKINDS`).
- Keyframes are 32 bytes: time, value, in/out tangent-kind ints (skipped),
  then in/out tangent pairs. `interpMethod`: 1 linear, 2 hermite (`poses.nma`
  writes those words for those values), 0 Bezier. `preStart`/`postStop`: 0
  hold, 1 linear extrapolation, 2/3 cycle [I for 1, from Maya's infinity types].
- `primType`: 3 triangle lists, 5 strips with `0xffff` restarts, 8 quad patches
  (groups of 3 or 4 indices plus restart) [I for 8, from index topology: quad
  fans of the subdivision cages].
- Textures: `2DTexture` format 25 means default; DXT1/5, L8A8 bump maps
  (16-bit luminance-alpha, decoded to RGBA) and RLE TGAs are all read.
  `3DTexture` (`.t3d`) is a 64³ byte volume plus a 16-byte header. `CULL` 1
  draws both sides, 2 culls back faces [I, from which shapes use which].
- `particles.nma` ends with a corrupt two-token tail (`0`, `o3`, line 928)
  after its closed `Scene`; the reader accepts exactly that tail [I: truncated
  write in the shipped file].

## Drawing

`shaders/mad-mod-mike.wgsl` reimplements the Cg programs by hand; the table is
in `docs/DEVELOPMENT.md`.

- Skinned meshes carry `model[n]` (map 23) bone slots on their geometry's
  `processParams`; shadow-caster variants carry `modelViewProj[n]` (map 26) on
  the shape. The port skins every color pass on the GPU (up to 8 influences
  from `boneWeights0/1`, `boneIndices0/1`); the bone palette uploads full
  column-major matrices to match the shader's `array<mat4x4>` layout
  (uploading transposed 3-row matrices instead explodes every skinned mesh
  while leaving unweighted static geometry untouched). Vertices with no
  weights filed are rigidly carried by the single bone (helmet, teeth,
  eyeballs, rocket pack); bend is `world · rest⁻¹`.
- Morphs are `objCoordDiffN`/`objNormalDiffN` streams driven by
  `<geometry>_blend_0_<N>` weight parameters; the first eight weights over
  0.01 apply on the CPU. The jaw is a joint plus blend shapes [I for the jaw].
- Hair grows from `hair_curves.txt` (212 quadratic guides) resampled per
  `hairclumpsparams.txt` (`Numinterp` strands a guide, jittered by `Radius`),
  and the goatee from `gotti_curves.txt` (10 cubic guides) with
  `gotti_params.txt`. The strands follow the head joint as camera-facing
  ribbons. The original's gravity, drag and relaxation
  (`hairparams.txt`) are not simulated.
- Particles (`particles.nma`, eight systems): billboards around each emitter's
  rest transform, animated by the `time` uniform; smoke samples the `.t3d`
  volumes. Streaks and point sprites are points [I].
- The player draws opaque shapes, then sorts transparent shapes back to
  front. It uses the authored near color groups for the room and workshop.
  The original's far, shadow, and reflection passes supply intermediate
  buffers for its compositor.
- Lights: a fixed rig (moon, omni, lamp, mermaid colors from the file) plus
  0.55 ambient [I: the original bakes most lighting into its textures and
  light maps, whose exact combination per `mike*.fp30` is approximated].

## Running

- Ten takes play the storyboard windows from `bin/args.txt` (`consume_anim`:
  start frame at 30 fps plus length in seconds; `demo::TAKES` follows it, so
  each take plays its own master-timeline segment and loops). Takes without
  a storyboard slot (`scene6.nmb`) play their key span [I]. `anim_reset.nma`
  and `poses.nma` parse but are not performed [I: reset pose and pose cycling].
- The movie uses the take's camera rig from `args.txt` flag 6 and the
  original's aspect-scaled lens. Only the active set and props draw, using
  the transform hierarchy and storyboard flags. The two old-computer cuts
  use an orbit shot when their rig crosses the computer wall. The unrigged
  `scene6.nmb` take follows Mike from a shipped preset angle. The flyaway
  uses `cornerCam` for departure, then follows Mike against the shipped moon
  backdrop.
- `c` switches between movie, free orbit around Mike's upper body, and the
  head-mounted helmet view. Orbit placement keeps the lens in front of opaque
  walls and props. Scripted and helmet views check the immediate
  foreground for clearance. `1`–`0` switch takes, arrows change expressions,
  space pauses, and `m` toggles the menu overlay.
- The wet cornea layer (`mike_wetball_*`, opaque-blended by default in the
  files) draws as a transparent analytic gloss over the textured eyeball;
  drawn opaque it covers the iris with a white shell.
- Expressions: `BlendCombos.txt`'s seven combos of five blend targets and a
  jaw angle; `↑`/`↓` step through them.
- Sound: `audio/soundtrack.wav` loops (FLAC on the web).

## Left out or approximate

- Shadow maps (hard omni, soft cube, radiance), depth of field, HDR glow,
  cube-Blur reflections, adaptive subdivision (control cages draw as quads),
  feedback/HDTV post chain, the menu buttons (drawn, not clickable), eye
  tracking, helmet, sparks physics, cloth physics, wireframe/point modes, the
  artist menu, screenshots and gamma control.
- Camera clearance samples the active set’s opaque geometry, including the posed door frame. The movie uses replacement framing at the computer wall and during the flyaway.
- `rot
...[truncated 737 chars]