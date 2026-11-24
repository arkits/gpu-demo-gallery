# How the original engine behaves

Recovered from `SushiDX.exe`. Addresses are function starts. [I] marks inference.

## Camera
- Path samples are 30 Hz. Position and orientation go through a cubic spline
  (0x4093e0, 0x40a1c0); orientation is splined in a stereographic projection of the
  quaternion and renormalized.
- A sample whose flag is 1 sets a "cut" marker when passed (0x40a1c0). 22 samples
  in the demo have it. [I] They are camera cuts.
- The port interpolates linearly between samples and does not blend into a cut.
- View = rotate(quaternion) after translate(-position) (0x4123b0, 0x430420, 0x430560).
- FOV below 0 in the path means "use the default" (0x4123b0). Every sample is -1, so
  it is always 35 degrees.
- Projection (0x42ba80): right-handed, depth 0..1, **horizontal** FOV, height from the
  aspect ratio. The port keeps the 4:3 vertical extent on wider screens.

## Scene drawing (0x413f50)
- A draw record selects geometry, up to 64 texture slots and up to 4 FastRT lights.
- Cells own groups; a group lists (draw, vertex buffer, first index, index count).
  44 groups cover every index of every buffer exactly once.
- Geometry gives the shader and a transparency flag.
- Shader variant: the pass property (such as `Normal`) plus `FastRT<n>`, where n is the
  draw's light count.
- Cells and visibility lists select what to draw. The port draws everything.

## FastRT lights (0x4265f0, 0x427160, 0x427210)
Light record fields: +0x2a object-ambient flag, +0x2b FastRT flag, +0x2e intensity
(object ambient), +0x36 color, +0x32 and +0x6b ranges, +0x5f position, +0x6f/+0x7f/+0x8f
light-space axes, +0x15f spotlight texture, +0x46/+0x47/+0x4b/+0x4f color animation
(flag, step, key count, 16-byte keys), +0x1ac pulse, +0x1a3 blink.
- `FastRTPosMatrix` rows: (position, 1/range at +0x32), (axis 1, 1/range at +0x6b),
  (axis 2, 0), (axis 3, 0).
- `FastRTColor`: rgb plus a weighted sum of rgb in w.
- A light whose animated color is black is skipped.
- 7 of the 19 lights are FastRT; draws list at most 3 of them.

## Textures (0x429a70)
Record: format +0x100 (0 L8, 3 ARGB8, 4 XRGB8, 6 DXT1, 8 DXT5), kind +0x104 (6 =
supplied by the engine), bits per pixel +0x1bd [I], mipmapped +0x1bf [I], face size +0x1c0, total +0x1c4,
width/height u16 +0x1d0, faces +0x1d4, cube flag +0x1d5. Identical textures are shared.

## Vertex data
Attributes are packed in declaration order: 1 position, 2 normal, 7 UV, 10 baked light,
14 material color. Colors are D3DCOLOR (BGRA bytes). Everything is in gamma space.

## Bone tracks (0x438d10, 0x439680)
Time modulo duration; walk key deltas; fraction clamped to 0..1. See `formats/README.md`.
