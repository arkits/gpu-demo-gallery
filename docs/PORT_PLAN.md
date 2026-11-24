# Pipe Dream port: analysis and plan

Status: in progress. Approach B (Rust + wgpu) chosen 2026-11-24. M0, M1 and M2 done.
Labels: [V] verified against the files, [I] inferred, [?] still unknown.

## 1. Goal

Run ATI's "Pipe Dream" Radeon 9700 demo (Sushi engine, D3D9, 2002-03) on modern
systems: browser first, then macOS and Windows, from the same code.

Success: the full piece plays start to finish, in sync with the music, with the
original camera, lighting, stencil shadows, reflections and ball motion blur.

Non-goals: a general Sushi engine or D3D9 emulator; remastered art.

## 2. What is in the package

| Item | Finding |
|---|---|
| `SushiDX.exe` | PE32 x86, linked 2003-03-13, imports d3d9 and Miles (`mss32`). Not packed: Ghidra finds 3,505 functions. Much of it is statically linked D3DX/JPEG code. [V] |
| `Sushi.ati` | Ordinary ZIP, no encryption. 40 MB, 126 MB unpacked. [V] |
| `AnimusicIntro.mp3` | 204.07 s, 44.1 kHz stereo, 192 kbps. [V] |
| `Shaders/*.ssh` | Plain-text source: 62 vs.1.1, 61 ps.2.0, 2 ps.1.4 programs, plus render states. About 40 distinct asm opcodes. No flow control seen in the opcode survey. [V] |
| `Scripts/Normal.sds` | The whole frame: 1 clear and 6 draw calls (section 4). [V] |
| `FlyPaths/Animusic.pth` | Text camera path: position, FOV (always -1 = default), flag, quaternion; step 0.033333 s; 6,448 frames = 214.9 s. [V] The flag (22 frames) is a camera cut. [I] |
| `Objects/animusic.ini` | 344 animated instances (67 Global, 277 SGlobal) of 101 meshes. Global vs SGlobal is [?]. |
| `Sushi2.ini` | near 1, far 5000, fov 35, startTime 10, four interactive orbit cameras. [V] |

Timeline: 214.9 s of camera minus the 10 s startTime is about the MP3 length, so
music likely starts at t=10. [I] `audioIntroOverlap = 1.0` may shift that. [?]

## 3. Binary formats

Recovered; see `docs/formats/README.md`. All 101 `.so2`, 344 `.sa2` and both scene
files parse to exactly end of file. [V] The meaning of some record fields is still
being recovered, so records keep their raw bytes.

## 4. Frame structure (from `Normal.sds`)

1. Clear Z, stencil = 128
2. Objects: opaque, shadow receivers
3. Scene: opaque + transparent
4. Shadow volumes into stencil (extruded in the vertex shader)
5. Fullscreen quad where stencil != 128, blend `InvDstAlpha, DstAlpha`
   (framebuffer alpha holds the shadow dim factor)
6. Objects: transparent receivers, then non-receivers

Lighting ("FastRT"): a shader variant is picked by the number of lights, up to 4
for scene shaders and 3 for most object shaders, each with a projected falloff texture.

## 5. Approaches

| | A. TypeScript + WebGL2 | B. Rust + wgpu (chosen) | C. Emulate the exe |
|---|---|---|---|
| Browser | Best fit, small download | WASM, larger, slower to iterate | x86 emulator + D3D9 layer |
| macOS/Windows | Tauri shell around the same code | True native binaries | Wine/DXVK-style per platform |
| Fidelity | High | High | Highest if it works |
| Risk | Low | Medium | High; poor fit for browser |

**Decision: B.** The desktop apps must be truly native, with no webview. One Rust
codebase builds to WASM for the browser and to native macOS (Metal) and Windows
(DirectX 12/Vulkan) binaries.

Calls made (correct any of these):
- Crates: `sushi-formats` (files, done), `sushi-shader` (.ssh to WGSL), `sushi-engine`
  (simulation and wgpu rendering), `pipedream` (window, audio, web and native shells).
- In the browser, WebGPU first, with wgpu's WebGL2 backend as fallback.
- Custom renderer: the engine's state and asm shaders map directly to wgpu pipelines.
- The app reads the user's own `Sushi.ati` and MP3. Assets are never committed.
- Readers are strict and fail on unread bytes.

## 6. Milestones

Each ends in something checkable.

- **M0 Setup. DONE.** Git repo with the demo ignored; Cargo workspace.
- **M1 Formats. DONE.** *Every data file parses to exactly end of file; tests also
  check index ranges, quaternions, key timing and texture references.*
- **M2 Static scene. DONE.** Scene, textures and baked lighting along the original
  camera path. *Checked in headless Chrome (WebGPU and WebGL2) and offscreen Metal;
  the native window runs but was not inspected visually.* A free camera moved to M7.
- **M3 Shaders.** `.ssh` parser (including `.ssi` `#replicate`) and translator from
  vs.1.1/ps.1.4/ps.2.0 asm to WGSL. *Check: every program validates; golden tests
  per opcode.*
- **M4 Animation.** Fly path, bone tracks, 40-matrix skinning, visibility, and the
  engine-driven values `SOLVelocity`, `SOLVibrate`, `SOLGlow`, `SOLOrientationX`.
  *Check: balls land on instruments on the beat.*
- **M5 Lighting.** FastRT light selection, spotlight textures, object ambient,
  reflections, ball motion blur.
- **M6 Shadows.** Stencil volumes and the dest-alpha quad, MSAA, resolve to screen.
- **M7 Show.** Audio clock as master time, fades, logo/credits, free-camera modes,
  keys, resize, fullscreen, load screen. *Check: full run in Chrome, Safari, Firefox.*
- **M8 Native release.** macOS app bundle and Windows exe. *Check: full run on both.*

## 7. Known traps

- D3D9 differences: half-pixel offset, winding, BGRA vertex colors, cube face order.
- vs.1.1 constants run to c189; use uniform buffers.
- BC/DXT textures are optional in browsers; include a CPU decoder fallback.
- Browsers block audio until a click: need a start screen.
- The output must be opaque; scene alpha carries shadow data.
- WebGPU, WebGL2, Metal and DX12 differ in features and limits; test in M2 onward.
- WebGPU clip space matches D3D (z 0..1); texture origin and winding still need care.
- WGSL has no unaligned vertex data: all strides seen are multiples of 4, so fine.

## 8. Open questions

1. No reference capture exists here. The user is looking for a
   recording; work continues without one, so visual fidelity is judged from the
   recovered code until then.
2. The assets are ATI's and Animusic's (credits: "Copyright 2001 Animusic"). Private
   use from your copy is the working assumption. Public hosting of the assets or
   music would need permission.
3. Semantics still to recover from the exe: SGlobal, path flag, `smoothTime`,
   light-selection order, key interpolation.

## 9. Tooling status

- REA 6.10.1 is registered for Claude Code and connected to Ghidra 12.2 and JDK 25.
  Its tools appear in sessions started after the registration.
- The registration still runs on nvm Node 24.22.3 from the npx cache
  (`~/.npm/_npx/af752b90495482d7`); removing either breaks it.
