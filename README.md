# ATB-TV

A port of 3dfx's "ATB-TV" ATB demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu. A TV cabinet playing a movie, built from the demo's own .3DF frames.

Play it: https://atb-tv-port.pages.dev

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
