# Zoltar engine notes

From static analysis of `bin/Zoltar.exe` with Ghidra, through `npx -y rea-agents@6.10.1 decompile-all`, and
from the demo's files. The program was not run. Addresses are of that file. [I] marks inference.

## Drawing

- `zoltar.nmb` lists every shape's passes: vertex program, textures, combiners, lights, blending and depth.
  `src/pass.rs` reads them. `src/vp.rs` turns `!!VP1.0` programs and `src/combiner.rs` the texture shaders
  and register combiners into WGSL. Shapes without a program use OpenGL's lighting and texture generation
  (`0x485c70`).
- A clip is a scene of its own: the face as whole meshes every 33 ms, which the vertex program blends, and
  curves for joints. Scale channels are taken out (`0x402e20`, `0x403000`). Joints that a clip does not move
  stay as the one before left them.
- The light in the scene is a placeholder. The program sets its direction in the camera's space to
  (0.62, -0.38, 0.67) (`0x401c70`; the L key, `0x403700`).
- Units with the texture `phongE10_tex_e`, the cube map of lights, get a texture matrix `followsLight` that
  turns the direction toward the light to +z (`0x401920`, `0x480520`). It acts after the turn to the shape's
  space of `object_reflect`.
- The gem is at (0.8, 0.8) with a size of 0.2 by 0.2 in a unit square (`0x403500`), and the logo at (0.8, 0)
  with 0.2 by 0.07 (`0x40b9f0`). The port keeps the shapes that these have in a 4:3 window.
- The picture is opaque. The original leaves combiner alpha in the frame buffer, and nothing reads it
  (tested), but a page would show through a WebGL canvas.

## Running

- Director, each frame: `0x402220`. Start and end of clips: `0x405c30`, `0x405c50`.
- 5 s after the start the greeting. 15 s after the last clip something by himself: if `rand() / 32767` is at
  most 0.8 the next idle clip, else the next fortune. `rand` is the C library's, never seeded [I].
- Fortunes and idle clips come in the order of their files (`0x402ad0`, `0x402920`), not at random. Nothing
  interrupts a clip but X.
- Voices start after the milliseconds in `fortuneAnimations.txt`, `greetingsAnimation.txt` and
  `idleAnimations.txt`. `src/demo.rs` has these, and the lengths of the clips, which a test compares.
- Sound (`0x401510`): `background_soft.wav` goes from 1 to 0.5 in the first 2 s of a clip and back after it.
  `OnEffect.wav` at 0.2 for fortunes, until the voice starts.
- Camera: 0.7 rad high (`0x474f00`), at (0, 12, 0) from 75 away, 40 to 200 (`0x401030`). Drags turn it by
  half a circle per window and it goes on turning when let go (`0x41c380`, `0x41c450`). Never from below
  (`0x41c520`). The port ends the spin if the pointer was still for 100 ms, as the program does for the light.
- Dragging the light: the window is a hemisphere, 0.5 of its width from the middle is sideways (`0x41d3c0`).

## Left out or approximate

- The menus with their pictures, the draw styles (P, W, S), walking and spherical cameras, panning, and
  the light's own spin. P pauses here, which Zoltar cannot do.
- The original's far plane is at most 100 away, so he vanishes when zoomed out far. Here it is 400.
- The direction of the light is given its length of one, where the original's is 0.99 [I].
- The 9-bit registers are clamped but not rounded. Mipmaps are box-filtered from the first level.
