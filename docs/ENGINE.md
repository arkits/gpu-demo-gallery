# How Bubble works

Addresses are in `bubble.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences.

- Six pictures of 256 by 256 are PNG resources: right, left, top, bottom, front, back, that
  is +X, -X, +Y, -Y, +Z, -Z. The program stands them upright in OpenGL's order and turns the
  four at the sides half around for the cube map (0x40d250).
- The bubble is an icosahedron divided two to four times, with new corners on the sphere
  (0x406570, 0x410fe0, 0x401350): 5120 triangles.
- Each step (0x4051d0, 0x411e20, 0x412170, 0x412240, 0x4120c0): speed gains 0.005 of the way
  back; 0.6 of each edge's change in length; half the mean of the triangles' normals, not
  normalized, as pressure. It becomes 0.9 of itself and 0.1 of the neighbors'. After moving,
  it is damped by a factor that grows from 0.7 to 0.99.
- A poke takes the point nearest the line of sight that faces the eye, and gives every point
  0.15 of its position, times 10^-d², inward (0x412550).
- The view's rotation is in the projection matrix (0x404fa0), so that OpenGL's reflected
  vectors come out in the space of the pictures. The bubble is 5 ahead of the eye. 2 to 40,
  half as high as far.
- The view turns (0x406400): speed gains 0.001 of the pointer's place, pitch is pulled back by
  0.0006, then 0.94 is kept. While pictures are shown, 0.001 pulls both back.
- The sphere map is made from the six pictures at the start, 256 by 256 (0x40de60).
- Vectors and sphere map coordinates are computed at corners and interpolated.

## Differences from the original

- 60 steps a second [I: the original steps once for each frame].
- The help box is left out. Space pokes; scrolling changes the distance.
- The original draws the surroundings as a box 20 wide to each side and tests depth against
  it; here they are drawn first, without.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
