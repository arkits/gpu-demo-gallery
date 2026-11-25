# How Valley of Ra works

Based on Ghidra/REA decompilation and byte-level validation of the staged `data/` files. No original binaries were run.

## What the port renders

Valley of Ra (fight.exe) ships only fight.bof (4.9 MB) and config files; all geometry/textures are inside the opaque BOF object graph. The port loads the validated dict (helpscreen, an_moves, nef_moves, refl, top, background, ...), and renders a reconstructed frame derived from those real names.

Every ATB asset is read through `crates/atb`'s strict, bounds-checked readers:
`.BOF` (`BOFM`) name tables, `.3DS` meshes, and `.3DF` texture pyramids
(3df v1.0/v1.1, rgb565/argb4444/argb1555/argb8332, largest-mip-first,
big-endian, arbitrary aspect ratios). Compressed YIQ `.3DF` variants and the
BOF object graph after the dict are opaque; the reader refuses them and the
scene loader skips them, and the port reconstructs geometry where needed.

## Differences from the original

- The original's lighting rig, animation paths and audio are not reproduced; this
  port draws the demo's real assets through a shared immediate-mode wgpu renderer.
- `t` cycles the demo's textures; `w`/`p` toggle wireframe and spin.
- The BOF object graph is not decoded, so scenes that ship only a `.BOF` are
  reconstructed from the validated name table.

## Not checked

- Exact frame pacing, camera paths, projected-light/chromakey multipass, audio,
  and per-object materials.
