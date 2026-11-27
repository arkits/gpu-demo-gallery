# How the demo works

Recovered from the Cg sources shipped with the demo and from `SMOKE_SIM.exe`
(sha256 of the installer: `550fbc1d7d55a987…`; the exe is dated 2006-09-11). The exe was decompiled with
Ghidra and never run. Addresses are in the exe.

## Data

| File | Use |
|---|---|
| `models/stand.obj`, `cushion.obj`, `shadow.obj`, `glass.obj` | Gold stand, velvet cushion, floor, back plate with its engraving |
| `models/trapped.obj` | 2,074 points: the autopilot's path |
| `textures/{left,right,bottom,top,back,front}.raw` | 512×512 RGB cube map |
| `shadows.raw`, `noise.raw` | 1024×1024 RGB |
| `velvet.raw` | 256×256, one channel |
| `logo.raw` | 128×64 RGBA |

`spiral.obj`, the `.lxo` files and `textures/logos` are not used by the exe as shipped
(the last needs an `oem.txt`).

## The grid

128×64×64 cells of RGBA half floats, kept as one 1024×512 texture: 8 rows of 8 slices. The
original draws the interior, then the walls with texture coordinates moved one cell
inwards. The port does both in one pass: a wall cell computes what its nearest interior
cell would, then applies the wall's scale.

## One step (0x40a7a0)

Time step 2 for smoke, 4 for water.

| | Smoke | Water |
|---|---|---|
| 1 | | Top two rows of velocity set to (0, −0.01, 0); walls copy their neighbor |
| 2 | Advect density: forwards, back, then corrected (BFECC), limited to ±1, × fade 0.985 | The same for the level set, limited to ±128, negated at the walls |
| 3 | Advect velocity, negated at the walls | The same |
| 4 | Vorticity confinement, strength 0.1 | Add 0.2 × height below 24.5; normalized gradient in RGB, level set in A |
| 5 | Add a Gaussian of color and of velocity at the source | Velocity while dragging |
| 6 | | Gravity −0.04 where the level set is below 1 |
| 7 | Divergence, 10 Jacobi iterations from the last pressure, subtract the gradient | The same |

The first-order mode (B) advects once, with confinement 0.19 and fade 0.99. The logo mode
stamps the image into slices 27 to 36 with fade 1 and no color.

The original also draws `air_f` before each Jacobi iteration; the iteration overwrites
it, so the port leaves it out. Its marker particles are compiled in but switched off.

## Control (`main.cpp`)

One step per drawn frame in the original. The port runs 60 a second whatever the display.

- Camera: azimuth 0, polar angle 1.2, distance 2, 45° field of view. After 10 s without
  turning it orbits, speeding up to 0.003 rad a frame. Both idle timers start expired, so
  the demo begins by moving on its own.
- Autopilot: position on the path is 1.3 × the sum of time steps; grid position is path ×
  128 + half the grid, velocity the difference of neighbors × 128.
- The velocity source jitters up to 2.5 cells, with the C library's `rand`.

## Drawing (0x40cf90)

1. Floor, gold, back faces of the glass, plate, velvet (not with water) into the scene image.
2. The volume into its own image: 384×384 for smoke, window size for water. Rays start on
   the box's front; the number of unit steps is its thickness along the view axis. Smoke
   composites back to front. Water finds the level set's zero, refines it three times, and
   shades with a cubic B-spline normal, refraction and reflection.
3. That image blended over the scene.
4. The scene to the window, then glass front faces, the plate refracting the scene, and the
   gold again.
5. Glow: gold outside the box at quarter size, blurred twice each way with −0.02 each time,
   added at 0.8.

Everything is in gamma space.

## Differences from the original

- Box thickness is computed in the shader instead of by subtracting two depth images.
- Sampling is limited to the slice, so neighboring slices do not bleed in at the edges.
- Water's hit point is not stored in half floats first, so it is smoother.
- The cubic weights are computed instead of read from a table.
- Values the original leaves undefined (unwritten alpha, uninitialized variables) are zero.
- Scroll and touch input are new. Debug views (T, C, V, D, E) and frame capture (G) are
  left out.
- `L` (an OEM logo) needs files the demo does not set up, so it is left out.

Not checked: side by side with the original running, Safari, Firefox, phones, Windows and
Intel Macs.
