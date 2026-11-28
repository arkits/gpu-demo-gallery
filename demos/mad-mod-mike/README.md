# Mad Mod Mike

A native and web port of NVIDIA's “Mad Mod Mike” demo for the GeForce 7 series (2005,
OpenGL with Cg) using Rust and wgpu. A character with morphed expressions and
grown hair acts out ten takes in a bedroom and a workshop, with rocket flames,
sparks, mermaid projections and menu cards.

Play it: https://mad-mod-mike-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put the contents
of its folder (`models`, `textures`, `shaders`, `bin`, `ui`, `audio`) in `data/`,
or set `MADMODMIKE_DATA`.

```sh
scripts/dev.sh run      # native player
scripts/dev.sh web      # local web build
scripts/dev.sh all      # native, WebGPU and WebGL2 checks
scripts/dev.sh package  # macOS app and Windows executable
```

| Input | Does |
|---|---|
| 1–9, 0 | Takes 1–10 |
| c | Scripted, free and helmet cameras |
| Up, down | Facial expression combos |
| Left, right | Previous and next take |
| Drag, scroll | Orbit, zoom. Middle drag pans, right drag zooms |
| m | Menu overlay |
| Space | Pause |
| M | Mute |
| Enter, Home | Full screen, reset view |
| Esc | Quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md). How to develop and
check the port: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
