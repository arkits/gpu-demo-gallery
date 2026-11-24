# Pipe Dream port: analysis and plan

Status: feature complete; see section 6 for what is not verified. Approach B
(Rust + wgpu) chosen 2026-11-24.
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
| `FlyPaths/Animusic.pth` | Text camera path: position, FOV (always -1 = default), flag, quaternion; step 0.033333 s; 6,448 frames = 214.9 s. [V] The flag (22 frames) marks the last sample of a shot. [V] |
| `Objects/animusic.ini` | 344 animated instances (67 Global, 277 SGlobal) of 101 meshes. SGlobal casts shadows. [V] |
| `Sushi2.ini` | near 1, far 5000, fov 35, startTime 10, four interactive orbit cameras. [V] |

Timeline: scene time starts at 10 s together with the music. [V] The camera path ends
0.9 s after the music.

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

- **M0 Setup. DONE.** Git repo with the demo ignored; Cargo workspace.
- **M1 Formats. DONE.** Every data file parses to exactly end of file; tests also check
  index ranges, quaternions, key timing, texture references and camera cuts.
- **M2 Static scene. DONE.**
- **M3 Shaders. DONE.** All 35 files parse; every drawable pass validates as WGSL.
- **M4 Animation, M5 Lighting, M6 Shadows. DONE.** A 2,150-frame sweep of the timeline
  needs no fallback shader variant and reports no missing pipeline.
- **M7 Show. DONE.** Music, fades, logos, free cameras, seeking. A full-length run in
  headless Chrome (WebGPU): 13,203 frames at 60 fps, none late, at most 12 ms from the
  music, and it looped. WebGL2 differs only along anti-aliased edges (mean 0.03 to 0.28
  of 255 at three times). Before 2026-11-24 the WebGL2 tests really ran WebGPU, because
  Chrome's flags no longer turn it off; the driver now checks which API the canvas uses.
- **M8 Native. BUILT.** macOS app runs. `PipeDream.exe` is cross-built but has **not been
  run on Windows**.

Not verified: Safari, Firefox, running on Windows, an Intel Mac, and any comparison with
the original (no recording yet). The native window's renderer is checked through offscreen
frames; the window itself has not been inspected on screen.

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
3. Remaining differences from the original are listed at the end of `ENGINE.md`.

## 9. Tooling status

- REA 6.10.1 is registered for Claude Code and connected to Ghidra 12.2 and JDK 25.
  Its tools appear in sessions started after the registration.
- The registration still runs on nvm Node 24.22.3 from the npx cache
  (`~/.npm/_npx/af752b90495482d7`); removing either breaks it.
