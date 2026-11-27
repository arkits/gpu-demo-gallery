# What Natural Light adds to the Sushi engine

Addresses are in `SushiDX.exe` (the same file as Pipe Dream's), decompiled with REA and Ghidra
and never run. [I] marks inferences. File formats: [formats/README.md](formats/README.md).

## Chapters (`Sushi2.ini`)

| Script | Camera path | Length |
|---|---|---|
| `Normal.sds` | `DebevecRNL.pth` | 90.1 s |
| `HDRSplit.sds` | `DebevecRNL1.pth` | 33.3 s |
| `ExposureSplit.sds` | `DebevecRNL3.pth` | 33.3 s |

Near 1, far 50000, 50 degrees. The music is one loop, IMA ADPCM.

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

The pedestal's mirror image, then the scene into a buffer 1.4 times as fine as the window,
colors divided by 16; downsampled to 512 by 384, blurred in x and y, and mixed 0.6 to 0.4
with vignette, exposure and gamma.

- Buffers have 16 bits per channel, 0 to 1. Browsers cannot draw to such integers everywhere,
  so they are half floats here, clamped to the same range by every shader.
- The probe is a cube map of texture type 11, four 16-bit integers, with a shared multiplier
  in alpha; also half floats here. What surrounds each ball is in further cube maps.
- A mirror's own surface (first set of a 0x290 record) is drawn with the scene (the end of
  0x415610). Its +5 flag, set here, limits the mirror image to the surface's outline; the port
  leaves that out, as the rest is not seen.
- The large buffers follow the window's shape and at least its height.

## Differences from the original

- Clip planes, camera splines and text as in the Car Paint port.
- Sizes of one pixel that shaders write down for 1024 by 768 (`vTexel...`) keep that share of
  the height, so that lines and glows look alike at any size.
- `ZBias` is taken as steps of a 24 bit depth buffer [I].
- The free camera, pause and chapter buttons are new. Menus, stereo and benchmark modes are
  left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
