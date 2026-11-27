# How Grass works

Addresses are in `grass.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences.

- Everything comes from gradient noise, with a table of 256 places in the program (0x4154c4)
  and gradients from the C library's `rand` (0x4068a0, 0x4069f0, 0x406c60).
- Land: noise of 0.13 the place, times 6, or times 1 around the sign (0x402910). The eye has
  100 by 100 plots around it; those that stay are kept as it moves (0x402dd0).
- Each plot has one blade (0x4029b0), moved by up to half a plot, unless it is under water or
  around the sign. Its length is noise × 20 + 10; shorter than 5, it is a daisy. Direction,
  height and how both bend come from noise, too. The width at the foot is a fiftieth of the
  length.
- A blade is a strip of 20, 10 or 5 parts by distance (20, 30) that turns as it goes
  (0x403080, 0x402f20), across the view. The wind adds noise of place and time, times its
  strength and a quarter of the length, to both bends. A strip's normal is the light's
  direction without what is along the blade: hair's lighting, per corner. Highlight 0.5,
  128th power, added after the picture.
- Fog from 25 to 50, by distance from the eye.
- The sky is a dome around the eye, flattened to a fifth (0x408330): four layers of one noise
  picture, each half as strong and twice as fine, moving (0x4086b0), faded to the fog at its
  foot. Three pictures: clouds, plasma, fire (0x4076b0).
- Shadows of clouds: a noise picture over x and z of everything, moving with the wind.
- Water is a plane at -0.5, four tenths there. The sign is a model in the program (0x403af0),
  mirroring a picture of a road as a sphere map.
- 300 flies wander (0x401170). The flare's parts are computed at the start (0x401a90) and lie
  on the line from the sun through the middle (0x402040).
- Grass grows from 20 shorter at the start. After 500 frames without input, the eye flies on
  by itself (0x404f30).

## Differences from the original

- 60 steps a second [I: one for each frame].
- Blades are bent on the GPU, with the original's numbers.
- The sun counts as hidden when land is in the way; the original reads the depth buffer, and
  blades hide it, too.
- Edges of the land's square are lit a little differently: their neighbors are outside.
- The logo is higher up. Its teapot, 200 to the north, help, the frame counter and the model
  view are left out. Wireframe leaves sky and water filled.
- Scrolling is new. Arrows change the speed while held.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
