# Car Paint

A port of ATI's Radeon 9700 "Car Paint" demo (2002, Direct3D 9) to current systems: browsers
(WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. It reads the demo's own
data, shaders and frame scripts.

The demo's files are ATI's and are not in this repository. Put your `Sushi.ati` and `Audio`
folder in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1 to 5 | Chapters: the car, normal maps, 8 and 16 bits, paint build up, colors |
| Space, ← → | Pause, skip |
| Drag, scroll, F | Free camera (not in the original); F returns |
| L, M | Logo, mute |
| Enter, Esc | Full screen, quit (native) |

The engine is the one in the [Pipe Dream port](https://github.com/arkits/pipedream), whose
`SushiDX.exe` is byte for byte the same; its `docs` describe the parts not listed in
[docs/ENGINE.md](docs/ENGINE.md).
