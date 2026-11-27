# Development

`scripts/dev.sh` lists the commands. Run `all` with the original data to lint native and
WASM builds, verify asset parsing, render in a hidden native window and run Chrome
tests for WebGPU and WebGL2. The comparison checks four frozen frames from each browser
backend against native frames: WebGPU must match at `--max-mean 1.5`, WebGL at
`--max-mean 6` (the grazing ground's mip filtering differs between Metal and ANGLE [I]).

`?time=5&yaw=0.4&pitch=-0.1&key=3` shows a frozen moment. Use the same values with
`scripts/dev.sh shot` to save a native frame.

`scripts/deploy-cloudflare.sh --prepare` stages the web build and lossless assets in
`dist/cloudflare`. Assets larger than 24 MiB are split into numbered parts, and audio
is compressed to FLAC. Git ignores the original files and generated output.

## Original data

The installer `Ogre_1.1.exe`
(sha256 `e5532c43eb93dcd607c24445568b99b4575a8938ce6f7c4b0d13132531f4c054`)
is an InstallShield setup. Unpack it (the `.exe` itself is never run) and point
`OGRE_DATA` at the extracted `files/` directory, which holds `models/`,
`textures/`, `shaders/`, `audio/` and `bin/args.txt`:

```sh
# one way to unpack: unshield the cabinets, then the InstallShield archives
unshield x data1.cab -d unpacked
python3 unpack.py unpacked
OGRE_DATA=/path/to/files scripts/dev.sh check
```

`bin/Spellcraft.exe`
(sha256 `902a9634ef8cb8a4dfc55075139a0d6099c27afe9b625009813ad0ef3a767f81`)
was analyzed statically only: `rea inspect` plus `rea decompile-all`
(2351 functions) with Ghidra, `JAVA_HOME=/opt/homebrew/opt/openjdk@25`
and `GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec`. Kind names and
field layouts in `src/tree.rs` (`NVMatrixTransform`, `NVSubdivisionIFS`,
`NVGeometryKeyframe`/`NVGeometryChannel`, `NVRenderPass`, `NVLightParam`)
were recovered from the decompilation; `[I]` in `docs/ENGINE.md` marks
what was inferred rather than read.

Nine `.nmb` files merge into one performance (`args.txt`, `NVDemoTest:
Merging scene`): `boss_scene_nv31.nmb`, `boss_subd_anim_occlusion.nmb`,
`boss_vest_subd_anim.nmb`, `boss_anims.nmb`, `creepA_anim_h.nmb`,
`creepB_anim_h.nmb`, `creep_Bx_anim_h.nmb`, `creepC_anim_h.nmb`,
`creepCx_anim_h.nmb`. `src/tree.rs` reads each to the last byte
(`tests/data.rs: scenes_parse_strictly_and_round_trip`).

## Coverage

- `the_cast_is_complete`: 21 shapes with geometry, transforms and shaders.
- `the_boss_dances_and_the_creeps_wriggle`: 4000+ baked/subdiv frames, finite
  poses, unit last rows on rest matrices.
- `vest_stays_bounded`: two Catmull-Clark levels keep the vest inside
  [-40, 60]^3 (control bounds are about -19..39).
- `one_quad_subdivides`: L1/L2 topology and planarity on a unit quad.
- `dev.sh smoke`: 252 frames offscreen in 5 s on this machine.
- `dev.sh compare`: native vs WebGPU mean <= 0.0001 on all four stills;
  native vs WebGL mean 4.1-5.3 (ground filtering, see above).

## Shaders

`shaders/ogre.wgsl` hand-ports the Cg 1.1 programs in the demo's `shaders/`:

| Cg | WGSL | Used for |
|---|---|---|
| `colorBumpSpecOcc.vp30` / `.fp30` | `vs_bump` / `fs_bump_occ` | boss body (occlusion) |
| `colorBumpSpec.vp30` / `.fp30` (+`Displace`) | `vs_bump` / `fs_bump` | vest (displace 0.8), creeps, teeth, jewelry |
| `colorBumpDiffuse` | `fs_bump_diffuse` | flat pieces |
| `shadow.vp30` / `shadow.fp30` | `vs_ground`+4-tap `pcf` / `fs_ground` | ground + shadow receiver |
| `background.vp20` / `.fp30` | `vs_bg` / `fs_bg` | backdrop |

Tangent-space lighting with a `phongmap` look-up, `ConstantParam`
`bumpScale`/`shadowAmbient`/`diffuseColor`/`specularColor`/`ambientColor`,
key light (1, 1, 1), one 1024 depth pass. No face culling (the backdrop is
viewed from the inside [I]). Subdivision, shadow filtering and the movie
camera are approximations [I]; see `docs/ENGINE.md`.

## Deploy

Live at <https://ogre-port.pages.dev> (`scripts/deploy-cloudflare.sh`,
project `ogre-port`). Only the built page, the packed assets and the FLAC
audio are uploaded.

## Licence

The original models, textures, shaders and audio belong to NVIDIA; they stay
out of Git (see `.gitignore`). This port's code is part of
`gpu-demo-gallery`.
