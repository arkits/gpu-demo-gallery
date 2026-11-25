# Bugs engine notes

The port uses the original scene data and static analysis of `BugsGL.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

The stage heightfield, 60-insect count and four LODs come from the stage model and `bugData.txt`. The material passes use the color and normal textures identified in the original render settings. Depth Vision accumulates projected fragments with depth testing disabled.

The port approximates flock motion, leg motion, bump-map register combiners, and the depth-color palette. It leaves out projected bug shadow textures, the helicopter and the navigation menus.
