# What Chimp adds to the Sushi engine

Addresses are in `SushiDX.exe` (the same file as Car Paint's, Pipe Dream's and Bear's),
decompiled with REA and Ghidra and never run. [I] marks inferences. File formats:
[formats/README.md](formats/README.md).

## The show (`Sushi2.ini`, `Scripts/Normal.sds`)

One chapter, `NORMAL`, 66.73 s along `Chimp.pth`. Near 1, far 5000, 50 degrees. The music
is `ChimpIntro.wav`, then `ChimpLoop.wav` from 1.484 s before the intro ends
(`audioIntroOverlap`), both IMA ADPCM. Two `control` cameras (the chimp, a butterfly); the
free camera here is the first.

The script draws the water's reflection and refraction into 800 by 600 buffers that share a
depth buffer, then the objects, the scene, the water and the shadow volumes.

## Water (0x415610)

- `DrawReflections` and `DrawRefractions` are the same routine with a different flag.
  Reflections draw the mirror's second set of the scene (stored already reflected, as in Car
  Paint) and the objects through the mirror's matrix. Refractions draw the scene as
  `DrawScene` does, from the eye, and no objects. Each clears the buffer and its depth first.
- Both keep the far side of the water: the plane at +0x1c8 negated, its distance moved by
  the record's +0x14 (0.1 here).
- `rChimpWater.ssh` mixes the two by a Fresnel term and fogs the refraction towards a murky
  brown, so the stream is dark where nothing lies below it.

## The chimp

- Meshes ending `_pms` are skinned with up to 20 bones; those ending `_m2` (the face, the
  fur at its rim) blend two shapes. The shapes are stored as `short4` positions and `ubyte4`
  normals, tangents and binormals, fractions of the box `SOLBoundBoxMin` to
  `SOLBoundBoxMax` (0x43db70, cases 6 to 8). The box is the one around all of the
  animation's frames; the per-frame boxes put the face beside the head [I: from the pictures].
- Vertex elements may be D3DDECLTYPE 6 to 10 (`short2`, `short4`, `ubyte4n`, `short2n`,
  `short4n`); `short4` and `short4n` take 8 bytes.

## Shaders

- Scene shaders use `exp` (ps.2.0) and plants `vs.2.0` with `mova`, which rounds to the
  nearest address where vs.1.1's `mov a0` rounds down.
- A pass may `Inherit` from several passes in turn. Inheriting a whole pass from another
  file (`oJunglePlants_pms.ssh` from its `.ssi`) brings its textures; its stream map is
  named `file::map`, as the objects name it. Inheriting a block of constants
  (`sJungleCanopy.ssi`) adds them to the pass.

## Differences from the original

- Clip planes, camera splines and text as in the Car Paint port.
- The free camera, pause and skip are new. Menus, stereo and benchmark modes are left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
