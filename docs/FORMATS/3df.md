# 3DF texture (observed)

Status: **Observed, not documented in ATBRT package**. Layout confirmed against
the staged Demoroom `.3DF` assets and a working reader in
`demoroom/crates/atb/src/tex3df.rs`.

## Evidence

- `.rea3dfx/PRIMER.md` records sample `3df v1.1` headers with `rgb565`,
  `lod range: 1 256`, `aspect ratio: 1 1`, followed by a raw RGB565 LOD pyramid.
- `apilib.dll` image dispatch decompilation includes a `3df` magic/header check
  (`.rea3dfx/apilib/functions/0x1003df20.c`, `0x1003df5d.c`, called from
  `0x1001b75d.c`).
- Byte validation on `demoroom/data/0000.3DF` (256x256) and `SANDM.3DF`
  (64x64): decoding `0000.3DF` at the confirmed layout yields the animated
  Egyptian-figure texture; a downsampled base matches the stored next mip
  (mean-squared error ~9 vs ~1600 for the wrong order), and the 1x1 texel equals
  the base's mean colour.

## Confirmed layout

- Four newline-terminated ASCII lines: `3df v1.1`, `rgb565`,
  `lod range: 1 <max>`, `aspect ratio: <w> <h>`. The binary payload begins
  immediately after the `aspect ratio` line's newline (no pad).
- The LOD pyramid is stored **largest mip first**, descending (256, 128, …,
  1), row-major.
- Each texel is a **big-endian** RGB565 word (high byte first). This is the
  opposite byte order to a naive little-endian read; getting it wrong produces
  psychedelic noise.
- Total payload = `2 * Σ (w_k * h_k)` over all levels. For square 1:1 with
  `lod range: 1 256` that is exactly 174762 bytes; a 174813-byte file has a
  51-byte header. Readers should verify this sum and reject any mismatch.

## Notes

- This is the classic 3dfx texture format family used by Glide-era tools and
  demos. The ATBRT package names texture-related components but does not
  document the file layout.
- The reader in `crates/atb` refuses unknown versions/formats, non-square aspect
  ratios (the only kind observed), and truncated/inconsistent payloads. The
  object-graph side (how demos bind these textures) is a separate concern.
