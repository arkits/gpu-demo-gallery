# How KyroToon works

Addresses are in `toon.exe` as unpacked by `upx -d` (image base 0x400000). The program is
PowerVR's "D3DShell" demo framework with modules for animation, camera, geometry, lights,
materials, render modes, shadows and screen fades; the scene, exported from 3D Studio Max, is
compiled in as tables. Nothing is read from disk but the optional `toon.ini`, which is also a
resource.

## Scene (0x403000)

30 frames a second, 700 frames in a loop. Tables: 3 lights at 0x49bb90; 58 meshes at
0x521ab0 (0x98 bytes each: name, counts, material, centre, flags, positions, 12 texture
coordinate kinds, normals, colours, faces); 17 materials at 0x49b640 (0x50 bytes: 12 map names,
ambient, diffuse and specular colours, shininess); mesh animations at 0x5d0218 (0x4c bytes:
visibility, matrix, position, rotation and scale tracks, each per frame or keyed); camera at
0x49bc80 with its tracks at 0x5d8110.

Tracks (0x403380): frame f = floor(t × 30); values blend from f to f+1 (slerp for rotations,
0x445710). Keyed tracks take the first key after f, and blend to the next only when f+1 is that
key's frame. The matrix track is ignored: the command line code sets matrices off (0x424d40),
so meshes use position, rotation (quaternion to matrix as 0x445a50) and scale.

Camera (0x4067d0, 0x445070): x = up × direction, up and direction as stored, normalised. The
projection's field of view is across; up and down it is 0.8 of that (4:3). The port widens the
view across for wider windows.

## Toon shading (render style 2)

Each material gets a mode (0x421df0): 0x37 plain, 0x38 with a diffuse map, 0x36 with an
opacity map (the logo). Meshes are drawn by mode, then material (0x423620).

For each mesh each frame:

1. Silhouette coordinates (0x41b9a0): the eye in the mesh's space; for each vertex V the unit
   vector to the eye and N its normal: u = |V·N|, v = (1 − (V − 2(V·N)N).y) / 2.
2. ProcessVertices with transform and lighting (0x41b1d0): three point lights at 0.71, ambient
   from the material.
3. Tone (0x41b070): u = v = (r + g + b) × 0.0012531 of the lit colour; the vertex colour is then
   replaced by the material's diffuse colour.

Mode 0x37 modulates the colour by the tone picture, then by the silhouette picture; 0x38 the
diffuse map by both. All filtering is point, without mipmaps (0x4165e0, for style 2). Meshes
named NO_SILHOUETTE (the floor) draw without the silhouette picture. The clear colour is
0xa6a0b6.

## Shadows (Shadow module, 0x43efe0)

Meshes named SHADOW_FLAT are flattened onto the floor (the height of SHADOW_FLOOR's centre)
from each light whose word 18 is 1 (Omni02), one unit above it (0x43ebc0); drawn into the
stencil only (0x43f610); then a quad over the screen darkens stencilled pixels by half and
clears them (0x43f830).

## Fades (ScreenFade, 0x43db80)

`toon.ini` holds `Frame N { Trigger Fade frames from to colour "picture" }`: at 0 the chip
picture fades out over 15 frames; at 650 it fades in and holds. A quad over the screen with the
picture and the fade's alpha. The C key shows the credits picture fading from 0.8 over 90 frames
(0x415180).

## Left out

The D3DShell's menus, text windows (frame rate, description) and benchmark; frustum culling
(nothing is drawn differently). The intro picture shown while loading.
