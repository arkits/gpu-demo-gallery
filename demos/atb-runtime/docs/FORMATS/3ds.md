# 3DS mesh (Autodesk 3D Studio R4, observed)

Status: **Observed**. Every ATB demo ships its scene objects as `.3DS` files
(`DEMOLITE.3DS`, `LUDWIG.3DS`, `GALLEON.3DS`, `ATAT.3DS`, …). The layout is the
standard R4 chunk format, confirmed byte-wise against the staged meshes; a
working strict reader lives in `demoroom/crates/atb/src/mesh3ds.rs` and parses
all 27 staged `.3DS` files.

## Confirmed layout

- Every chunk: `[u16 id LE][u32 length LE][payload]`, where `length` includes
  the 6-byte header. Containers nest: `0x4d4d` (main) → `0x3d3d` (editor) →
  `0x4000` (object). The top-level `0x4d4d` chunk must consume the whole file.
- A `0x4000` object begins with a **NUL-terminated name** (`room01`, `ankh`,
  `floor01`, `Camera01`, `Light03`, …), then trimesh chunks:
  - `0x4110` vertices: `[u16 count][ xyz f32 × count ]`
  - `0x4140` texcoords: `[u16 count][ uv f32 × count ]` (optional)
  - `0x4120` faces: `[u16 count][ (a,b,c,flags) u16 × count ]`, followed by
    `0x4130` (face material: NUL name + index list) and `0x4150` (smoothing)
    sub-chunks.
- Cameras/lights (`Camera0x`, `Light0x`) are `0x4000` objects with no geometry
  chunks; the reader still lists them as empty meshes.

## Notes

- This is the classic 3D Studio format, not a 3dfx invention; the ATB runtime
  loads it through `apilib.dll` / the FLT/OpenFlight layer. The ATBRT package
  does not document the file layout.
- The `crates/atb` reader bounds-checks every chunk and array, refuses malformed
  files, and never reads past the end. It exposes geometry and material names
  only; binding those materials to `.3DF` textures is done by the demo.
- Faces carry a fourth `u16` flags word (visibility/mapping) that the reader
  discards; material assignment per face (`0x4130`) is captured as names.
