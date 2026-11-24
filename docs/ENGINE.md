# How the original engine behaves

Recovered from `SushiDX.exe`. Addresses are function starts. [I] marks inference.

## Camera
- Path samples are 30 Hz. Position and orientation go through a cubic spline
  (0x4093e0, 0x40a1c0); orientation is splined in a stereographic projection of the
  quaternion and renormalized.
- A sample whose flag is 1 is the last of its shot: every jump in the path follows one
  (0x40a1c0 sets a cut marker there). 22 samples have it, including the final one.
- The port uses Catmull-Rom through the samples and holds a shot's last sample.
- View = rotate(quaternion) after translate(-position) (0x4123b0, 0x430420, 0x430560).
- FOV below 0 in the path means "use the default" (0x4123b0). Every sample is -1, so
  it is always 35 degrees.
- Projection (0x42ba80): right-handed, depth 0..1, **horizontal** FOV, height from the
  aspect ratio. The port keeps the 4:3 vertical extent on wider screens.

## Frame (`Scripts/Normal.sds`)
1. Clear depth to 1 and stencil to 128. Color is not cleared; the port clears to black.
2. Objects: `Normal`, opaque, receiving.
3. Scene: `Normal`.
4. Objects: `Shadow`, into stencil only.
5. Quad where stencil differs from 128, blended `InvDestAlpha, DestAlpha`. Framebuffer
   alpha holds each pixel's shadow brightness.
6. Objects: `Normal`, transparent, receiving. 7. Objects not receiving (none exist).

## Scene drawing (0x413f50)
- A draw record selects geometry, up to 64 texture slots and up to 4 FastRT lights.
- Cells own groups; a group lists (draw, vertex buffer, first index, index count).
  44 groups cover every index of every buffer exactly once.
- Cells and visibility lists select what to draw. The port draws everything, in draw order.

## Objects
`Objects/animusic.ini` lists 344 instances: `Global` (no shadow) or `SGlobal` (casts a
shadow), a start offset (always 0), a mesh file and an animation file.

Mesh flags (+0): 1 casts a shadow, 2 receives, 8 transparent, 0x10 hidden except in the
shadow pass (0x432160). `gVLight` meshes are hidden helpers.

Draw commands (0x432690): 0 draw (first, last index); 1 set matrix slot (slot, bone);
2 set texture (declaration index, object texture); 3 set parameter (parameter, value);
4 select vertex and index buffer.

Animation
- Bones: neighboring keys blended component by component (0x439bb0). Bone matrices map the
  stored vertex positions to the world; objects have no other transform.
- `SOLVisibility` is stepped, not blended (0x431d50).
- Other tracks (`SOLGlow`, `SOLVibrate`, `SOLMonoLength`) blend linearly and set the shader
  variable of the same name (0x43db70).
- `Glow` is asked for when |SOLGlow| > 1/255, else `Vibrate` when |SOLVibrate| > 0.0001
  (0x43e5e0).
- `SOLVelocityX`: direction and distance moved since the last frame; zero below 1e-6 or
  above 80. `SOLOrientationX` turns +z toward it. The port uses a fixed 1/60 s step.
- Bounds are sampled over the clip, interpolated, and the radius is multiplied by the
  largest `BoundsExpand` (0x431b00).

## Lights (0x414c40, 0x415140, 0x4170c0, 0x4265f0)
Record fields: type +0x20 (0 point, 1 spot), flags +0x28 (bit 1 casts shadows),
object-ambient +0x2a, FastRT +0x2b, ambient intensity +0x2e, ranges +0x32 and +0x6b,
color +0x36, color keys +0x46 (flag, step, count, 16-byte keys), cone degrees +0x53,
position +0x5f, axes +0x6f/+0x7f/+0x8f (the last is the direction), six volume planes +0x9f,
spotlight texture +0x15f.
- Reach of a sphere: 0 outside range; for spots, 1 - (angle - spread) / half cone, clamped.
- Receiving objects use the first four FastRT lights that reach them and are not black, and
  the first four object-ambient lights that reach them.
- Shadows use one light: the strongest shadow light whose volume holds the object.
- `FastRTPosMatrix`: (position, 1/+0x32), (axis 1, 1/+0x6b), (axis 2, 0), (axis 3, 0).
  `FastRTColor`: rgb and 0.3r + 0.59g + 0.11b.
- Over the whole show the light counts always have an exact shader variant, which supports
  this reading.

## Show (0x421c50, 0x4241a0, 0x4252b0, 0x4253a0)
- Scene time starts at `startTime` (10 s) and the music at 0, together.
- Black fades of 2 s at both ends (0x41a950).
- Logos (0x418af0): `ChipLogo.tga` bottom right, 27% of the width; `Logo2.tga` bottom left,
  25%. Both are stored bottom row first.
- Free cameras orbit a point within distance, pitch and yaw limits (0x411fb0, 0x4139c0).

## Textures (0x429a70)
Record: format +0x100 (0 L8, 3 ARGB8, 4 XRGB8, 6 DXT1, 8 DXT5), kind +0x104 (6 =
supplied by the engine), bits per pixel +0x1bd [I], mipmapped +0x1bf [I], face size
+0x1c0, total +0x1c4, width/height u16 +0x1d0, faces +0x1d4, cube flag +0x1d5.
Identical textures are shared, and some names exist in two formats.

## Vertex data
Attributes are packed in declaration order. Usage codes: 0 position, 1 blend weight,
2 blend index, 3 normal, 7 texture coordinate, 8 tangent, 9 binormal, 12 color. Scene
records use their own codes: 1 position, 2 normal, 7 UV, 10 baked light, 14 material color.
Colors are D3DCOLOR (BGRA bytes). Everything is in gamma space.

## Known differences from the original
- Catmull-Rom instead of the engine's own spline between 30 Hz camera samples.
- Fixed motion-blur step; no portal culling; wide screens show more at the sides.
- No Space-to-start screen, help menus, gamma setup, benchmark or stereo modes.
- Not compared with a recording: none has been available.
