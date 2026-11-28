# Development

`scripts/dev.sh` lists the commands. Run `all` with the original data to lint
native and WASM builds, run the strict parser tests, render in a hidden native
window and run Chrome tests for WebGPU and WebGL2. The comparison checks one
frozen frame per take natively, in WebGPU and in WebGL2.

`?time=5&take=3&yaw=0&key=` shows a frozen moment. Use the same values with
`scripts/dev.sh shot` to save a native frame (shot takes
`SECONDS TAKE YAW PITCH KEY OUT.png`).

`scripts/deploy-cloudflare.sh --prepare` stages the web build and lossless
assets in `dist/cloudflare`. Assets larger than 24 MiB are split into numbered
parts (`scripts/pack_parts.py`, manifest `assets.json`), and audio is
compressed to FLAC. Git ignores the original files and generated output.

## Reproducing the port

Original installer (not in git):

```sh
ls -la /Users/archit/Dev/gpu-demos/NVIDIA-demos/MadModMikeSetup.exe
# 160203454 bytes, SHA-256 9d99e32451ccd93ac8cf784f95eb50df8dc6b239ff1f3530b82864357905eb2d
```

Unpack (InstallShield; the exact tool does not matter, only the bytes below):

```sh
# produces $OUT/files/{bin,models,shaders,textures,audio,ui} (~280 MB unpacked)
sha256sum $OUT/files/bin/MadModMike.exe
# 37fa2497caaa8b3fec7471835cd7199976fbf01768ce417d7c7a01393cf0b875
ls $OUT/files/models | wc -l        # 24 (.nmb twins and .nma fragments)
ls $OUT/files/textures | wc -l      # 448
ls $OUT/files/shaders | wc -l       # 224 + custom/
```

Point the port at the unpacked tree (or copy/symlink it to `data/`):

```sh
export MADMODMIKE_DATA=$OUT/files
scripts/dev.sh check
scripts/dev.sh all
```

## Reverse engineering evidence

- `npx -y rea-agents@6.10.1 inspect bin/MadModMike.exe` with
  `JAVA_HOME=/opt/homebrew/opt/openjdk@25/libexec/openjdk.jdk/Contents/Home`
  and `GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec`:
  PE x86, image base `0x400000`, 11,049 procedures, 5,166 strings
  (evidence `ev_2007577b6122b1231f90713e711df56c143b8fcba13c5a2b811c30000`).
- `rea decompile-all` into `decompile/` (11k functions; the program was never
  run). Node registration tables used for `src/tree.rs`:
  `0x4d17d0` Scene (15 fields), `0x50f190` PointLight (6),
  `0x4f07f0` ProcessedMesh/`0x70000038` (8), `0x504270` MayaFloatChannel (3),
  `0x4f6f80` Shader (5), `0x4c7fe0` 2DTexture (3), `0x4d5610` Scene sounds,
  plus ~70 more kinds enumerated in `docs/ENGINE.md`.
- Field-type vtables cluster strings, floats, ints, 1-byte bools, counted
  arrays and node lists; every assignment in `tree.rs` was validated by
  parsing all 26 scene files strictly to the last byte
  (`tests/data.rs::assets_parse_and_round_trip`, `text_scenes_parse_strictly`).

## Coverage

- Readers: all 12 `.nmb` files parse with one schema; all 14 `.nma` files
  parse, including the `particles.nma` corrupt tail (`["0", "o3"]`, asserted).
- Model: 1,591 shapes, ≤21 bones each (limit 96 in the shader), ≤90 morphs;
  every bone index and vertex index is range-checked (`bones_look_sane`).
- Blend/morph mapping: `<geometry>_blend_0_<N>` weight nodes drive
  `objCoordDiffN` streams 1:1.
- Enum values pinned against the binaries: `primType` 3/5/8, `interpMethod`
  1/2, pre/post 0, `profile` fp/vp 6151/6148, texture `format` 25, `map`
  1/4/5/6/7/9/23/26/43 via param-name cross-reference, `property` 0/1,
  `space` 1. Text-only enums (`cull` words, blends, filters, wrap, depth/stencil
  funcs) map to wgpu equivalents and are covered by render tests, not by bytes.
- Rendering is deterministic across runs: texture-role maps are `BTreeMap`s,
  so texture choice does not depend on hash order (native and web stills
  compare pixel-identical on WebGPU).

## Shader table (Cg → WGSL, all by hand)

| Original programs | Port entry | Notes |
|---|---|---|
| `mike*.fp30`, `room.fp30` | `fs_main`, `fs_skin`, `fs_room`, `fs_emissive` | Signed HILO/DXT5 normals, authored occlusion, animated lights, projected key shadows |
| `screen_old*`, `screen*` | `fs_screen_old`, `fs_screen`, `fs_keyboard` | Amber CRT, original mermaid tiles, emissive keyboard |
| `mike_wetball_*` | `fs_wetball` | Transparent cornea with analytic gloss |
| `hair*.vp30`, `hair*.fp30` | `vs_hair`, `fs_hair` | Camera-facing ribbons attached to the head |
| `particle_*` | `vs_smoke`, `fs_smoke`, `vs_streak`, `fs_streak` | Filed tool and jet emitters with deterministic animation history |
| `flames*`, `composite_heat.fp30`, `composite.fp30` | `vs_flame`, `fs_flame_mask`, `fs_composite` | Depth-tested heat mask, refraction, glow, depth-aware softness |
| `cast_hard_omnishadow*`, `*ShadowCube*`, `cube_blur_*` | `vs_shadow` for the primary source | The original cube-map and radiance loops remain unported |

Compare the opening and doorway, the grinder and welder, the flyaway smoke
trail, and the upgraded display against
[NVIDIA's original capture](https://download.nvidia.com/downloads/nZone/videos/nvidia/640-1mbits-NVIDIA-MadModMike.wmv).
The [2005 NVIDIA presentation](https://download.nvidia.com/developer/presentations/2005/SIGGRAPH/Truth_About_NVIDIA_Demos.pdf)
documents the original's lighting, shadow and depth-of-field passes. The
backend still comparisons only measure consistency across the port's renderers.

## Deploy

```sh
scripts/deploy-cloudflare.sh            # project mad-mod-mike-port
scripts/deploy-cloudflare.sh --prepare  # assemble only, no upload
```

Play: https://mad-mod-mike-port.pages.dev

## Verification steps

1. `export MADMODMIKE_DATA=<unpack>/files` (or copy it to `data/`).
2. `scripts/dev.sh check` — fmt, clippy (native + wasm), all tests.
3. `scripts/dev.sh smoke` — hidden-window frames, grep `^frames`.
4. `scripts/dev.sh webtest` — Chrome WebGPU + WebGL2, music playing.
5. `scripts/dev.sh compare` — native vs WebGPU vs WebGL2 stills.
6. `scripts/deploy-cloudflare.sh --prepare && ls dist/cloudflare/local`.
