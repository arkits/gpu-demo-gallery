# How ATB-TV works

From Ghidra decompilation of the original program and the staged files; no original binaries were run. `[I]` marks inferences.

## How ATB-TV draws

`atbtv.exe` builds four shapes in code: a 32-sided cylinder of radius 1 and
height pi (`0x401b90`), a 12-unit floor with its texture four times each way
(`0x401e20`), the cabinet's three sides and an arc-shaped screen of radius
sqrt 8 in six pieces (`0x402140`). Each is a lit, textured material (0x5001,
`0x401680`): diffuse 0.8 for the stand, 0.6 for the rest, the screen
glowing 0.3. Lights (`0x4015d0`, `0x402fe0`): ambient 0.25 and a 0.75 light
from (1, 0, -1) towards (-1, 0.2, 1); `L` adds a specular highlight (green
on the stand) or makes the ambient full. The camera is 14 back, turned 15
degrees down (`0x401480`), with a 60-degree view.

Each frame (`0x402df0`) turns the set a degree about y and shows the next
movie frame on the screen's texture (`0x402cc0`). The readme says it plays
every `.3df` in `movie/` in order; the port plays all 73.

### Differences from the original

- The original steps once a refresh; the port steps at 60 Hz.
- The specular exponent follows the material's shininess word. [I]

### Not checked

- The help picture (`tvhelp.tga`), performance display, joystick.

## The ATB engine (shared by the ATB ports)

ATB, 3dfx's Arcade Tool Box, is linked into each demo's `.exe`. The
addresses below are `wiz.exe`'s unless a section says otherwise; the other
ATB programs hold the same code at nearby addresses.

### Scene files (`.BOF`)

`crates/atb/src/bof.rs` reads them. `atsLoad` (`0x414620`) checks `BOFM`
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

### Materials and drawing

`atrMaterialRealize` (`0x4326a0`) maps a material's words straight onto
Glide: `grTexCombine`, `grColorCombine`, `grAlphaCombine`,
`grAlphaBlendFunction`, alpha test, chroma key, cull, clamp, filtering and
depth mask. The port's `glide` fragment program evaluates the same combine
equations, and its pipeline state takes the same blend factors, so each
material looks as its words say. `atrMaterialSetup` (`demoroom.exe`
`0x420830`) builds materials the programs make in code from flags: the low
byte picks a texture set-up, 0xf00 the blend (0x100 multiplies), 0x3000
the colour combine, 0xc000 where colour comes from (lit, vertex colours or
a constant).

Texture coordinates come from the material's mode (`0x4326a0`): the
vertex's own (0x1000-0x3000), a sphere map from the reflected eye ray
(0x4000, `0x43c650`), the per-triangle light-map corners (0x5000,
`0x44ab40`) or a projector's frustum (0x6000, `0x43cb10`).

Lighting is per vertex on the CPU (`0x41e710` sets it up): emissive colour,
plus ambient lights times diffuse, plus directional and point lights by
Lambert's law; a specular term uses the material's shininess word. [I] for
the specular exponent's exact use. Vertex-coloured materials take the
colour slots instead.

The camera (`rcamera.c`): horizontal field of view 60 degrees by default,
near 1, far 65535, looking along its frame's z with y up; the view is the
inverse of its frame (`0x41e480`).

### 3D Studio and VQ models

`tdstoats.c` (`atsflip.exe` `0x410630`) and `vqtoats.c` (`0x40d510`) turn
`.3ds` and `.vq` files into the same objects; `crates/atb/src/tds.rs` does
the same, with the same presets, colours and axes.
