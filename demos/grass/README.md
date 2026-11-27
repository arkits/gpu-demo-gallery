# Grass

A port of NVIDIA's "Grass" demo for the GeForce 256 (August 2000, by Simon Green, OpenGL) to
current systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu.
A meadow without an end, made of noise, in the wind. It reads the demo's own pictures and
sound, and a model and tables inside its program.

Play it: https://grass-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put `grass.exe` and its `tex`
and `audio` folders in `data/`. The program is only read, never run.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Left, right button | Fly, fly back |
| Middle button | Look around; with the left one, move the sun |
| Arrows, scroll | Turn, faster and slower |
| Space, x | Storm, next sky |
| + -, [ ], ; ', < > | Wind, width, width of the tips, length |
| a, g, k, l, o, s | Sharper ground, grass, sky, light, shadows of clouds, flare |
| v, r, w | Fast, reset, wireframe |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
