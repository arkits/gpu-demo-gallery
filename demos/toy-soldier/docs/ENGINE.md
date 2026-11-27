# Toy Soldier engine notes

Source evidence is the extracted demo data and static decompilation of the viewer using
`npx -y rea-agents@6.10.1 decompile-all` with Ghidra. Addresses refer to that viewer.
The binary reader consumes each model in full and validates resolved resources and vertex indices.

The DX7 viewer’s scene registration at `0x4288f0` supplies the top-level resource arrays. Shapes name the geometry and contain render settings (`0x44acc0`, `0x4132f0`). Each geometry stores positions, normals, indices and texture applications (`0x4569d0`). The original reads the room cube face order from `room.ifl`. DDS images decode to RGBA before upload so the same source texels are used by Metal, WebGPU and WebGL2.

`setup_fullscreen.txt` selects five models at distances 5, 20, 60, 140 and 1000. It configures the 6 × 9 formation and 20 × 22.2 spacing reproduced at `0x404ee0`. The high-detail soldier contains `MARCH_ANIM` and `PANIC_ANIM` (`0x404260`), applied by name to all five models. The robot’s animation is embedded in its own model. The zero-duration channels are constant. `0x405eb0` references the three WAV cues.

The viewer separates forward movement from the root animation at `0x4053c0`.
The port removes the linear component of each forward track before sampling the local
pose. The march moves 8.96 units per 1.299-second stride. The robot advances 10 units
per 1.333-second stride. The formation loops at ±100 units, as in `0x4070b0`, with
ordered coverage at the two edges. Panic blends local joint transforms from the current
march pose and eases the soldiers into and out of scatter. Taking over the camera
keeps its current position and orientation.

The projected shadows use stencil to restrict them to the tabletop and shade each
sample once. Shading uses the supplied light map and reflection cube. The orbit,
panic paths, and combiner response remain approximations. The original camera script,
automatic shot sequence, menu and drawing modes remain unported.
