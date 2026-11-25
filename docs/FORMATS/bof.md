# BOF / BOFM (observed)

Status: **Observed, not documented in ATBRT package**. The name-table framing
below is confirmed against every staged ATB demo and a working reader in
`demoroom/crates/atb/src/bof.rs`. The object graph after the dict is still open.

## Evidence

- ATB demo decompilations contain strings like `boftoats.c`,
  `FILE I/O Error ... boftoats.c`, and demo file names such as `demoroom.bof`,
  `race.bof`, `wiz.bof`. Example demoroom functions:
  `.rea3dfx/demoroom/functions/0x401ed0.c`, `0x411660.c`, `0x4081f0.c`.
- Byte validation across all staged packages: `BOFM` magic, a `Dict` tag at
  0x10, and a dict whose entry counts match the demos — Demoroom 38, Wizards 17,
  Valley-of-Ra 9, Racing 7, Grand Bleu 2, Flip 1, plus every Grand Bleu model
  BOF (1 each). Names always start with `helpscreen` and include texture files
  (`0000.3DF` …) and object names (`demolite`, `start`, `track_noroad`, …).

## Confirmed name-table layout

- `BOFM` magic, then three big-endian version words (e.g. `2, 1, 5`).
- `Dict` tag at offset 0x10.
- A **little-endian** u32 entry count at `Dict + 12` (0x1c): `26 00 00 00` = 38.
- From `Dict + 16` (0x20): one record per entry — `[u8 len][name incl NUL]`
  with a small fixed trailer (an id word; exact semantics unresolved). Names
  are printable ASCII with a NUL terminator; the length includes the NUL.
- Everything after the name table is the scene/object graph: raw memory dumps
  with baked pointers (`cdcdcdcd` MSVC debug-heap fill is visible in related
  `.daf` dumps), not yet decoded.

## Notes

- The staged ATBRT original package does not include a BOF format specification.
- BOF appears to be an ATB scene/object database used by ATB demos. `[I]` Demo
  loaders select a loader by extension and dispatch through BOF parsing code
  (`FUN_004081f0`); the byte-level object schema still needs reader RE.
- The `crates/atb` reader verifies magic, finds `Dict`, bounds-checks the count
  and every name length, and exposes only the validated name table. It refuses
  to fabricate a scene. Encrypted/compressed or variant BOFs would be rejected.
