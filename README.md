# Valley of Ra

Anubis and Nefertiti fight before a pyramid. Each fighter's moves are frame ranges of one long animation, played as `fight.exe` plays them, with its springy camera and the keys from its readme.

A port of 3dfx's "Valley of Ra" demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://valley-of-ra-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Up | Anubis jumps |
| Left / Right | Anubis walks |
| 7 8 9 6 | Anubis blocks, punches, high kick, low kick |
| w | Nefertiti jumps |
| a / s | Nefertiti walks |
| u i o l | Nefertiti blocks, punches, high kick, low kick |
| f | Fog |
| x | Wireframe |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
