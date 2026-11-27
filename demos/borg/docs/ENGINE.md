# How Borg draws

Addresses are in `borg.exe` and its `gmt.dll` (matrices, window), decompiled with REA and
Ghidra and never run. Textures and their settings are also in `borg.txt`. [I] marks
inferences.

- The eye is at the origin; the cube, 1 wide, turns about one of its corners at
  (-0.5, -0.5, -5). 30 degrees up and down. Each frame turns it a little, about axes that
  change every 64 frames (0x4015f0, 0x402da0).
- Passes, each of two pictures (0x4015f0):
  1. cube1 × cube2 × light. cube2 repeats 3.5 times.
  2. cube3 × cube4, blended as twice the product.
  3. The spotlight, projected from a place of its own, added.
  4. lights, twice at different scales, once turned, added.
  5. The side walls or the front, mirrored, where envmatt has any color (0x401110, 0x401040).
- Passes 2, 4 and 5 are on the four sides around; the two ends have passes 1 and 3.
- Light: 50 ambient and 230 diffuse, of 255, one normal a side.
- Space is two walls at x = ±10 (0x401c10).

## Differences from the original

- It begins with all textures on; the original begins with none and has the presenter add
  them one by one.
- 60 steps a second [I: the original moves once a frame].
- Sides that face away are left out by their plane, and nothing needs a depth buffer.
- Wide windows show more at the sides. Dragging and scrolling are new.
- The help text and the single-texture fallback are left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
