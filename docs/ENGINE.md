# How Racing works

From Ghidra decompilation of the original program and the staged files; no original binaries were run. `[I]` marks inferences.

## How Racing draws

`race.exe` (`0x401eb0`) loads `race.bof`: the track (`track_noroad`), four
cars and the help picture. Lanes are not in the scene file: four tables of
364 points in the program's data (`0x4648b0` counts, `0x4648c0` pointers),
each point six floats. The port reads them from the user's `race.exe`; no
numbers of it are in this repository.

Each frame (`0x402120`) moves every car along its lane by 70 units a second
(`0x401260`), interpolating position and heading, spins the wheels 3.373
degrees per unit of speed and steers the front pair, by replacing the
matrices of the `rollR`, `rollL` and `rear` frames. Cars 0 to 2 take lanes
1 to 3 and car 3 a copy of car 0's lane; car 0 starts two units ahead and
sits four to the side and 0.3 up. Only cars 0 and 3 are drawn (`0x402cd0`).

Views (`0x402120`, keys 1-5): from the cockpit looking back, a chase camera
behind and between the two cars, a camera following in world axes, the
nearest of eight roadside posts zoomed onto the car (`0x4027d0`), and one in
front. Lights (`0x401d50`): 0.6 from above and behind and 0.9 along +z.
The sky is (0.2, 0.3, 0.5); fog is grey from 60 to 100.

### Differences from the original

- The original draws two half-screen views turned a quarter of the field of
  view apart; the port draws one 60-degree view.
- The engine sound (`MCLAREN.WAV`), the free camera and the speedometer are
  not ported.

### Not checked

- Collisions between cars (`0x401c50`).

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
