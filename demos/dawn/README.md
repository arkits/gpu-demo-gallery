# Dawn

A native and web port of NVIDIA’s December 2002 GeForce FX launch demo using Rust and wgpu.
A fairy on a branch, with skinned and morphed vertices, skin lit by cube maps, translucent wings, and hair drawn as lines.

[Play Dawn](https://dawn-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures` and `audio` directories in
`data/`, or set `DAWN_DATA`. The port draws the demo’s “ultra” version. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

| Input | Does |
|---|---|
| D, S, F | The dance with its own camera, sunning, free camera |
| 1–8, + and − | Expressions |
| Left, middle, right drag | Orbit, pan, zoom. The wheel and pinching also zoom |
| Space | Pause |
| Home, L, M, Enter | Reset the view, NVIDIA logo, mute, full screen |

The page also has the sliders of the original for each expression, oily skin and goose bumps.
Left alone for two minutes, the demo returns to the dance.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
