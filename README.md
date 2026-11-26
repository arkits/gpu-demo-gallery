# Demoroom

An Egyptian room lit by a projector you steer. Its pictures — nine slides and a twenty-frame film — are multiplied over the walls in a second pass, as 3dfx showed off multi-pass projected textures.

A port of 3dfx's "Demoroom" demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://demoroom-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Arrows | Turn the projector |
| Shift + Up/Down | Move the projector along its beam |
| A / D, 4 / 6 | Turn the camera |
| 8 / 2 | Move the camera |
| Drag, scroll | Turn, move the camera |
| Space, n | Next slide (the tenth is the film) |
| l | Projected light on/off |
| f | Fog |
| c | Centre camera and light |
| w / t | Wireframe / textures |
| h, F1 | Help picture |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
