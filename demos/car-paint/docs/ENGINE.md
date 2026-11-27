# What Car Paint adds to the Sushi engine

Addresses are in `SushiDX.exe` (the same file as Pipe Dream's), decompiled with Ghidra and
never run. File formats: [formats/README.md](formats/README.md).

## Chapters (`Sushi2.ini`)

| Script | Camera path | Length |
|---|---|---|
| `Normal.sds` | `CarRoom.pth` | 61.3 s |
| `NoNormalMap.sds` | `NormalMap.pth` | 37.5 s |
| `NormalMap.sds` | `NormalMap8vs16.pth` | 31 s |
| `Paint.sds` | `Paint.pth` | 33.4 s |
| `MultiColor.sds` | `CarRoom.pth` | 61.3 s |

They follow one another (`cycleAllModes`), with 1 s fades. Near 1, far 99999, 50 degrees.
The music starts over with each (0x4253a0): an intro, then a loop from 6.92 s before its
end. Both are IMA ADPCM; the decoder gives the same samples as macOS.

## Frame scripts (0x404810)

Render targets, viewports, scissors, clears, objects, scene, reflections, outlines and text.
Coordinates are fractions from the bottom left. Buffers made by one script exist for all.
Text is 0.3125 as wide as high, from `Text.tga` (8 by 16 characters).

## Mirrors (0x290 records of a scene; 0x415610)

| Offset | Meaning |
|---|---|
| +5, +6 | Limited to its outline; drawn in place instead of into a buffer (both 0 here) |
| +0x18 | Corners |
| +0x1c8, +0x1d8 | Plane, reflection matrix |
| +0x218, +0x234, +0x250 | Three visibility sets as in lights: the mirror's surface, the scene seen in it, unused |

`DrawReflections` draws the second set as it is, since the original's compiler stored the
scene already reflected, then the objects with bones, lights and winding reflected and
everything on the wrong side clipped. Shaders with the `Reflections` property are preferred.
Surfaces that read the buffer are left out while it is drawn; the original reads it, which
is undefined, but clips those surfaces away.

The floor reads the buffer through `ScreenProjectionMatrix`: clip space to texture
coordinates times w.

## Other additions

- Texture type 12 (V16U16), the car's 2048×2048 normal map, becomes two half floats, which
  every backend can filter. Half floats are finer than 16 bits near zero, where most of a
  normal map lies, and coarser near one.
- Alpha test, color write masks, anisotropic filtering, `1-r0`, `dp2add`, `texldp`, and
  fixed-function Modulate2x.

## Differences from the original

- Clipping is done by the projection's near plane, as WebGPU has no clip planes. It clips
  the scene in the mirror too, which the original does not.
- Reflections are 512×384 as in the original, but multisampled.
- Scripts' `SetProjectionBasedOnViewport` is ignored: the projection follows the window.
- Outlines are about 1/768 of the height instead of one pixel. Text and outlines are drawn
  after everything else, which is where the scripts have them.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
