# Surf

A port of 3dfx's "Surf" demo (1998) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu. It draws the demo's moving B-spline control surface with the original texture and environment map.

Play it: https://surf-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| d | Show control points |
| n | Show normals |
| g | Gap fill mode |
| w | Wireframe |
| e | Environment map |
| h | Fog |
| x | Dynamic LOD |
| p | Pause waves |
| t | B-spline / Bezier |
| = / - | More / less subdivision |
| Drag | Turn camera |
| Scroll | Move in / out |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
