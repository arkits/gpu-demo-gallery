# What NPR Hatching adds to the Sushi engine

Addresses are in `SushiDX.exe` (the same file as Pipe Dream's), decompiled with REA and Ghidra
and never run. [I] marks inferences. File formats: [formats/README.md](formats/README.md).

## Chapters (`Sushi2.ini`)

Five scripts along one 40 s camera path: `Normal`, `ToggleEdges`, `TypesOfEdges`,
`ShadowEdges`, `EdgeWidth`. Near 60, far 4000, 50 degrees. No music.

## Drawing (0x404810)

- `SetRenderTarget` takes a number, a color buffer and a depth buffer. Number 0 resets the
  viewport for drawing, but `Clear` keeps to the rectangle of the last `SetViewport`
  (0x42f2e0): scripts rely on it to clear one part of the window.
- Buffers keep their contents, and depth buffers are shared between color buffers.
- `DrawQuad` names an entry of the scene file's table at header +0x112 (name, draw record).
  Its corners and texture coordinates are fractions from the bottom left, of the viewport and
  of the textures.
- `SetProjectionBasedOnViewport` keeps the horizontal field of view (0x42b950).
- Shaders are found by property and number of lights (0x445c20). Some shaders these demos need
  name no number of lights; the port lets them serve any, though the original's comparison
  has no such rule [I].

## The picture

`Normal.sds`: depth, then shadow volumes into the stencil buffer (an `SGlobal` object's
strongest shadow light, 0x415140), then three targets at once: hatching, normals and depth,
and texture colors as object numbers. Three rectangles multiply lines into `cEdgeBuffer`, and
a last one thickens them on the picture.

Shadow volumes count on pass with the depth test reversed. `Normal` shades by six hatching
textures whose weights come from a lookup by N.L; shadowed pixels use one fixed texture and
half the color.

Shaders inherit passes by number as well as by name, also across later shaders of a file.

## Differences from the original

- Clip planes, camera splines and text as in the Car Paint port.
- Sizes of one pixel that shaders write down for 1024 by 768 (`vTexel...`) keep that share of
  the height, so that lines and glows look alike at any size.
- `ZBias` is taken as steps of a 24 bit depth buffer [I].
- The free camera, pause and chapter buttons are new. Menus, stereo and benchmark modes are
  left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.

No multisampling, as in the original (`multisampleType = 0`): the outlines cover most edges.
