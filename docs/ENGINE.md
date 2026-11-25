# Squid engine notes

The port uses the original scene data and static analysis of `Squid.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

`SquidParams.txt` gives fog distance, kelp and fish counts, and 5,120 silt particles and 1,000 bubbles. The port loads the rigid and skinned model streams, plays the shipped animation channels, and applies animated billboards for the two particle sets.

The port approximates the particle and fish paths, underwater fog, lighting, and steering. The original fluid grid, procedural plants beyond the visible kelp, and draw and navigation menus are not reproduced.
