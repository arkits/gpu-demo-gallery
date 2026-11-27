# Engine notes

## ATI SushiDX8

The six releases share an identical `SushiDX8.exe` (SHA-1 `4549bd902d065e1c8fc8688ef57a3f5bbb8310ee`). Rea recovered the scene reader at `0x405a50`, object reader at `0x430860`, animation reader at `0x431120`, and indexed draw stream at `0x430df0`. The v54 scene loader uses the declaration selected at `0x41cfc0` for terrain, water and other static geometry. The object loader uses the declarations selected at `0x42f900` and `0x430100`. Draw commands populate the joint palette, and each weighted vertex resolves its joint indices against that command's palette. Island 2's waterbug and slug use up to 20 palette entries per draw. A vertex reused under a different palette gets its own output index. Paths come from the original `FlyPaths` files.

Dolphin evaluates cubic PN triangle positions after applying the original bone animation. Nature and Island 2 use distorted reflection and refraction captures, with one reflected plane per scene. The port uses WGSL versions of the baked vertex lighting, layered terrain, cloud shader, caustics and material effects. The Island oceans, reflective crystals and bubbles sample their original cubemaps. Bubble film, crystal transparency and iridescent insect wings use angle-dependent shading modeled on the shipped shaders. The Island 2 slug uses the gloss mask in its bump map. Nature composites its lower waterfall foam into the water pass. Its plunge spray and Island 1's fire use animated texture displacement and the original sprite alpha. Rachel uses the original animated face streams and skin, eye and hair textures. Nature, Rachel and the Island releases package their original soundtracks.

Differences include estimated material lighting and Rachel position scale and bias. The renderable water passes use the visible scene instead of each original visibility list. Particle systems, shadow passes, Island teleport sequences and the original menus are incomplete. The current renderer loops the main camera paths and starts the Island demos at an established view along each path.

## Treasure Chest

The POF reader consumes geometry, normal, UV and face chunks. The model and texture pairings follow the executable at `0x410e70`, both camera paths come from the files referenced by `0x411d40`, and the lid uses the hinge and motion recovered at `0x411890`. The room, chest, four moving lights and seven showpieces use original data. A panorama of the textured keep supplies room reflections, modulated by the original material masks. Specimen placement, colors and material lighting are estimates. The original per-object Shader Model 1.1/1.4 comparison modes are incomplete.

## GeoForms

The port reads the field of view from `sceneBS.nma`, samples an original Voronoi volume and warp texture, and loads the shipping stencil textures. The stencil loading path can be seen at `0x4248a0` and `0x434c10` in Rea. The original `_bkg.fp30`, `blobs.fp30` and `deform*.vp30` guided the background, shading and deformation.

The liquid renderer sphere-traces a smooth distance field, estimates thickness and applies colored transmission, Fresnel reflection and surface noise. It converts the original stencils to signed distance fields and shades them with the same volume lighting. This approximates the original isosurface and subsurface scattering. Original depth peeling and HDR motion blur are incomplete. The web and native players analyze a chosen audio file. The original screensaver used system audio capture and its own beat detector.
