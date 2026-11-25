# Port plan: the second batch

Status: Families A–D have native and web ports. The gallery now covers these two
batches. Families E–H remain outside the current scope. The original packages were
statically inspected with REA and Ghidra.

## Families C and D results

Toy Soldier and Crystal Ball use the first-generation viewer reader and the native,
WebGPU and WebGL2 shell copied from Creature. Bugs, Wolfman, Squid, Chameleon,
Tidepool and Zoltar use the second-generation Scene reader. The asset format,
geometry streams and animation channels were resolved from REA and Ghidra output.
Tidepool's 12 position arrays and Chameleon's 165 position arrays match the
corresponding text scene coordinates within 0.002 scene units.

The rendering effects are implemented in WGSL from the scene data, decompilation
and vertex listings. The individual ENGINE notes record approximations and remaining
original effects. Zoltar's 134 MiB compressed pack is split into six download parts.

Native Metal and Chrome WebGPU and WebGL2 render checks passed. The macOS packages
were smoke-tested and the x86-64 Windows executables were cross-built.


[V] checked against the files, [I] inferred, [?] unknown. Nothing downloaded was run.

Covers `NVIDIA-demos` (36 packages) and `Soggi-demos` (10). Pipe Dream's data and program are
byte for byte the ones already ported [V], and ChameleonMark is Chameleon plus benchmark
scripts and text versions of its models [V]. That leaves **44 demos: 35 worth porting in
seven families, 9 not recommended.**

## 1. Findings that shape the plan

1. **The three new Radeon 9700 demos run on `SushiDX.exe`, the same file as Pipe Dream's** [V:
   SHA-1 91cd8d6e...]. The existing crates read almost all their data already (section 4).
2. **NVIDIA kept one scene format**, binary 2000 to 2007 and its text form to 2010 [V for the
   framing]: `u32` kind, numbered
   fields, -3 name, -4 end. `src/tree.rs` reads the framing already. Only the table of kinds grows.
3. **That table can be extracted by machine.** Each node's constructor registers its fields by
   name, in order, with a typed accessor. `rea decompile-all` on `Squid.exe` gave names, order
   and types for 30-odd node kinds [V]. Kind ids and a full parse are still to do.
4. **There are answer keys.** Toys, Tidepool and ChameleonMark ship the same scenes as text
   (`.nma`) and binary (`.nmb`) [V], so the binary reader can be tested against the text.
5. **Shaders are mostly source**: about 1,580 Cg/HLSL files and 112 `!!VP1.0` files [V].
   With a 25-line prelude, `slangc -target wgsl` compiled 644 of 1,576 (41%), and naga accepted
   610 of those [V]. Failures are dialect differences (`texobj2D`, implicit vector truncation,
   parameter order, file-level macros). Useful, not push-button: budget for hand work.
6. **Licences.** Eight NVIDIA packages (Ogre, Timbury, Blobby Dancer, Clear Sailing, Mad Mod
   Mike, Time Machine, Toys, Vulcan) contain a licence saying the software may not be
   "distributed", nor reverse engineered [V]. 3dfx Donut's forbids use on other platforms [V].
   The first batch's sites bundle the content. See decision 1.

## 2. The demos

Size is the data as plain xz / audio before FLAC. Pages allows 25 MiB *per file*, so larger
packs must be split. Effort is relative: S about like Box of Smoke, M like Car Paint,
L like Pipe Dream from scratch.

### A. ATI Sushi, Direct3D 9 (existing crates)

| Demo | Shows | Size MiB / MB | What is new | Effort |
|---|---|---|---|---|
| NPR Hatching | Pencil hatching, edge detection | 2.1 / 0 | Three render targets at once, `Inherit(this, shader, pass)`, sampler `Clamp(cc)` | S |
| Rendering with Natural Light | HDR balls, bloom, exposure | 7.5 / 4.0 | Float targets, a 16-bit cube map of unknown type, `#include` in shaders | S |
| Bear | Fur shells and fins | 15.3 / 0.4 | Skinned geometry (kept in the `.sa2`), 23 bones, `#include`, commas in the camera path | M |

### B. Small programs, 1998 to 2000

| Demo | Year, API | Size | Where the content is | Effort |
|---|---|---|---|---|
| Grove | 2000, OpenGL | 1.2 MB exe | 19 PNG resources | S |
| Lightning | 2000, OpenGL | 2.2 MB exe | 9 PNG and 7 sound resources | S |
| Bubble | 1999, OpenGL | 1.1 MB exe | 7 PNG resources; CPU spring mesh, cube map | S |
| Donut | 1998, Glide 2 | 0.9 | `.dat` files; bump and environment map | S |
| Borg | 1998, Glide 2 | 0.4 | 14 images; `borg.txt` documents every pass | S |
| Grass | 2000, OpenGL | 3.7 / 5.5 | 7 TGA; combiners | S |
| Small Pond | 2000, OpenGL | 17.3 / 3.8 | 702 cube faces as DDS; water simulation in code | M |

