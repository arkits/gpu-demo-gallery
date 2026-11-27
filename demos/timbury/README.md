# Timbury

A native and web port of NVIDIA's 2003 GeForce FX “BugLooker” demo using Rust and wgpu.
A dancing fairy, a 40-bug swarm, meadow grass and hair: skinned and morphed meshes,
HDR bloom, a clickable in-scene menu and two movie cameras.

[Play Timbury](https://timbury-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures`, `ui` and `audio` directories in
`data/`, or set `TIMBURY_DATA`. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

| Input | Does |
|---|---|
| 1, 2, F | Movie cameras 1 and 2, free camera |
| Left drag | Orbit (click the in-scene menu too) |
| Middle, right drag | Pan, zoom. The wheel and pinching also zoom |
| Space, P | Pause |
| Home, L, M, Enter | Reset the view, logo, mute, full screen |

The in-scene menu also holds five blur/exposure sliders. Left alone for two minutes,
the demo returns to the movie.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
