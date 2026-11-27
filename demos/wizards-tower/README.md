# Wizards Tower

A walk through the five rooms of a wizard's tower, each triangle lit by its own light map in a second, multiplied pass, as `wiz.exe` showed off light-mapped environments.

A port of 3dfx's "Wizards Tower" demo (1996) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://wizards-tower-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Up / Down | Walk |
| Left / Right | Turn |
| Drag | Look |
| 1-5 | Go to room 1-5 |
| w / t | Wireframe / textures |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