### C. NVIDIA viewer, first generation (as Creature)

| Demo | Size | What is new | Effort |
|---|---|---|---|
| Toy Soldier | 2.5 / 11.4 | Root kind 0x10000010, levels of detail | S |
| Crystal Ball | 7.6 / 14.5 | Kind 0x100000b1; cube map refraction | S |

### D. NVIDIA viewer, second generation (GeForce3 and 4; `Scene` root)

| Demo | Size | Programs | Effort |
|---|---|---|---|
| Bugs | 5.6 / 4.6 | none; combiners, shadows, flocks | M |
| Wolfman | 14.9 / 9.0 | 28 vertex programs; fur, skinning, shadow map | M |
| Squid | 16.2 / 14.9 | 18 vertex programs; steering | M |
| Chameleon | 17.0 / 16.6 | 25 vertex programs; the skins | M |
| Tidepool | 27.6 / 22.4 | 36 vertex programs; water, Z-correct bump | M |
| Zoltar | 134.8 / 13.5 | 5 vertex programs; 12 large speech animations | M, size |

New for the family: `!!VP1.0` to WGSL (like the Sushi assembler), texture shaders (dot product
and dependent reads), combiners from Principles of Shading.

### E. ATI Sushi, Direct3D 8

| Demo | Size | What is new | Effort |
|---|---|---|---|
| Dolphin | 3.1 / 0 | Older file versions (v54, `v00.01.11`), 116 ps.1.4 programs, TruForm surfaces | M |

### F. NVIDIA NVDemo with Cg (GeForce FX to 7)

| Demo | Size | Notes | Effort |
|---|---|---|---|
| Blobby Dancer | 0.5 / 4.1 | Own program; metaballs; 28 fp30/vp30 listings | S |
| GeoForms | 1.5 / 0 | Direct3D 9; metaballs that follow sound | M |
| Vulcan | 26.4 / 15.6 | Flames, soft shadows | M |
| Time Machine | 35.4 / 12.2 | Aging paint, rust, chrome | M |
| Toys | 29.8 / 27.2 | Depth of field; **has the text twins** | M |
| Gas Station | 40.2 / 30.2 | Soft shadows | M |
| Clear Sailing | 52.7 / 22.1 | Ocean, ropes, HDR | L |
| Timbury | 58.0 / 13.0 | HDR, bump, crowds | L |
| Mad Mod Mike | 81.7 / 20.2 | Many scenes, hair | L, size |
| Ogre | 171.3 / 7.3 | Subdivision surfaces; third-party art (Spellcraft) | L, size |

### G. GeForce 8

| Demo | Size | Notes | Effort |
|---|---|---|---|
| Human Head | 68.0 / 0.7 | 17 offscreen passes; a real actor's likeness | L |
| Froggy | 154.9 / 10.4 | Simulated skin, stream out | L |
| Cascades | 117.4 / 5.6 | 29 geometry shaders, stream out: compute in WebGPU, hard in WebGL2 | L |

### H. Other engines, formats unknown

| Demo | Size | Notes | Effort |
|---|---|---|---|
| Radeon's Ark | 49.4 / 19.7 | ATI "Proteus", Direct3D 7, Sushi's ancestor [I]; clear file headers | L |
| Matrox G400 | 22.5 / 0 | Softimage text files and one 36 MB data file | L |
| Dawning | 17.2 / 54.2 | One `.ras` that looks compressed or scrambled [I] | L, risky |

### Not recommended

| Demo | Why |
|---|---|
| Apollo 11 | Unreal Engine 4, 1.4 GB |
| Unigine Heaven, Stone Giant | Other companies' engines and products |
| Design Garage | Ray tracer on CUDA and OptiX; 937 MB of scenes |
| Supersonic Sled | CUDA, PhysX, Direct3D 11 tessellation; 111 MiB |
| Alien vs. Triangles, Endless City, FaceWorks | Tessellation; 240 to 294 MiB; contents of the `.npk` are scrambled [V] |
| G-SYNC Pendulum | Exists to show a monitor feature; OptiX; `.npk` scrambled [V] |

## 3. Approaches

**1. One demo at a time, copying code, as so far.** Simple, independent repositories. With 35
demos the copies drift: Car Paint's `sushi-engine` already differs from Pipe Dream's in 9 files [V].

**2. One repository per demo, with shared crates as Git dependencies (recommended).** This keeps
your decision of one repository per demo ("copied or used as a dependency"). Shared: `sushi`
(Car Paint's copy, which is the larger one), `nv-scene` (schema, reader, animation, combiners,
programs), `demo-shell` (window, page, audio clock, packing, test driver; `native.rs` in
Creature and Principles of Shading differ by 33 lines of about 400 [V]). Existing five demos stay as they are.

**3. One player that runs any NVIDIA scene.** Least code per demo in theory, but each demo's
behavior (menus, cameras, simulation) is in its program, so it would not save that work.

## 4. What the trial runs showed

