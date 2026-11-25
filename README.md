# Borg

A port of 3dfx's "Borg" demo for the Voodoo2 (1998, Glide 2) to current systems: browsers
(WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A cube with nine textures
in five passes. It reads the demo's own pictures.

Play it: https://borg-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put them in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1 to 8 | Ship's four textures, two layers of lights, spotlight, windows that reflect |
| 0 | None of them |
| Space | Freeze |
| a A, b B, e E | Ambient light, filtering, background |
| x z, X Z | Nearer, farther; keep moving |
| Drag, scroll | Turn, zoom (not in the original) |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
