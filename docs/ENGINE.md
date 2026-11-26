# How Surf works

From Ghidra decompilation of the original program and the staged files; no original binaries were run. `[I]` marks inferences.

## How Surf draws

`surf_glide2x.exe` reads `data\surf.txt` (a 22 by 22 grid of control
points, `0x401660`), `tex.tga` and `env.tga`. Each frame (`0x4012a0`) it
renews any of its four wave sources whose life is over — a random point of
the grid, amplitude 0.5 to 1.5, life 5 to 15 seconds — then sets every
control point's height: for each source within the grid's half-diagonal,
minus (1 - r)^2 times an envelope that rises to 1 at mid-life and back,
times cos(31.4 r - phase), times the amplitude, where r is the distance as
a fraction of the half-diagonal and the phase advances a degree every 8 ms.

The surface is drawn 20 times larger across and 10 times up, z up. The
camera (`0x406c30`, `0x4078f0`) starts at (0, 0, 10) looking along -y; the
mouse turns it a tenth of a degree a pixel and the arrows move it a unit a
frame along its right. The view is 60 degrees, near 1, far 1000; the screen
is cleared to black. Environment map and fog start off.

### Differences from the original

- The original subdivides each patch by its distance; the port uses one
  level for the whole surface. [I]
- Lighting and texture coordinates are the port's own; the patch drawer
  (`0x402d00`) is not followed yet. [I]
