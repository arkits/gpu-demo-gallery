# How ATB-TV works

Based on Ghidra/REA decompilation and byte-level validation of the staged `data/` files. No original binaries were run.

## What the port renders

ATB-TV (atbtv.exe) draws a TV cabinet with a movie playing on its screen; the movie is a sequence of .3DF frames under movie/. The port loads those real frames plus the cabinet textures (BASE/CYLINDER/TVCABNT/TVSCRN.3DF) through crates/atb and renders a reconstructed frame grid of them.

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
