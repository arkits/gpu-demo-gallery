# How Small Pond works

Addresses are in `NvWavesGL.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences.

- The surroundings are 117 cubes of photographs (DXT5), taken from morning to evening. Each
  is shown for 5 frames, or for as many as the digit says, and fades into the next (0x40df35).
  A day has 140; the first stays through the last 23, the night. It only goes on once `p` is
  pressed (0x411014).
- They are drawn on a bowl of 64 by 16 and a lid (0x41bd20, 0x40e289). At each corner the
  cube map is looked up toward the corner, a little lower, so that the ground stays below.
- The sun goes over in 110 of the 140 steps and under in the rest, turned by -0.5 and 0.5
  (0x411014). Its height is the brightness of basin and dirt.
- Water (0x410c0d): 81 by 81 points. A ripple knows when its front comes to each (0x408940);
  from then on the point swings as `strength · sin(time - arrival)`. Swings of all ripples add
  up as one sine and one cosine part per point, which lose 5% a frame. At a wall the ripple
  begins again from behind it with a fifth of its strength (0x408d58), up to 25 at once.
- Normals are differences to one neighbor, or to both with `n` (0x4106fd). They are not
  normalized [I].
- First pass: the photographs, by reflection at each point.
- Second pass (0x40ffd9, 0x40f00f): the line from the eye, bent by a quarter of the normal,
  meets the floor or the slanted walls (0x40fc63); the picture of the pool is looked up
  there. It is lighter where the water above that point is high. How much of it shows is
  120 times the square of the bent line's share along the normal. Dirt and leaves lie over
  it. The result is added to what is left of the reflection.
- Basin (0x40e350): a strip of 16 corners with a normal map. Its shadow is a picture on the
  ground, stretched away from the sun.
- The view turns by 0.0001 radians a millisecond until it is dragged; then it keeps a
  hundredth of the last movement.

## Differences from the original

- 60 frames a second [I]. The field of view is taken as 1 radian [I], drags as of a window of
  1024 by 768.
- Fading photographs are drawn one over the other instead of together.
- A step is `.`: Return is full screen. `x`, a click on the water and scrolling are new.
- `r`, `u` and `v`, for measuring speed, are left out. So is the loading bar: everything is
  loaded at the start. Wireframe has one color and leaves out the shadow.
- Photographs have smaller copies for far away, which the original has not.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
