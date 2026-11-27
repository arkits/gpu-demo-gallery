# Bubble

A port of NVIDIA's "Bubble" demo for the GeForce 256 (1999, OpenGL) to current systems:
browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A bubble of
springs that mirrors its surroundings, through a cube map or a sphere map. It reads the
pictures inside the demo's own program.

Play it: https://bubble-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put your `bubble.exe` in
`data/`. It is only read, never run.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Pointer | The view turns faster the farther it is from the middle |
| Click, Space | Poke |
| c, s, w | Cube map, sphere map, wireframe |
| t | The pictures themselves |
| 2 to 4 | Triangles |
| Scroll | Distance (not in the original) |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
