# How Donut draws

Addresses are in `donut.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences.

- `.dat` files are TGA pictures, 256 by 256. Their first row, the bottom one, is t = 0.
- The torus has 50 by 50 quads, an outer radius of 1 and an inner one of 0.25 (0x408ab0),
  with a normal, a tangent and their product at each corner. Pictures repeat four times each
  way. 0.8 of the half width for each unit, from 2 away (0x4070f0).
- Light (0x403c90), in the torus' own space (0x403d70): 32 + 192 N·L of 255, and for the plain
  mode a highlight of 128 (R·E N·L)^8, from direction (1, 1, 2).
- Modes (0x4048b0, for two texture units):
  - Plain: picture × light + highlight.
  - Bumps: heights less the same heights moved toward the light, plus 0x7f, all clamped. By up
    to 4 texels of 256 along tangent and binormal (0x408da0).
  - Reflection: `bg.dat` bent as on a sphere (0x407cd0), looked up by the turned normal's x
    and y, mixed with the picture by the picture's 4 bits of alpha (0x405960), × light.
  - Both: bumps, not lit, then twice the product with the reflection; or once, with `b`.
- Motion (0x401000): each frame turns by what the mouse dragged in the last frame, a quarter
  of a degree a pixel, so that letting go keeps it spinning (0x405a00).

## Differences from the original

- 60 steps a second [I: the original moves once a frame].
- What Control did with a letter is on the capital letter, in larger steps.
- Wide windows show more at the sides.
- Left out: the opening pictures, text, the pointer's drawing, turning about the view axis,
  the light's brightness by dragging, and the fallback for one texture unit.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
