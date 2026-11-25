# NPR Hatching

A port of ATI's Radeon 9700 "NPR Hatching" demo (2002, Direct3D 9) to current systems:
browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A still life is
drawn with tonal art maps and stencil shadows, then outlined where normals, depth or object
colors change. It reads the demo's own data, shaders and frame scripts.

Play it: https://npr-hatching-port.pages.dev

The demo's files are ATI's and are not in this repository. Put your `Sushi.ati` in
`data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1 to 5 | Chapters: normal, toggle edges, types of edges, shadow edges, edge width |
| Space, ← → | Pause, skip |
| Drag, scroll, F | Free camera (not in the original); F returns |
| L | Logo |
| Enter, Esc | Full screen, quit (native) |

The engine is the one in the [Pipe Dream](https://github.com/arkits/pipedream) and
[Car Paint](https://github.com/arkits/car-paint) ports, whose `SushiDX.exe` is byte for byte
the same; their `docs` describe the parts not listed in [docs/ENGINE.md](docs/ENGINE.md).
