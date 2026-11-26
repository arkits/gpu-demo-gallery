# Chameleon engine notes

The port uses the original scene data and static analysis of `ChameleonGL.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

The scene identifies the bone transforms per draw and its vertex streams carry the bone weights. The port evaluates the Maya transforms and weights. The 165 position arrays agreed with the ChameleonMark text scene to within 0.002 scene units. Every shape is drawn with its own passes: `src/vp.rs` and `src/combiner.rs` turn the vertex programs and register combiners into WGSL.

The `LodShape` children are seven skins, not levels of detail. As at `0x4023ba`, a skin stays 9,918 ms, the first with a head start of 2,500 ms (`0x402409`), and the next fades in at 0.0003 a millisecond, about 3.3 s (`0x4bb038`), with the sound `chafx2` (`0x4d1a88`). A fading skin is drawn where the old one is, by equal depth, and over it. The skin with holes comes first and whole. The branch is in seven segments that take turns: only the last, the current and the next are drawn (`0x402483`). The camera starts 25 away and keeps between 10 and 80, with the near and far planes at 1 and 100, where the leaves fade (`0x4029d0`).

Left out: the draw and camera menus and their sprites.
