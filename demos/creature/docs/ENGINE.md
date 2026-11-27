# How the demo works

Recovered from the files and `CreatureViewerDX7.exe` (Ghidra). [I] marks inferences.

## Files

`.nmb` and `.nab` are trees of typed nodes: `u32` kind, then fields as `i32` id and
value. Id -3 is the name and -4 ends the node. `src/tree.rs` holds the kinds and fields
with the viewer's own names. Every file is read to its last byte; unknown kinds and
fields are errors.

- `creature.nmb`: 348 transforms (54 of them joints) and 219 rigid shapes. Nothing is
  skinned.
- `creature.nab`: 123 channels with 313 Maya curves that move nodes by name.
- `fish.nmb`, `beam.nmb`: one shape each.
- DXT1 and DXT5 textures; uncompressed TGA for normal maps, caustics (32 frames) and the
  water's cube map (`underwater.ifl`: +X, -X, +Y, -Y, +Z, -Z).

## Animation

A curve has a start, a stop, and what happens before and after (0 nothing, 1 cycle).
Keys have a value and tangents in seconds and value. Method 1 is Hermite, 2 is Bezier
with control points a third of the tangents away, drawn in where time would run
backward (0x46d6c0; the same as Maya's). Transforms compose as Maya's: pivots, scale,
orientation, rotation, translation; joints add their own orientation after the rotation.
Euler angles turn about x, then y, then z. About 30 hairs end their 7 second loop up to
10 degrees from where it starts; that twitch is in the data.

## Shading

`setupHack` values: 1 `mat` (vertex lighting), 2 `mat_c` (and a texture), 9 `bc_d`
(normal map in the shape's space dot light, times a texture).

The viewer changes `mat_c` shapes (0x404060): ambient 0.14, and another texture stage
that multiplies by the caustics, 30 frames a second, projected from above:
u = 0.4 x, v = -0.4 z. All shapes use one light of white diffuse and ambient. It is above
the camera, moves with it, and sways by 0.3 cos(0.003 ms) and 0.3 cos(0.0029 ms) about
two axes (0x403cc0).

Fog is linear per vertex, 0 to 80. Its color goes with the angle between up and the
view from black (looking down) to (.05, .6, .4) (looking up), as in
`setup_fullscreen.txt`. The water behind is a cube map without fog.

## Fish

500 boids start from the C library's `rand` (seed 1), which the port repeats. Each sees
others within 8. Forces (0x40ca00): separation 1 and alignment 0.8, both weighted by
1/d2; cohesion 1 (the mean *position* of the others, as the original has it, which
pushes outward); home 0.001 d3 toward the middle; predator 10/d away from the middle.
They turn at most 0.6 a second, change speed by 0.7 a second, and swim 0.5 to 8.5-11.5.

## Plankton

2000 points in a cell of 80, repeated in the 27 cells around the camera. Each follows a
mix of two currents: x by 11 cos(0.00051 ms) and y by 7 cos(0.00019 ms). The original
does that with vertex blending. They show the caustics' color.

## Laser

Space: ten 10 long beams from the mask at (0, 2.7, 12 + 10 i) for 5 seconds, texture
moving by 0.0007 and 0.0043 per ms, alpha blended, with `laser.wav`.

## Camera

Spherical: 0.7 radians vertically, planes 1 and 112, starting at 1.5 times the bounding
radius (14.6 here [I: meshes' spheres are box middle and farthest vertex]), never from
below.

## Differences

- 4x antialiasing; the caustics have mips. Points grow with the screen's height.
- Narrow screens keep the width of the view.
- No culling of back faces [I: the files set none]. The beam does not write depth [I].
- Plankton is drawn opaque [I].
- Left out: the menu, help screen, frame counter, points, lines and solid styles, keys for
  the fishes' forces, and the voice clip on Enter (`drevil.wav`), which imitates a film
  character and is not part of the scene. The port bundles neither.
- At most 1000 fish. Pausing and buttons are new.
