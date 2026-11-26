# BOF (BOFM): ATB scene files

Status: **Decoded.** Read from the loaders in the ATB demo programs
(`boftoats.c`; `wiz.exe` `0x414620` and `0x411440`) and checked by a strict
reader, `crates/atb/src/bof.rs` in the ATB ports, that reads every staged
`.BOF` to its last byte. An earlier version of this page described a little-
endian dictionary count and a byte-length name table followed by memory
dumps; that was a misreading of the same bytes and is withdrawn.

`atsLoad` (`0x414620`) checks `BOFM`
and version 2, then reads objects until an id of 0 (`0x411440`): each is
its id (counting up from 1), its class name as a counted string, then the
class's fields as that class's loader reads them (vtable slot 0x24). The
stream is big-endian and unpadded. References between objects are ids,
turned into pointers after loading (slot 0x28).

| Class | Fields (after the parent's) |
|---|---|
| Node | two words (a draw mask among them), name, bounding sphere |
| Group | version 1, a 16-bit child count, child ids |
| Frame | version 1, a word, a 4x4 matrix (row vectors) |
| Seq | version 1, bounce, counted, seconds a child, offset, repeats |
| LOD | version 0, centre, ranges (`0x416050` picks the child whose range is the largest below the eye's distance) |
| Anim | version 1, current frame, frame count, id of the matrices (an `Object` block of frames × children × 16 floats) |
| Shape | version 1, (material, geometry) pairs |
| Image | `AtrImg`: Glide format, size, mip levels, file name, texels, and a 256-entry table for palette/YIQ formats |
| Texture | version 2, a handle, image ids |
| Material | `atrMaterial`: 79 words of Glide state (one of them a byte) |
| TriSet | `atrTriSet`: strips of up to 62 vertices of 16 floats, triangles packed as three bytes in a word, and optionally a light-map picture per triangle |
| CMesh | `atmCmesh`: a collision mesh |
| Object | an opaque block of words |

Every staged `.BOF` reads to its last byte. Earlier notes called the object
graph a memory dump with baked pointers; it is not.

A vertex's 16 floats are position, normal, texture coordinates, then a
colour (rgb or rgba); the other slots hold second and third texture
coordinates where a material asks for them.

