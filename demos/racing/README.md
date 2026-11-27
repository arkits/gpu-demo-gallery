# Racing

Indy cars lapping a speedway at 70 units a second along the lanes `race.exe` keeps in its data, wheels turning and steering, with the demo's five camera views.

A port of 3dfx's "Racing" demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://racing-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| 1 | Rear view |
| 2 | Chase |
| 3 | Follow |
| 4 | Track cameras |
| 5 | Front |
| - / + | Slower / faster |
| f | Fog |
| w / t | Wireframe / textures |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
