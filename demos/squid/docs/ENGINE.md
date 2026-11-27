# Squid engine notes

The port uses the original scene data and static analysis of `Squid.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

`SquidParams.txt` gives fog distance, kelp and fish counts, and 5,120 silt particles and 1,000 bubbles. Every shape is drawn with its own passes: `src/vp.rs` and `src/combiner.rs` turn the vertex programs and register combiners into WGSL. The squid has three morphable skins, which give way to each other in extra blended passes as at `0x40f210`, and it blinks. Its legs and long tentacles are the scene's `Curve` nodes, drawn as tubes. Silt and bubbles follow `silt.vxp` and `bubble.vxp`, with squares for points. The caustics are 32 pictures, 30 a second (`0x4014e0`), and the light's ambient colour turns (`0x410fa0`). The water's surface is 140 over the eye (`0x5330d8`) and follows the camera.

The squid steers itself and changes its mind from time to time (`0x410e00`, `0x410bb0`) with the C library's `rand`. It turns at 0.105 rad/s (`0x5330e8`), keeps between heights 10 and 50 (`0x5330dc`), and each stroke gives it speed 4, of which 0.98 is left each frame (`0x5330ec`). The arrow keys steer it for a while, and after 8 s without a key it steers itself again. A skin lasts 20 s; S changes it at once.

Inferred: steps of 1/60 s, the 3.75 s stroke, the tiles of kelp and plants and the sway of their stalks, the trailing tentacle (the program simulated ten joints), and the fish, which circle the squid. Left out: the fluid grid and the draw and navigation menus.
