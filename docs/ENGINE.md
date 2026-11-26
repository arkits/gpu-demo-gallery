# Tidepool engine notes

The port uses the original scene data and static analysis of `tidepool.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

The scene includes rock geometry, a water grid, a background cube map and normal-map materials. The 12 position arrays agreed with the installer’s text scene to within 0.002 scene units.

Every shape is drawn with its own passes: `src/vp.rs` and `src/combiner.rs` turn the vertex programs and register combiners into WGSL. As the vertex programs lay it out, two textures are drawn before the picture: light bent by the water onto the pool (caustics, covering it by a number of `ProgramMCB_directionalLightcaustics`), and the pool as seen from above, which the water then shows bent and, by a Fresnel term, mixed with the cave's cube map. The shoreline's "Z-correct bump mapping" replaces depths so that the water meets the rocks along its bumps; Z turns it off and on. The bump map drifts over the water.

The original solved a fluid on two grids. The port solves the wave equation on one, over the pool's grid, with a slow swell that keeps still water alive [I]. A click on the water, or S, drops a drip into it. Left out: steam, the lens flare, the guides, the parameter dialog, and the low pool and plain water meant for slower cards.
