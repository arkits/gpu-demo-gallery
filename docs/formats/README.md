# Sushi 2.0 file formats

Recovered from `SushiDX.exe` (addresses in parentheses). The code in
`crates/sushi-formats` is the reference; this is the overview. Little-endian.
"Pointer" fields are stale runtime values to ignore. Counts are signed; < 1 means none.

## Sushi.ati
Ordinary ZIP (deflate). Paths are case-insensitive. `.ati` files *inside* it are
the compiled scene format below.

## .so2 object (0x437d20)
0x18 bytes `SOL RUN00 v00.02.01`, 0x24 bytes `* ATI Sushi 2.0 Material v18 *`,
u32 texture count, 0x1ed texture records, texel blobs (size at record +0x1c4),
u32 mesh count, meshes, 0x28 bounds.

Mesh (0x4366c0), 200-byte record then: shader header; u32 x frames; per frame 0x18
vertex buffer records (stride +8, count +12), each followed by its data in order;
0x10 index buffer records (count +4) and u16 indices; command lists; stream arrays;
stream maps; 0x50 shader params with vec4 values. Shader path at +4.
Counts: frames +0x8c, streams +0x98, index buffers +0xa0, commands +0xa8,
stream arrays +0xb0, stream maps +0xb8, params +0xc0.

Shader header (0x460710): 0x20 magic `* ATI Shader File v2 *`, then misc pairs,
streams, stream maps and textures, all with u32 length-prefixed strings.

## .sa2 animation (0x438350)
0x18 magic `SOA RUN00 v00.02.01`, 0x24 material magic, u32, **f32 duration**, u32
bone count, pointer. (The duration 215.333 reads as ASCII "VUWC"; it is not a tag.)

Per bone: u32 key count, u32 type, pointer, then keys. Each key starts with the
seconds until the next key. Sampling (0x438d10): time modulo duration, walk the
deltas, fraction = (t - start) / delta clamped to 0..1.

| Type | Key size | Key (after dt) | Shared data |
|---|---|---|---|
| 1 | 0x44 | 4x4 matrix | |
| 2 | 0x34 | 3x3, translation | |
| 3 | 0x28 | 3x3 | record 0: translation |
| 4 | 0x10 | translation | records 0-2: 3x3 |
| 5 | 0x20 | quaternion, translation | |
| 6 | 0x14 | quaternion | record 0: translation |

In this demo only types 2 (501 tracks), 4 (146) and 5 (295) occur. In all 942 tracks
the last key has dt = -1 and the other deltas sum to 215.33 s.

Then: 0x10 morph header, meshes, 0x5c param tracks with 0x14 keys, f32 max time,
bounds list (0x28 each), total bounds (0x28).

## .ati scene (0x40bba0; writer 0x40b180)
Two 0x20 magics, 0x19a header with counts, 17 fixed-stride tables, then each
record's variable data in table order. See `scene.rs`. Two tables (0x290 and 0x310
records) are empty in both demo files and rejected if present.

## .pth fly path
Text: `v1.0`, step seconds, then pairs of lines: `x y z fov flag` and quaternion.

## Still unknown
Meaning of many record fields (kept as `raw`), path `flag`, mesh `frames`, scene
cells/groups, texture format/size fields, key blend rule between samples.
