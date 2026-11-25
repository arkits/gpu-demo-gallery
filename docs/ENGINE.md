# How Racing works

Based on Ghidra/REA decompilation and byte-level validation of the staged `data/` files. No original binaries were run.

## What the port renders

Racing (race.exe) ships only race.bof (3.6 MB) plus MCLAREN.WAV: all geometry and textures live inside the BOF object graph, which is opaque. The port loads the validated dict (helpscreen, neclogo, track_noroad, sgi_indy, ...) and renders a reconstructed frame derived from those real names.

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
