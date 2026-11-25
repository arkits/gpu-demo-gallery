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
multipass targets, scene lights, and camera cuts. Dangerous Curves also uses its sampled
irradiance and gradients and renders a dynamic reflection cube. The Assassin uses a ten-bit
main buffer and represents the sampleable shadow map with a single-channel color attachment.

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
