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
and is unchanged but for what Crowd needs (below).

## Crowd

Crowd's `SushiDX.exe` was decompiled with `rea-agents@6.10.1 decompile-all` (Ghidra). Its
`Sushi.ini` sets no `backBufferColorDepth`, so the main buffer has eight bits per channel.

- Soldiers are drawn four to a mesh (`oCrowdSk*.ssh`): the bone index divided by `nBones`
  picks one of four, and `dot(pick, vRandomVec)` is that soldier's random number. It chooses
  the row of the color-range texture in the normal passes and is the color in the
  `RandomVectors` pass.
- `vRandomVec` is `AppUpdate(SOLVectorADB0)`: an object data block of kind 0
  (`SOLVectorADB.cpp`, read at 0x474350) holding four floats and an index, named
  `SOLVectorADB<index>` (format string at 0x54fdd4). Every soldier object has one. Shader
  variables whose `AppUpdate` names an object parameter take its value.
- Vertex element type 7 is four signed 16-bit integers, unnormalised.
- Particles: `NonUniformRandom` values skew towards an end by their exponent (0x495020);
  `Kill` shapes may be inverted and notify a `Collision` emitter, which then spawns
  `NewParticlesPerCollision` particles where each particle died.
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