Car Paint's crates, unchanged:

| | NPR | Natural Light | Bear |
|---|---|---|---|
| Objects, animations | 3/3, 3/3 | none | 1/1, 1/1 |
| Scenes and globals | 2/2 | 2/2 | 2/2 |
| Camera paths | 1/1 | 3/3 | 0/1 (commas) |
| Shader files that fail to parse | 2 of 12 | 15 of 28 (14 `#include`) | 5 of 20 (4 `#include`) |

Parsing is not drawing; none has been rendered.

REA 6.10.1 with Ghidra 12.2 imported `Squid.exe` (1.3 MB) in 59 s. Evidence:
`ev_e3e260d4` (overview), `ev_0ca114f5` (decompilation of 0x400000 to 0x500000).
The MCP registration does not start, because the npx folder it points to is gone. The CLI
works with `GHIDRA_INSTALL_DIR` and `JAVA_HOME` set as in that registration, and is enough for all of this.

## 5. Pipeline for each demo

The same as the first five.

| Step | Output | Gate |
|---|---|---|
| 1. Unpack, never run | `data/` (ignored by Git), inventory, SHA-1 | Cabinet MD5s match |
| 2. Reverse engineer | `rea inspect`, `rea decompile-all`; `docs/ENGINE.md` with addresses and [I] marks | Frame order, constants, controls and timeline listed with their source |
| 3. Read | Strict readers | Every file to its last byte; index and key checks |
| 4. Shade | WGSL, translated when packed or by hand | naga accepts every used program |
| 5. Draw | `examples/shot.rs` | Still frames; sweep of the timeline without warnings |
| 6. Play | Native window, web page, controls, sound | `dev.sh smoke` |
| 7. Test | `dev.sh all` | WebGPU and WebGL2 within mean 1.5 of 255 of native; the canvas's API is checked |
| 8. Package | macOS app, Windows exe, xz and FLAC, split above 25 MiB | Pack verifies against the originals |
| 9. Publish | Site per decision 1, gallery card and picture | Browser test of the live site; content not in Git |

Reverse engineering is the only part that differs by family. Families share steps 3 to 5.

## 6. Order

0. **Foundations.** Shared crates from existing code; `slangc` and schema extractor as tools.
   *Gate: Car Paint on the shared `sushi` renders the same frames as today.*
1. **Family A**: NPR Hatching, Natural Light, Bear. Best value: engine known.
2. **Family B**: Grove, Lightning, Bubble, Borg, Donut, Grass, Small Pond. Independent of
   everything, all small. Can run beside 1.
3. **Family C**: Toy Soldier, Crystal Ball.
4. **Family D**: first the schema and `!!VP1.0`, proved on Tidepool's and Chameleon's text twins;
   then Bugs, Wolfman, Squid, Chameleon, Tidepool, Zoltar.
5. **Family E**: Dolphin.
6. **Family F**: Blobby Dancer, then Toys first of the rest (it has the twins), Vulcan, Time
   Machine, Gas Station, GeoForms, Clear Sailing, Timbury, Mad Mod Mike, Ogre.
7. **Families G and H**: decide again after 6.

Stop points: after 2 the gallery has 15 demos, after 4 it has 23.

## 7. Risks

- **Nothing to compare with.** No recordings, so correct means "matches the recovered code".
  Recordings of the originals would help most for D and F.
- **Behavior in code.** The NVIDIA demos' cameras and menus take more work than their data.
- **Shader dialects.** If the translator stalls, port only the programs a scene uses by hand.
- **Generated WGSL is derived from their shaders**: make it when packing, keep it out of Git.
- **Download size.** Six of the 35 exceed 60 MiB without loss. They need a choice: wait, or lossy textures.
- **WebGL2.** Float targets and multiple targets are fine for A; G needs compute.
- **Gallery layout.** One column per demo works for 5, not 15.

## 8. Decisions needed

1. **Publishing.** (a) The site has code only and the visitor drops in the original installer,
   read in the browser [recommended for this batch; the InstallShield reader is about 50 lines];
   (b) bundle as in the first batch; (c) local only.
2. **Scope.** Orders 0 to 2 first (10 demos), then look again [recommended], or all 35.
3. **Shared crates** (approach 2) [recommended], or keep copying.

## 9. Reproducing this

`tools/unpack`: `pftw.py` (InstallShield launchers), `iscab2.py` (cabinets 5 to 7, several
volumes, MD5), `is3.py` (InstallShield 3), `msi.py` (names for an unpacked MSI's cabinet),
`census.py` (APIs, formats, sizes; set `UNPACKED` to a folder with `x` and `y` in it).
`unar` opens the rest. All cabinets extracted at the stated sizes. MD5s matched except four
installer files per newer package (`setup.inx`, `setup.exe`, `setup.ini`), checked on three
of the packages that have any; no demo content is among those.
Not fully unpacked: Design Garage (256 of 307 files; LZX errors) and Stone Giant (one 797 MB stream).
