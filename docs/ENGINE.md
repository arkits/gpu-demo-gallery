# How the demo works

Recovered from the files and `LogoViewerGL.exe` (Ghidra). [I] marks inferences.

## Files

`.nmb` is a tree of typed nodes: `u32` kind, then fields as `i32` id and value. Id -3 is
the name (`u32` length, bytes) and -4 ends the node. `.nma` is the same tree as text.
`src/tree.rs` holds the kinds and fields, with the names the viewer uses itself. Every
`.nmb` is read to its last byte; unknown kinds and fields are errors.

`logo.setup` names three models (metal, tron, bump). Each `.nmb` has the geometry; the
three `.nma` beside it have the same shapes with only `renderSettings`, which replace
the model's, matched by name [I: the viewer goes by order, which is the same].

Primitive type 5 is a triangle strip and 3 a list [I, fits every index count].

`.dds` are DXT1 and DXT5 with mips. The cube map (`logo-cubemap.ifl`) is
`sunol_{right,left,top,bottom,front,back}_mipmap.dds` for +X, -X, +Y, -Y, +Z, -Z.
`cloudyhills*` and `bumpy_bump` are not used.

## Shading

`src/combiner.rs` turns the combiner settings into WGSL: general combiners with input
maps, `PASS_THRU`, `MOD`, `DOT`, `INTERP`, bias and scale. Registers are limited to
[-1, 1] and the result to [0, 1]. Register 0's alpha starts as texture 0's.

- Shiny: `D*T`; then the reflection; then half of each.
- Techy: `D*T`; half of each; reflection weighted by the texture's alpha. Its right
  shapes are plain reflection throughout.
- Bumpy: normal map (in the model's space) dot light, times a color; plus 20 percent
  reflection; plus reflection weighted by alpha. `bumpDiffuse` puts the light's
  direction, in the model's space, into constant 0.

Reflections use the camera's space (`EYE_REFLECT`, as `GL_REFLECTION_MAP`), so the
surroundings turn with the camera.

One white directional light, fixed to the camera: Rx(-0.6) Ry(0.6) (0, 0, 1).
Vertex lighting, no ambient, no specular.

Stages 0 to 2 replace every shape's settings: white points of size 2, white lines, or
0.8 grey lit. Points and lines ignore depth.

## Camera

Spherical: view = T(0, 0, -distance) Rx(-pitch) Ry(yaw). 0.7 radians vertically. The
distance starts at 1.5 times the bounding radius plus 50 and stays in 200 to 500. The
radius follows the viewer's rule for groups (middle of the children's boxes, farthest
child); the meshes' own spheres are taken as box middle and farthest vertex [I], giving
249.6. The pitch is never positive. It turns once in 20 seconds until dragged, and then
goes on as it was let go.

## Show

Starts with the first model's reflection. After 2 seconds it fades to the first stage.
Then one stage every 10 seconds without input. The second and third models start at
their texture. Changing model takes 2 seconds to black and 2 back. Each model has a
looping track; `transition.wav` plays at the start and on later changes. The captions
(256 x 256, added) and badge (alpha blended) sit in the lower corners.

## Differences

- 4x antialiasing; mips are blended, where the files ask for the nearest.
- Points and lines grow with the screen's height.
- On screens other than 4:3 the images keep their shape; narrower ones keep the width
  of the view, and the caption is larger.
- Pausing, zooming by wheel, number keys and buttons are new. The original zooms with
  the other mouse button and has menus.
- The background is taken to be black [I].
- Near and far planes are 10 and 2000; the original fits them to the scene.
