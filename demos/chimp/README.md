# Chimp

A port of ATI's Radeon 9800 "Chimp" demo (2003, Direct3D 9) to current systems: browsers
(WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A chimpanzee by a jungle
stream, with fur of shells and fins, a face that changes expression, butterflies and water
that reflects and refracts. It reads the demo's own data, shaders and frame script.

Play it: https://chimp-port.pages.dev

The demo's files are ATI's and are not in this repository. Put your `Sushi.ati` and `Audio`
folder in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Space, ← → | Pause, skip |
| Drag, scroll, F | Free camera around the chimp (the first `control` of `Sushi2.ini`); F returns |
| L, M | Logo, mute |
| Enter, Esc | Full screen, quit (native) |

The engine is the one in the [Car Paint](../car-paint), [Pipe Dream](../pipedream) and
[Bear](../bear) ports, whose `SushiDX.exe` is byte for byte the same as Chimp's; their docs
describe the parts not listed in [docs/ENGINE.md](docs/ENGINE.md).
