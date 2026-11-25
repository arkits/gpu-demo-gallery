# Wolfman engine notes

The port uses the original scene data and static analysis of `FurViewerGL.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

Fur configuration is registered at `0x412b20`. The port reads the shell count, spacing and texture names directly, parses DXT3 alpha, and blends up to eight textured shells. Matrix palette skinning uses the weighted and indexed vertex streams in the model. The original vertex programs describe the blend, including four-register matrix strides.

The six animation segments use estimated boundaries within the animation file. The port approximates the anisotropic fur lighting and camera. It leaves out fins, the original shadow-map pass and the old draw and camera menus.
