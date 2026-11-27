# Bear

A port of ATI's Radeon 9700 "Bear" demo (2002, Direct3D 9) to current systems: browsers
(WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A bear's head with fur of
shells and fins, lit along each hair. It reads the demo's own data, shaders and frame scripts.

Play it: https://bear-port.pages.dev

The demo's files are ATI's and are not in this repository. Put your `Sushi.ati` and `Audio` folder in
`data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1 to 4 | Chapters: normal, fur construction, lighting decomposition, fur variations |
| Space, ← → | Pause, skip |
| Drag, scroll, F | Free camera (not in the original); F returns |
| L, M | Logo, mute |
| Enter, Esc | Full screen, quit (native) |

The engine is the one in the [Pipe Dream](https://github.com/arkits/pipedream) and
[Car Paint](https://github.com/arkits/car-paint) ports, whose `SushiDX.exe` is byte for byte
the same; their `docs` describe the parts not listed in [docs/ENGINE.md](docs/ENGINE.md).
