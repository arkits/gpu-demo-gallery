# What Bear adds to the Sushi engine

Addresses are in `SushiDX.exe` (the same file as Pipe Dream's), decompiled with REA and Ghidra
and never run. [I] marks inferences. File formats: [formats/README.md](formats/README.md).

## Chapters (`Sushi2.ini`)

`Normal`, `FurConstruction`, `LightingDecomp` and `LengthComparison`, 24 s each from a camera
that does not move. Near 1, far 500, 50 degrees. The sound is one loop, IMA ADPCM, as long as
the animation (7.97 s).

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

## The bear

- `bear.so2` has only textures. The meshes are in `bear.sa2`, after the bone tracks: four
  change shape (80 frames 0.1008 s apart) and the rest follow a palette of up to 40 bones.
- A mesh that changes shape binds several vertex streams: one frame, the next (stream array
  +0: 0 or 1), and what stays the same (-1, stored in frame 0 only). `morphlerp0` holds both
  frames' weights.
- `PassGroup Fur(10)`: meshes of a group take turns, so that all draw `Fur0` (skin), then all
  draw `Fur1` (fins), then the eight shells [I: from the shaders; the function that does this
  was not decompiled].
- Textures named `ElementN` are one image per shell.
- `LODBias` is added in the shader, since browsers' samplers have none.

## Differences from the original

- The forest is a flat picture as large as a 4:3 view, so windows of other shapes crop the
  picture instead of showing more, and the free camera keeps near the original one.
- Clip planes, camera splines and text as in the Car Paint port.
- Sizes of one pixel that shaders write down for 1024 by 768 (`vTexel...`) keep that share of
  the height, so that lines and glows look alike at any size.
- `ZBias` is taken as steps of a 24 bit depth buffer [I].
- The free camera, pause and chapter buttons are new. Menus, stereo and benchmark modes are
  left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
