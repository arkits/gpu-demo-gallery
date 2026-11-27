# How Lightning works

Addresses are in `lightning000419.exe`, decompiled with REA and Ghidra and never run. [I]
marks inferences.

- Pictures are PNG resources, sounds WAVE. Pictures are stood upright for OpenGL.
- The eye circles the middle at up to 6, looking at it; 0.8 of half the height for each unit
  (0x407480, 0x407830). It turns by itself.
- Two rooms, 6 and 10 to each side (0x415f10), the second seen through holes in the first: an
  alpha test above 0.1. Balls and rods between (0x40cd40). The plate is 2 to each side, at
  y = -2 (0x411940).
- The bolt goes from (0, 2, 0) to where a ray through the pointer meets the plate. It has 128
  parts, made by moving middles by up to 0.3 of the length; nine in ten are kept from one
  frame to the next (0x4017f0, 0x4014f0). It is drawn as a band that faces the eye (0x402040).
- Lights (0x407520): one above, always; three along the bolt, at parts 64, 32 and 96; the four
  large sparks. Highlights to the 64th power, the eye's place taken into account.
- With bumps (0x416ec0, 0x412820): the walls are first drawn at 0.7, unlit, then a second time,
  added, times the picture's alpha: the product of a normal picture and the direction half way
  between light and eye, to the eighth power, by register combiners (0x41fce0). The walls'
  light is 1.2 a + b + c of the bolt's three points. The plate gets the same on top of its
  usual light, from part 96, or from far above when there is no bolt.
- Normals come from heights, times 6 for the plate and 4 for the walls (0x424250).
- The plate has 32 by 32 heats. A strike adds 0.1 of what is missing; they spread and are
  lost (0x413610). Heat t adds (t, t², t³).
- Sparks: large ones start in 7 of 100 frames, small ones five in each. They fall, bounce off
  frame, plate and walls with 0.8 of their speed (0x41b750, 0x418d40) and fade. They are drawn
  as kites along their way (0x41bc90).

## Differences from the original

- Lights are computed for each pixel; the original does it at corners of fine grids (walls 32
  by 32) and interpolates.
- 60 steps a second; the original mixes steps and measured time.
- What is nearer than 1 to the eye is drawn; the original cuts it off.
- The three random numbers of a middle are taken to be independent [I]. Balls are divided
  from octahedra [I]. The order of a kite's corners is inferred [I].
- Space, scrolling and wider views in upright windows are new. Help is left out. Sounds of
  sparks and bounces have one volume.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
