# Crystal Ball

A native and web port of NVIDIA’s June 2000 graphics demo using Rust and wgpu.
A refracting globe on a brass stand, two explorable castles, changing cube maps, falling flakes, and a soundtrack for each environment.

[Play Crystal Ball](https://crystal-ball-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

The assets come from the extracted NVIDIA installer. Keep `models`, `textures_ball`, `textures_light`, `textures_dark`, `UserInterface`, and `sounds` in the extracted folder.
Put that folder at `data/` or set `CRYSTAL_BALL_DATA`.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

Space enters or leaves the ball. S swaps castles. R shakes the ball. Drag to orbit and scroll or pinch to zoom. P pauses, Home resets the view,
L toggles the NVIDIA logo, M mutes, and Enter toggles full screen.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md). The original assets belong to NVIDIA.
