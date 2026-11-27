# How Blobby Dancer works

Addresses are in `BlobbyDancer32.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences. The animation, floor, sky and normal maps are resources of the program; the
looks are Cg 1.1 listings beside it, which the port translates to WGSL when it starts
(`src/fp30.rs`).

- The dancer (`dancer_11_to_28_103v.vab`, read by 0x415ca0) is a tree of 258 nodes that move,
  turn or scale, each number following a key list for each of 12 clips (0x410f10). Its leaves
  hold 103 points: where the blobs want to be. The tree stands at y = -0.115.
- Blobs follow their points on damped springs (0x41b2c0, solved exactly by 0x41a480): friction
  0.2, stiffness 3, inertia 0.2387 from the registry defaults, so the body lags and jiggles.
  Now and then all of them grow lighter or wobble (slow cosine flares), and they bounce off
  the walls of a box from -0.99 to 0.99 with half their speed.
- The field is summed on a grid of 100 cells a side over -1 to 1 (`VoxelDim`): each blob adds
  a table of e^(-0.012 i), shifted to reach zero at 577, at 250 entries for each squared
  radius (0x41be90, 0x419990). The surface is at 0.8; marching cubes (0x41ff40, the usual
  tables, read from the program) runs over the cells near blobs. Normals are differences of
  the field across neighbouring points. Blobs have a radius of 0.1.
- Clips run for their longest key. 1.5 s before the end something happens to the body,
  chosen in the middle of the clip (0x4119e0): 68 times in 100 one of 12 moves, otherwise a
  morph into the next clip. The moves scatter groups of 2, 3, 4 or 24 blobs on sine paths,
  melt them to the floor, drop them under gravity with heavy or stiff springs, freeze them,
  or swirl them into a column. Each eases in over the end of one clip and out over the start
  of the next, with a pause of 0.65 s between (none after a morph). Meanwhile the camera
  pulls back to 2.3 and the body wobbles.
- Looks (21 programs, fresnel_00 to 20): each lasts 6.4 to 10.2 s and fades into a random next
  one (0x4072f0), drawn twice, the second blended. They get time-varying numbers
  (`timefunc`, 0x406350), the sky as a cube, one of 8 normal maps, and for some the last frame
  as a rectangle texture, for refraction.
- fresnel.vp30 gives them the normal and the reflected eye ray in the eye's space.
- The floor (0x4067c0) is a disc of 128 pieces at y = -0.99, drawn with floor2.fp30 over the
  dancer mirrored in it; the mirror is kept to the disc with the stencil buffer.
- The camera (0x4023d0): 60° high, 0.1 to 10, 2 back, turning a degree a frame.

## Differences from the original

- 60 frames a second [I]. The springs are integrated in small steps instead of solved.
- What the moves do to the points is followed in outline; their sine paths, the swirl's order
  and some weights are [I].
- The reflection is cut to the disc in the program instead of with a stencil. Shadows on the
  floor, the dome behind, tracers, the logo and intro, the beat detector that drives the
  director (0x40a280, 0x4139f0) and pulsing blobs are left out.
- The floor's color is white and its reflectivity a slow wave [I].
- `n`, `c`, `r`, dragging and zoom are the port's keys.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
