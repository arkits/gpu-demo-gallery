# Toy Soldier engine notes

Source evidence is the extracted demo data and static decompilation of the viewer using
`npx -y rea-agents@6.10.1 decompile-all` with Ghidra. Addresses refer to that viewer.
The binary reader consumes each model in full and validates resolved resources and vertex indices.

The DX7 viewer’s scene registration at `0x4288f0` supplies the top-level resource arrays. Shapes name the geometry and contain render settings (`0x44acc0`, `0x4132f0`). Each geometry stores positions, normals, indices and texture applications (`0x4569d0`). The original reads the room cube face order from `room.ifl`. DDS images decode to RGBA before upload so the same source texels are used by Metal, WebGPU and WebGL2.

`setup_fullscreen.txt` selects five models at distances 5, 20, 60, 140 and 1000. It configures the 6 × 9 formation and 20 × 22.2 spacing reproduced at `0x404ee0`. The high-detail soldier contains `MARCH_ANIM` and `PANIC_ANIM` (`0x404260`), applied by name to all five models. The robot’s animation is embedded in its own model. The zero-duration channels are constant. `0x405eb0` references the three WAV cues.

The port projects the actors’ silhouettes onto the table and uses the shipped light map and reflection cube. The original has custom combiner and stencil state. The port approximates that lighting, shadow opacity and material response in WGSL. The orbit, formation motion, and panic timing are approximations. The original camera script, distance curves, automatic shot sequence, menu and alternate drawing modes are not reproduced.
