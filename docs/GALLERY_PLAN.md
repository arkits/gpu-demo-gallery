# Gallery plan: more demos

Status: DRAFT, awaiting decisions (last section). Nothing is built.
[V] checked against the files, [I] inferred, [?] unknown. Installers were unpacked, never run.

## What was downloaded

| Demo | Made by | Engine | Shading | Data |
|---|---|---|---|---|
| Car Paint | ATI, 2002-03 | Sushi; **same exe as Pipe Dream, byte for byte** [V] | 21 demo `.ssh` files, vs.1.1/ps.1.4/ps.2.0 | 24 MB `Sushi.ati`, two WAVs |
| Box of Smoke | NVIDIA, 2006 | Own program, OpenGL + Cg, 176 KB exe [V] | 56 Cg files **in source** [V] | 16 MB of OBJ and raw files, no sound |
| Principles of Shading | NVIDIA, June 2000 | NVIDIA viewer, OpenGL | Texture and combiner descriptions in the models | 72 MB unpacked, four WAVs |
| Creature | NVIDIA, June 2000 | NVIDIA viewer, DirectX 7 | Same kind of descriptions | 25 MB unpacked, three WAVs |

## What already works

Existing crates were run on Car Paint unchanged:

| Car Paint data | Result |
|---|---|
| Objects, animations, camera paths | 3/3, 3/3, 4/4 parse exactly [V] |
| Demo shaders | 20/21 parse; one needs the `1-r0.a` modifier [V] |
| `Globals.ati` | Parses [V] |
| `CarRoom.ati` | Stops at the one 0x290 record, deliberately unsupported so far [V] |

Parsing is not drawing: translation and rendering of these are untested.

## Recommendation: one repository

| Layer | Today | Used by |
|---|---|---|
| Shell: window, web page, audio clock, controls, packing, tests, deployment | `pipedream`, scripts | all four |
| GPU helpers: targets, textures, uniforms, overlay | part of `sushi-engine` | all four |
| Sushi formats, shaders, engine | three crates | Pipe Dream, Car Paint, later ATI demos |
| NVIDIA viewer: models, animation, combiners | new | Principles of Shading, Creature |
| Smoke solver | new | Box of Smoke |

Three of the four demos share an engine with another one, and all share the shell, so
separate repositories would mean copying fixes around. Proposed layout:

    crates/shell  crates/gpu  crates/sushi-*  crates/nv-viewer  crates/smoke
    demos/<id>/demo.toml      title, credits, files, controls, chapters
    web/                      gallery page and player page

One WASM file per engine, so a page downloads only what it needs.

## The gallery

- Home page: a card per demo with a still rendered by the port, maker, year, the GPU it was
  made for, and what it showed off. Cards link to `/pipedream`, `/carpaint`, ...
- Player page: the current one, with download size shown before Start and per-demo controls.
- Same Cloudflare approach: direct upload, content outside Git, xz packs under 25 MiB each
  (unpacked folders, plain xz: Principles of Shading 23.3 MiB, Creature 13.1, Smoke 3.9 [V];
  the first only just fits).
- The current Pipe Dream link keeps working.
- Native: one app with a chooser.

## Phases

**0. Reshape.** Extract shell and GPU crates; move Pipe Dream's constants (scene name, start
time, fades, logos, script) into data. *Check: the same frames as now, and `dev.sh all`.*

**1. Car Paint.** Best value for effort.
- Scene 0x290 records: mirrors. `0x415610` walks them, pushes a matrix from each record and
  enables clip planes (render state 0x98) [V]. That the matrix is a mirror is [I].
- Run `.sds` scripts instead of the hard-coded frame: render targets, viewports, scissor,
  reflections, quads, text. Five chapters, each a script and a camera path.
- Texture format 12: 32 bits per pixel, two 16-bit channels [V]; used for the car's
  2048x2048 normal map. Sound: intro WAV overlapping a loop by 6.92 s.
- *Check: every chapter renders; a timeline sweep has no warnings.*

**2. Box of Smoke.** Independent of the rest, small download, interactive.
- Port the Cg by hand to WGSL. The simulation shaders read 3D grids as flat 2D atlases [V],
  so it needs float render targets. `raycast_f.cg` samples a 3D texture, but the exe does
  not name that file, only the slice and level-set casters [V].
- Recover pass order, constants and grid size from the 176 KB exe.
- Controls: drag moves the source, Ctrl-drag turns the box, N logo, W water, S smoke.
- *Check: solver tests (divergence falls, smoke is conserved) and 60 fps at full grid.*

**3. Principles of Shading.** Opens up the NVIDIA viewer.
- `.nmb` is a tagged binary tree; `.nma` is the same idea as text, so one explains the other
  [I]. Both demos start with the same tags (0x10000021, 0x10000040) [V].
- Combiner setups are data: PASS_THRU, MOD, INTERP and DOT, and a `bumpDiffuse` setup in the
  text files [V]. Generate WGSL from them.
- *Check: all models parse to end of file; every combiner setup translates.*

**4. Creature.** Same viewer, plus animation (`.nab`, 123 channels [I]), skinning, caustics
(32 frames) and cube maps. Highest uncertainty, so last.

**5. Gallery release.** Can go live after phase 1 and grow with each demo.

## Risks

- **16-bit normal map.** One Car Paint chapter compares 8 and 16 bit, so precision is the
  point. Browsers do not filter 16-bit integer textures everywhere; plan on filtering in the
  shader.
- **Clip planes** are not in core WebGPU or WebGL2; use an oblique projection.
- **No recordings** of any of these. Without them, correctness means "matches the recovered
  code", as with Pipe Dream.
- **Behavior lives in code** for the NVIDIA demos (menus, transitions, camera), so expect
  more reverse engineering per demo than the ATI ones.
- **Rights.** Content belongs to ATI, NVIDIA and others. Creature includes `drevil.wav`, whose
  name suggests a film clip [I]; Car Paint credits George Valavanis for its music and models
  a Ferrari F50 (from file names [I]). Publishing is the owner's decision per demo.

## Decisions needed

1. One repository, renamed from `pipedream`? (recommended)
2. Order: Car Paint, Smoke, Principles of Shading, Creature?
3. Which demos may be public, and should `drevil.wav` be left out?

## Reproducing the analysis
`tools/iscab.py data1.hdr data1.cab [out]` reads the InstallShield cabinets (all 285 files
extract at their stated sizes). `unar` opens the outer installers.
