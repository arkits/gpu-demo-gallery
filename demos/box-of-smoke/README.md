# Box of Smoke

A port of NVIDIA's "Box of Smoke" demo (2006, OpenGL and Cg on a GeForce 8) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu.

Smoke and water are simulated on the GPU on a 128×64×64 grid, as in the original.

The demo's models and images are NVIDIA's and are not in this repository. Unpack your copy
of the installer (for example with `unar`, without running it) and put its `models` and
`textures` folders in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Drag | Move the smoke (right button: in depth) |
| Ctrl or Shift drag, two fingers | Turn the box (right button: zoom) |
| Scroll, pinch | Zoom |
| S, W, N | Smoke, water, logo |
| B | Sharper or softer motion |
| ( ) and [ ] and { } | Color, size, fade |
| A | Render at twice the resolution |
| Space, ` | Pause, single step |
| Enter, Esc | Full screen, quit (native) |

After 10 seconds without input the demo moves by itself.

More: [how it works](docs/ENGINE.md), [development](docs/DEVELOPMENT.md). Uncertain points
are listed at the end of the first.
