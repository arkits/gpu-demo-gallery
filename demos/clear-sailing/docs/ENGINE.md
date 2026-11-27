# How Clear Sailing works

Addresses are in `bin/PirateShip.exe` (InstallShield 6, 2004; PE x86, image base
0x400000, sha256 `4725c110…b3df`, 6470 procedures, 2845 strings), decompiled with
REA and Ghidra 12.2 and never run. [I] marks inferences; the rest was read in
code or checked against the data.

## Files

The installer unpacks `PirateShip.exe` (1.7 MB) with Cg 1.x `vp30`/`fp30`
sources, 330 textures, 8 scene files and WAV audio. Two scenes are binary
(`PS_03_19.nmb`, 7.1 MB: the ship; `sail_animation.nmb`, 0.9 MB: 18 sail-morph
channels), six are text (`pirates`, `ropes`, `foamy_ocean`, `reflection`,
`hdr`, `menu`, `smoke` `.nma`). There are no `.nma` twins of the `.nmb` files:
the text scenes are extra fragments (ropes, ocean, menus), not copies.

`src/nmb.rs` reads both `.nmb` files to the last byte. The framing matches the
earlier NVDemo viewers (kind id, fields by index, `-3` name, `-4` end) with a
newer schema, recovered from the program's kind constructors:

- `NVRenderPass` (`FUN_00468430`, called from 0x468c6c): `colorTargets`,
  `depthTarget`, `clearsColor`, `clearColor`, `clearsDepth`, `clearDepth`,
  `clearsStencil`, `clearStencil`, `camera`, `useSceneShapes`, `shapes`,
  `useSceneBackgrounds`, `backgrounds`, `overrideSettings`, `useListIndices`,
  `cubeFace`, `viewport` — fields 0–12, 14–16; field 13 is absent when unset.
- `SpotLight` is `transform, color, intensity, isEyespace, position, direction,
  inner, outer, attenuation` (not Dawn's diffuse/specular/ambient triple).
- `Camera` is in `.nma` order: `useScreenAspectRatio, near, minNear,
  dynamicNear, far, maxFar, dynamicFar, fieldOfView, aspectRatio, position,
  orientation, transform`.
- `ConstantParam` carries `value, min, max`; `TextureParam` carries
  `paramName, texture, state`.
- `Shape` has one `transform` string (not a list); `MayaTransform` adds
  `rotationOrder` before `children`.
- `0x20000101` is a procedural texture (`dimensions`, two ints) [I].
- `0xc0000000` is `RenderPass`, `0x60000022` a shape `Group`.
- `GeometryChannel` (`FUN_00484280`, registered at 0x486d24 as `0xa0000313`
  with `target, keyframeInterpMethod, looping, keyframes`): its keyframes are
  embedded (`FUN_00481d30`): a count, then `0xa0000120` morph targets, each an
  index, a time as double, an array of positions and an array of normals —
  absolute ship-space targets, 121 or 56 vertices, 12–30 per sail over
  0–1.5 s (`sail_animation.nmb`, `timeScale` 0.63 [I]).
- `primType` 3 is a triangle list, 5 a strip restarting at `0xffff` (as in
  Dawn); all 69 ship meshes use them, with `a2v.objCoord`, `a2v.objNormal`,
  `a2v.objTangent`, `a2v.objBinormal`, `a2v.TexCoord`.

`src/nma.rs` reads every `.nma` file to the last byte: `Scene { … }` with typed
blocks, `[...]` lists, `//` and `/* */` comments and `,` separators. `Mesh`
positions are literal `Pnt3fArray` values; ropes are `line_strip`s;
`ParticleSystem` (`smoke` 10000, `debris` 1000, lifespan 2) and `Ocean`
(amplitude 0.5, displacement 2, wavelengths 20–5000, 150×150) carry the
simulation parameters. `args.txt` gives window, HDR and target sizes;
`sliders.txt` (read beside `sliders.txt@0x52c038`, used from 0x4207df and
0x426acf) gives 38 defaults.

## Drawing

`shaders/demo.wgsl` has one program per pass, by hand from the Cg:

- Ocean (`ocean.vp30/.fp30`): sky cube plus the ship's reflection texture
  under chop, wake, foam and fog. The grid (150×150 in the file, 128×128
  here [I: speed]) and its Gerstner waves run on the CPU; `chopinessTex`
  feedback is approximated analytically [I].
- Ship and sails (`sails.fp30`, also used for the hull's
  `blinnAmbientBumpCubemapShadow`): Blinn with bump and cube maps, sun,
  water bounce and fog. Shadow maps are skipped (factor 1) [I].
- Ropes (`rope`), smoke and spray (point sprites in the file, billboards
  here), the HDR chain (`comp`, 9-tap `blur1`/`blury` here a 5-tap separable
  blur) and menus (`menu.nma` pictures).

Frame order [I, from the passes and `Creating HDR passes…`]: shadow, ship
reflection (512×256), main, HDR bright/blur/composite. The port renders
reflection, scene, bright, two blurs and the composite, then the menu quads.

The program runs a GLUT loop (`_glutMainLoop@0`); startup prints `Loading
Pirate Ship Demo...` (@0x52c950) and builds `main_menu` (@0x52c96c) and
`dev_menu` (@0x52c21c) from `menu.nma`, whose callbacks (`QuitCB`,
`AboutCB`) and slider bindings (`NVTexSlider::AddToViewer`) the port does not
reproduce [I].

Sails loop their keyframes; the ship heaves, pitches and rolls around the
`center_of_mass` sliders; smoke and spray are CPU particles driven by the
`smoke_*`, wind and damping sliders.

## Differences from the original

- The menu system (drop-downs, callbacks such as `QuitCB`) is a static logo
  and actions bar; slider values change with keys instead of drag controls.
- No shadow maps; one cube map stands in for the diffuse/specular pair; the
  demo's gamma is not reproduced exactly.
- Wind, waves, smoke and ship motion follow the file parameters but their
  integrators are the port's [I].
- `F` fires a broadside (sound, smoke, splashes), `H` toggles HDR, `U`
  toggles menus, `V - =` edit the featured sliders, `R` auto-turns.

Not checked: side by side with the original, Safari, Firefox, phones,
Windows, Intel Macs. The DDS decoder flips rows to big-endian bit order per
spec; menu pictures are stored mirrored (`logo.dds` upside down too), which
the port undoes when drawing them.
