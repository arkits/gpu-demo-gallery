# Valley of Ra

A port of 3dfx's "Valley of Ra" ATB demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu. The ATB fighting scene, reconstructed from the fight.bof name table.

Play it: https://valley-of-ra-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| w | Wireframe |
| p | Pause spin |
| t | Next texture |
| Drag | Turn camera |
| Scroll | Move in / out |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
