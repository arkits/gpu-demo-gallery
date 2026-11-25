# Tidepool engine notes

The port uses the original scene data and static analysis of `tidepool.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

The scene includes rock geometry, a water grid, a background cube map and normal-map materials. The 12 position arrays agreed with the installer’s text scene to within 0.002 scene units.

The port shades a displaced translucent surface over the original basin using a cube map and depth testing. This approximates the original Z-correct bump mapping. It leaves out the fluid solver, underwater render targets, caustic pass, steam and parameter dialog.
