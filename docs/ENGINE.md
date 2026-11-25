# How the original works

This port is based on Ghidra/REA decompilation in `.rea3dfx/d6030` and the staged `data/` files. No original binaries were run.

## What the demo is for

The readme states the purpose plainly: show the same scene at 60 fps and at half or
quarter that rate, side by side. The left screen updates every frame; the right
screen only every 2nd frame (30 Hz) or 4th frame (15 Hz, toggled with `R`). The
moving "3Dfx" painting on the wall and the bouncing ball look shakier and blurrier
on the right. `.` steps the animation one frame at a time so the cadence is
visible. `Space` toggles scene spin, `B` the bounce, `1`-`9` recall saved views.

## Files and what the port reads

- **Camera presets** `init1.dat`..`init9.dat`: the loader (`0x4011b0`,
  `0x401130`) reads/writes a 0x1e8-byte state block (`DAT_00431918`) with
  `fread`/`fwrite`. The staged files are 484 bytes. The block contains a camera
  rotation matrix (words 71..90 hold normalised cos/sin pairs) and position-like
  values (words 65/66). `parse_init` in `src/demo.rs` derives a yaw, distance and
  height from those real floats, so the nine presets differ as the originals do.
  Field semantics are reconstructed, not authoritative — see the RE notes on
  `0x401000` (defaults) and `0x402630` (main loop).
- **Textures** `data/data/*.TGA`: the port loads real demo textures — `tile`,
  `wicker`, `paint_a/b/c`, the scrolling `3dfx` painting, `plate`, `chrome`,
  `crystal2`, `bee`. `chrome.tga` and `bee.tga` are run-length encoded (TGA type
  10), so `src/image.rs` decodes RLE packets; the rest are uncompressed type 2.
- **Models** `scene.daf`, `animmodel1.daf`, `bouncing.daf`, `tdxObject.daf`: these
  are raw dumps of in-memory geometry structs with baked pointers (the `cdcdcdcd`
  fill is MSVC's debug heap marker). `FUN_004089b0` and friends (`0x408b40`,
  `0x408e30`, `0x409020`, `0x409290`) show a pointer/count mesh graph. Reconstructing
  those meshes faithfully is a separate reverse-engineering task; the port instead
  rebuilds the room, wall paintings, bouncing chrome sphere and spinning crystal
  object from the demo's own textures and presets, which is what the frame-rate
  comparison is about.

## Render structure

`0x401240` is the frame renderer; `0x402630` sets up clip/viewport for the split
and swaps active/alternate init state blocks between screens. The port mirrors
this by folding full-screen clip-space geometry into the left or right half
(`remap` in `src/demo.rs`) and sampling the right half's animation time at
`floor(t * rate) / rate`. Depth, blending and texturing go through the shared
immediate-mode `gl.rs`/WGSL programs, as in the other small ports.

## Controls

| Key | Action |
|---|---|
| 1-9 | Saved camera position / modes |
| R | Right screen 30 Hz or 15 Hz |
| Space | Toggle scene spin |
| B | Toggle bouncing ball |
| . | Toggle frame-by-frame stepping |
| H | Help banner |
| Drag | Turn camera |
| Ctrl+1..9 | (original) save a preset |

## Port-specific choices

- `FILES` is the exact lower-case set present in the staged `data/`; `Files::check`
  requires all of them. No sound files were found in the load path, so `SOUNDS`,
  `PARTS` are empty.
- `PROGRAMS` uses `plain` and `textured` from `prelude.wgsl` plus a local
  `surface` program for the chrome ball's cheap reflective sheen.
