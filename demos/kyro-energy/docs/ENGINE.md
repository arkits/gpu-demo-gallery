# How Kyro Energy works

Addresses are in `energy.exe` as unpacked by `upx -d`. It is the PowerVR "D3DShell" framework
of KyroToon and Still Life (see KyroToon's ENGINE.md for tables, tracks and camera), with the
Procedural EMBM module.

## Scene (0x403000)

30 frames a second, 600 in a loop. Three lights at 0x49c900 (in style 0 Direct01 and Omni01
on, ANIM_Direct02 off, 0x4167b0), 12 meshes at 0x4e6ba0, 5 materials at 0x49c770, camera at
0x4a0298 with tracks at 0x4ecb08. Materials (0x421fa0): the bottle's ("Material #2") has a
diffuse map, `refmap2.bmp` for the environment-bump stage, and `proc.bmp` as light map and bump
map, which are the procedural maps: mode 0x0f. The caps and table have diffuse and environment
maps (0x27); the beer mat a diffuse map (0).

## The frame (0x415fb0)

1. Clear to black.
2. STENCIL_table into the stencil as 1, no depth (0x440980).
3. Meshes named REFLECTION (mirrored copies under the table) where the stencil is 1.
4. The rest. Meshes with vertex colours (the table, alpha 0x7f) are drawn blended by their
   alpha with COLORVERTEX (0x433fb0), so the reflection shows through. Meshes with a light map
   are not lit.

Mode 0x0f (0x439df0): diffuse × light map; BUMPENVMAPLUMINANCE with the drops' bump map,
matrix 0.5, luminance scale 0.7; the environment map added, clamped. Mode 0x27: diffuse, then
the environment map added. Environment coordinates every frame (0x41b810): R = V − 2(V·N)N
for the unit vector to the eye in the mesh's space; u = (R.x + 1)/2, v = (1 − R.z)/2. The label
has no mipmaps (stage 0 MIPFILTER none for "Material #2"); the rest linear.

## Drops (Procedural EMBM, 0x431520 to 0x432a30)

A 256 × 256 height map of 16-bit pixels. Four drops start at random places (Visual C++
rand) with a speed from 1 to 3; every second step each pushes its place into a trail of 50
(one counter shared by all four, as in the original), falls by its whole speed, and goes left
or right by one in a three-state walk (slow drops go straight). Each step the map is cleared,
each trail point is stamped with an 8-pixel strip from the first drop picture's top row,
scaled by up to 0.15 × 0.02 per point, the drops with their 16 × 16 pictures (keeping the
larger height) and ten still drops with an 8 × 8 one. The bump map (0x4327a0): differences of
the height to the right and below, five bits each, and luminance 255 where the height is 10 or
more, else 100 (T and G change it). The light map: white, with the drops' 16 × 16 light
pictures copied in.

## Left out

The menus, text windows, toon and wireframe styles (menu), and the second procedural set.
