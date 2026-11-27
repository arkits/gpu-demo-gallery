# Sushi 2.0 renderer

This port reads the Direct3D 9 version of ATI's Sushi data. Reverse engineering used
`rea-agents@6.10.1 decompile-all` with Ghidra 12.2 on the three original SushiDX executables.
The file readers are documented in [formats/README.md](formats/README.md).

The engine loads camera segments and scene overrides from `Sushi2.ini`. A small Lua 5
interpreter runs ATI's frame scripts and submits the script's draw commands to wgpu. The shader
compiler translates the archive's shader declarations and HLSL to WGSL, using the vertex stream
layouts in the compiled meshes. Numbered engine parameters bind to the corresponding HLSL
constant arrays.

The main render path uses the original material properties, skeletal and morph animation,
multipass targets, scene lights, and camera cuts. The engine came from the The Assassin port
with what Crowd and Subsurface Scattering need (below).

## Subsurface Scattering

Subsurface Scattering's `SushiDX.exe` is byte for byte Crowd's (SHA-1 274d650c…). It was
decompiled with `rea-agents@6.10.1 decompile-all`; functions the batch skipped were decompiled
one by one in a Ghidra project, and the spherical harmonics code was run under Unicorn
(`_rea/next7/emu.py`) to settle its conventions.

- Every object carries two light transfer blocks, one per `PrtReceiver<k>` of its shader,
  in order; `AppUpdate("PrtCPCABuffer", k, i, ...)` picks block `k`. Each holds the PCA
  clusters (per cluster: the mean, then `4·W` vectors, each red, green, blue of 36
  coefficients), with `W = PCAWeightVectors(W)` of that receiver. The statue streams its
  weights per vertex; the temple samples them from seven signed textures (`S8888`,
  Q8W8V8U8) per texel.
- The buffer is set at 0x45c7f0 from per-cluster float4s made at 0x45b010: the mean's light
  as RGB, then for red, green and blue the light of each PCA vector, four to a float4.
- Light sources (0x458b50 parses them, 0x45b010 computes them each frame, always in the
  object's space, using the rigid inverse of its first bone):
  - `Runtime`: the block's named scene lights (`KEYShape`, `FILLShape`, `RIMShape`) as
    directional lights from the centroid towards each light, coloured by color × intensity,
    with `AtiSHEvalDirectionalLight` (0x450230; normalised by 4π, 4π/3, 16π/17 or 32π/31 by
    order). `Centroid x y z` replaces the object's bounding centre.
  - `Cubemap`: the environment projected by the exporter and stored in the block, turned
    into object space with `AtiSHRotate` (0x44f570). Under emulation it moves light at d to
    d·M, as a Direct3D row vector.
  - Then `Scale s` multiplies, and `Rotate x y z` turns by the axis-angle vector: its matrix
    (0x44d2c0) is transposed (0x44cf60) before `AtiSHRotate`, which makes the temple's sky
    light turn 110° about y, as the sky shader turns its cube map.
- ATI's SH basis is the one of D3DX (Condon–Shortley phase, √2 for m ≠ 0).
- The statue lerps its two transfers by its own scale: `saturate((_m00 − 0.1) · 1.031)`.
- Sushi binds the registers of `sampler t[n]` to textures `t0`..; a lone `sampler tSky`
  takes `tSky0`. `tex2Dbias` on such arrays adds the bias to the sampler's own.
- An HLSL array of one (`SI_SKINNING_MAX_BONES 1`) is still filled from the replicated
  parameters `mSiWorld0`...
- `control` lines (parsed at 0x426cdb) are the interactive cameras: `LookAt`/`LAt`,
  `Dist`/`D`, `Pitch`/`P`, `Yaw`/`Y` as lowest, highest and start (the middle if missing),
  `FOV`, `FDist`, `FStop`, `FLength` and `Seq` (the script it belongs to; any if missing,
  0x416ea0). The camera sits at the point plus the distance along
  (−sin yaw·cos pitch, sin pitch, cos yaw·cos pitch) and looks back at it (0x415520,
  0x4138b0).

## Crowd

- Soldiers are drawn four to a mesh (`oCrowdSk*.ssh`): the bone index divided by `nBones`
  picks one of four, and `dot(pick, vRandomVec)` is that soldier's random number.
- `vRandomVec` is `AppUpdate(SOLVectorADB0)`: an object data block of kind 0
  (`SOLVectorADB.cpp`, read at 0x474350) of four floats and an index. Shader variables whose
  `AppUpdate` names an object parameter take its value.
- Vertex element type 7 is four signed 16-bit integers, unnormalised.
- Particles: `NonUniformRandom` values skew towards an end by their exponent (0x495020);
  `Kill` shapes may be inverted and notify a `Collision` emitter.
- Music: `Audio/SceneIntro.wav`, then `Audio/SceneLoop.wav` repeating, both 16-bit PCM;
  `audioIntroOverlap` is 0.

## Shared engine

Things that are easy to get wrong, with where the original does them (Double Cross's program):

- A draw's bone palette has up to 100 places (`SiSkinning.shl`); hair uses 80.
- A mesh with weighted shapes is the mesh itself, then what each shape adds. The strongest
  shapes are bound after the mesh (0x46eb70).
- A parameter record of kind 0 means the shader's own value. Animated ones blend and repeat
  (0x455a60). Meshes set them by the name of the parameter, not of the record.
- `Time` and `ObjectTime` are fraction, seconds of two minutes, sine and seconds (0x435600).
- Visibility and other tracks, and bounds, repeat after a length of their own.
- Meshes of an animation replace the object's (0x4536f0).
- `ObjectAmbient` numbers count the lights that reach the object, at most five (0x419130).
- `DrawReflections` clears to black, and shows scene and objects by default (0x49b850).

Particle changes are stepped at a fixed rate for repeatable seeking. The sampling of particles,
motion blur, and some lighting operations is an approximation of the Direct3D renderer.
The player has its own controls and uses the archive's playable chapters. The original
installer, menus, and legacy wireframe variants are outside the playback path.
