# Surf

Standing in a B-spline sea. Four wave sources wander the control grid, each sending out a ripple that swells and dies over 5 to 15 seconds, as `surf.exe` moves its surface.

A port of 3dfx's "Surf" demo (1998) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://surf-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Drag | Look round |
| Left / Right | Move sideways |
| = / - | More / fewer subdivisions |
| d | Control points |
| n | Normals |
| g | Gap fill |
| w | Wireframe |
| e | Environment map |
| h | Fog |
| p | Pause waves |
| t | B-spline / Bezier |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
