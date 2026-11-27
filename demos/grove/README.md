# Grove

A port of NVIDIA's "Grove" demo for the GeForce 256 (April 2000, OpenGL) to current systems:
browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. Trees that grow
from seven numbers, lit by fireflies. It reads the pictures inside the demo's own program.

Play it: https://grove-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put your `grove000419.exe` in
`data/`. It is only read, never run.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Left, right button | Fly toward the pointer, back |
| Arrows | Sideways, up and down |
| Sliders, u | Shape of the first tree; show or hide |
| c, g, + - | New tree, fifteen more trees, one more or fewer |
| l, 1 to 8 | Leaves or bills, fireflies for each tree |
| f, w | Free view (does not turn to the middle), wireframe |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
