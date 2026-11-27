# Donut

A port of 3dfx's "Donut" demo for the Voodoo2 (1998, Glide 2) to current systems: browsers
(WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A torus with embossed
bumps under a reflection. It reads the demo's own pictures.

Play it: https://donut-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| p | Plain, bumps, reflection, both |
| t | Next bumps |
| d D, c C, e E | More and less depth, reflection, brightness of bumps |
| b | How bumps and reflection mix |
| + -, PageUp PageDown | Triangles, size of the hole |
| g, w, n | Background, wireframe, normals |
| o, r | Drag turns the light or the torus; reset |
| Drag, scroll | Turn, scale |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
