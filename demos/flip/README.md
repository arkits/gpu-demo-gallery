# Flip

ATB Flip: one model at a time — Anubis, an AT-AT, a dragon, Ludwig, a skeleton, a chrome martini glass and Volkswagen, and `hpflip`'s helicopter — turned with the arrow keys, as `atsflip.exe` loads them from 3D Studio and VQ files.

A port of 3dfx's "Flip" demo (1996-97) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://flip-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| 1-8 | Anubis, AT-AT, dragon, Ludwig, skeleton, martini, VW, Hind |
| Arrows, drag | Turn the model |
| Scroll | Zoom |
| A / D | Turn faster / slower |
| f | Fog |
| c | Centre |
| w / t | Wireframe / textures |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
