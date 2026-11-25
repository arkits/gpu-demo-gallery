# Demoroom engine notes

Based on Ghidra/REA decompilation in `.rea3dfx/demoroom` and `.rea3dfx/apilib`,
plus byte-level validation of the staged `data/` files. No original binaries
were run. This is the ATB pioneer port and owns `crates/atb`.

## Observed data (formats confirmed)

- `demoroom.bof`: `BOFM` container. Header: three big-endian version words
  (2, 1, 5), a `Dict` tag at 0x10, a little-endian entry count (38), then
  `[u8 len][ASCII name incl NUL]` records: `helpscreen`, `0000.3DF` …
  `0020.3DF`, projector textures, `demolite`. The object graph after the dict
  is opaque (raw dumps with baked pointers). `crates/atb/src/bof.rs` reads the
  validated name table; its framing was checked against every staged ATB BOF.
- `*.3DF`: `3df v1.1` / `rgb565` / `lod range: 1 <max>` / `aspect ratio: 1 1`.
  The payload is the LOD pyramid **largest-first**, each texel **big-endian**
  RGB565; total bytes = `2 * Σ w_k*h_k`. Confirmed by decoding `0000.3DF`
  (the animated Egyptian-figure texture) and `SANDM.3DF` (sand). Reader:
  `crates/atb/src/tex3df.rs`.
- `DEMOLITE.3DS`: 3D Studio R4 (the room object — `room01`, `ankh`, `floor01`,
  cameras/lights). Standard chunk tree: `0x4110` verts, `0x4140` UVs, `0x4120`
  faces + `0x4130` material names. All 27 staged `.3DS` meshes across the ATB
  packages parse cleanly.
- `KEY.DAT`, `JOY_MAP.DAT`, `CONFIG.DAT`: input/config mappings.

## Reader status

`crates/atb` has strict, bounds-checked readers for BOF name tables (`bof.rs`),
3DF textures (`tex3df.rs`) and 3DS meshes (`mesh3ds.rs`), with unit tests
(synthetic fixtures) and integration tests against the staged files
(`ATB_DATA=... ATB_SCAN=... cargo test -p atb`; all 27 staged `.3DS` meshes
parse). `crates/demoroom` now **renders real ATB content end to end**: it loads
`demoroom.bof` (38 names), `DEMOLITE.3DS` (7 geometry meshes, 142 triangles,
centred + Z-up→Y-up) and a `.3DF` texture, then draws the textured room through
the shared wgpu `gl` layer. Verified by `examples/shot.rs` (non-blank PNG),
`--smoke` (offscreen frames), clippy native+wasm, and **`scripts/dev.sh all`**
(check / smoke / webtest WebGPU+WebGL2 / compare, all green). It runs in a
window through a winit `Player` and in the browser through the wasm shell
(`web/index.html` + `WebFiles::open` + `DEMOPAK1`/xz pack).

Still future work: BOF object-graph extraction, the original's projected-light
cycling / chromakey multipass / per-object materials. The current draw uses one
texture and baked per-face lighting, not the demo's lighting rig.

## Runtime anchors

- `FUN_00401ed0`: loads `demoroom.bof`, finds `helpscreen` and `demolite`, loads projected/static/animated textures.
- `FUN_00401fd0` / `0x4020c0`: load the 10 static projector and 20 animated `0000..0019` textures.
- `FUN_00401770`: frame/update, projected light cycling, camera/light transforms, chromakey/projected texture passes.
- `FUN_00403430`: frame shell, help/HUD/stats, buffer swap.
- `FUN_004081f0`: file load by extension via loader table.
- `FUN_00406790`: scene draw through a virtual node-draw interface.

## Port plan (remaining)

- `crates/atb`: BOF object-graph access, `.VQ` texture reader, narrow scene/camera/light interfaces. (`.3DS` reader is done.)
- `crates/demoroom`: replace the placeholder renderer with the shared wgpu
  `gl` layer, load the real BOF scene and textures, implement projected-light
  cycling, chromakey multipass, camera/light controls, and native/web shells.
