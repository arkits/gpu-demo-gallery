# How Radeon's Ark works

`Radeon'sArk1.3.exe` runs ATI's radiATIon engine on Direct3D 7 behind a small layer that
looks like OpenGL (matrix stacks, `glRotate`, `gluPerspective` with near 1 and far 4000, 50
degrees). Addresses below are in it; it was decompiled and never run.

## The show

Launched plainly, the program goes straight to the flight (the title cards and the space
opening belong to `-loop` and to a space scene this build never loads): `music.wav` starts
and the camera follows `fly.pth`, fading in over the first second and out over the last. The
picture is 16:9, letterboxed. After the flight the original shows its menu of effect
samples; this port flies again.

## The world (`radiation.ati`, read by 0x4125f0)

The file is the world's structure as it lay in memory, then every array it points to in a
fixed order; pointers are meaningless and counts say how much follows. It holds 33 rooms of
0x448 bytes, 21 mirrors of 200 bytes each with its own copy of all 33 rooms already
reflected, and 346 textures (format, flags, offset, width, height and the names of the
pictures they came from) over one 82 MB block of pixels.

A room keeps its vertices in one blob, in 30 buckets by kind (offsets in words at 0x2c8);
each bucket's triangles are a list of 16-bit commands (offsets at 0x3c0): 0, 1 and 2 put a
texture on a stage, 3 draws that many indexed triangles, 4 sets a bump matrix, 5 ends. A
bucket's kind fixes its vertex format and its texture stages (0x4274c0, 0x428600, 0x424df0,
0x427b00): lit-in colours with one, two or three textures (a light map, a detail map),
decals, translucent surfaces, and chrome mapped by the reflection vector through a dual
paraboloid map (0x4249e0 builds its texture matrix). Culling is clockwise.

Mirrors (0x419090) are drawn before the room the eye is in: their backdrop without depth
writes, their reflected rooms cut by user clip planes through the eye and the mirror's four
corners, reflected characters, then the surface added with a Fresnel weight
((1 - v·n)² between the mirror's alpha and one, 0x419f50).

Exhibit tanks (a room's 0x120 records, 0x426490) are windows: what is behind is drawn
through the window's edges with the world bent by M · L · M⁻¹, where M is the window's frame
and L stretches depth by how square on the eye looks and shears it by the eye's slant. Two
layers of caustics are projected from above, scrolling with time.

Pools (0x84 records) are height grids pulled by springs to their eight neighbours
(0x425430, a step a frame; two vertices are lifted at load to start the ripples), whose eye
ray is bent into the water and followed to the pool's floor or walls (0x425750).

## Paths (`.pth`, 0x41ee90)

A count, then each point's place and rotation (x, y, z, w), `c` after a point the path cuts
from. A point a second; a cut takes no time (`fly.pth`: 253 points, 29 cuts, 224 seconds).
Places and rotations are natural cubic splines over the points with eight more at each end
and around every cut; rotations are mapped to (x, y, z, 1 - w) / √(2 (1 - w)) for the spline
and back after. The view is the rotation, then the place undone.

## Characters (`obj.ini`, Proteus `.obj` and `.pan`)

`obj.ini` names each character's object and animation and whether it is seen in mirrors.
An object has bones with their rest matrices, materials (with their pictures) and meshes
that each blend at most four bones; an animation has its length in seconds and a matrix per
frame for the whole and for every bone, played by the flight's clock and lerped between
frames. A character lives in the tank whose box holds its first frame, else in its room.
They are lit by two white directional lights.

## Not ported

Noelle's nine animation events (their own drawing path), the eight tank windows that
render reflections to textures (on only with that option), and the menu of effect samples.
