# Ogre engine notes

From static analysis of `bin/Spellcraft.exe`
(sha256 `902a9634ef8cb8a4dfc55075139a0d6099c27afe9b625009813ad0ef3a767f81`)
with Ghidra through `rea-agents@6.10.1 inspect` and `decompile-all`
(2351 functions), and from the demo's files. The program was not run.
[I] marks inference; the rest was read in code or checked against the data.

## Files

`Ogre_1.1.exe` (sha256 `e5532c43eb93dcd607c24445568b99b4575a8938ce6f7c4b0d13132531f4c054`)
is an InstallShield setup. Unpacked, `bin/args.txt` lists the scene and the eight
animation files it merges (`NVDemoTest: Merging scene`):

- `models/boss_scene_nv31.nmb` (the stage: ground, backdrop, string, transforms)
- `models/boss_subd_anim_occlusion.nmb` (boss body, with baked tessellation + occlusion)
- `models/boss_vest_subd_anim.nmb` (vest, subdivided at runtime)
- `models/boss_anims.nmb` (boss skeleton/geo keyframes, skirts, belt)
- `models/creepA_anim_h.nmb`, `creepB_anim_h.nmb`, `creep_Bx_anim_h.nmb`,
  `models/creepC_anim_h.nmb`, `models/creepCx_anim_h.nmb` (five creeps, baked frames)

`src/tree.rs` reads all nine `.nmb` files to the last byte. Tree framing as in the
earlier viewers, with new kinds: `0x80000003` is `MatrixTransform` (flags u32 + 16
floats, row-major [V]); `0x80000004` is `MaxTransform` (rotation/scaleRotation quats,
scale, translation, extra) with no children (shape nodes hang off the scene's field 9).
`GeometryKeyframe` is a `Vector(2)` range plus `Many` vertex arrays; `TransformKeyframe`
repeats field 1 (rotation V6 then scaleRotation V4 [V]) and ends with a double `-4` marker [V].

- Strips (primType 5) restart at `0xffff`; creep eyes use strips.
- `StaticSubdivisionLevel` 0 renders the control mesh directly (skirts, belt);
  level 1+ is two Catmull-Clark levels in the port [I].
- Boss body ships baked tessellated topology (`tess`/`tess_uvs` split maps):
  5087 tess verts map to 3845 control verts with 16 first-wins split conflicts [I].
- CreepB's 506 frames are baked per-frame verts; no runtime tessellation is needed for them.
- Vest transform (255, 76, 7) plus rotation is identical in all three boss files [V].

## Drawing

Cg 1.1 programs ship as source in `shaders/`: `colorBumpSpec.vp30` /
`colorBumpSpec.fp30` (tangent-space bump + `phongmap` look-up + shadow test),
`colorBumpSpecOcc` (same plus `a2v.occlusion`), `colorBumpSpecDisplace`
(same plus `displace 0.8` along the normal for the vest [V]),
`colorBumpDiffuse`, `shadow` (4-sample PCF, 1024 map, blur width 4) and
`background`. `shaders/ogre.wgsl` hand-ports them.

| Original | Port | Shapes |
|---|---|---|
| `colorBumpSpecOcc` | `fs_bump_occ` | boss body (occlusion), boss head extras |
| `colorBumpSpec` / `colorBumpSpecDisplace` | `fs_bump` | vest (displace), creeps, teeth, jewelry |
| `colorBumpDiffuse` | `fs_bump_diffuse` | ground-adjacent / flat pieces |
| `shadow` | `fs_ground` | ground (`ogre_ground2.dds`), shadow receiver |
| `background` | `fs_bg` | backdrop (`ogre_bg2.dds`) |

- Light direction is (1, 1, 1) normalized [V]; `bumpScale`, `specularColor`,
  `diffuseColor`, `ambientColor`, `shadowAmbient` come from each shape's
  `ConstantParam`s [V].
- Shadow map is a single 1024 depth pass; the port uses one `sampler_comparison`.
- No face culling: the backdrop is viewed from the inside [I].

## Running

- `args.txt` opens at 1024x768 with `audio\ogreWurly.wav` on repeat.
- The movie camera eases around the boss at radius 650, height 0.05, yaw drift
  0.05 rad/s [I]; free orbit clamps pitch to -90..+45 deg and distance to 40..2000.
- Free camera looks at (0, 70, 0) [I].
- After 120 s without input the view returns home [I].

## Left out or approximate

- Subdivision surfaces, lighting falloff and shadow filtering are approximations [I].
- The 4-tap PCF uses hardware comparison instead of the exact Cg offsets [I].
- The string, badge (`nv_badge.tga`) and UI textures render unlit [I].
- CreepB's baked frames are interpolated linearly; the original curve type is Hermite-like [I].
- The original menu / camera cuts are not reproduced; there is one continuous shot [I].
