# Small Pond

A port of NVIDIA's "Small Pond" demo for the GeForce2 (July 2000, OpenGL) to current systems:
browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A wooden pool in
a garden: ripples mirror photographs of a day around it and bend the view of the boards
below. It reads the demo's own pictures and sound.

Play it: https://pond-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put the contents of its `Pond
Small` folder in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Drag | Turn |
| Right button, or a click on the water | Ripple |
| Up, down, scroll | Zoom |
| p | Play the day |
| 1 to 9, 0 | Full speed, half, a third, to a tenth |
| + - | Strength of ripples |
| n | Normals from both sides |
| x | Ripple somewhere |
| Space, . | Stop, one step |
| w, h | Wireframe, help |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
