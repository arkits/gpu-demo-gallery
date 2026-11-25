# Chameleon engine notes

The port uses the original scene data and static analysis of `ChameleonGL.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

The scene identifies the bone transforms per draw and its vertex streams carry the bone weights. The port evaluates the Maya transforms and weights, then samples the supplied alternate diffuse textures when the skin changes. The 165 position arrays agreed with the ChameleonMark text scene to within 0.002 scene units.

The port uses the leading LOD and approximates the original combiner and skin transitions. The original automatic material choreography and draw modes are not reproduced.
