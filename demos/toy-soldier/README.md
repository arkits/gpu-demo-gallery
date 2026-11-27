# Toy Soldier

A native and web port of NVIDIA’s June 2000 graphics demo using Rust and wgpu.
A marching tabletop formation, a wind-up robot, five levels of detail, projected shadows, and cube-map reflections.

[Play Toy Soldier](https://toy-soldier-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

The assets come from the extracted NVIDIA installer. Keep `models`, `textures`, `UserInterface`, and `audio` in the extracted folder.
Put that folder at `data/` or set `TOY_SOLDIER_DATA`.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

Space starts the panic sequence. S returns to marching. Drag to orbit and scroll or pinch to zoom. P pauses, Home resets the view,
L toggles the NVIDIA logo, M mutes, and Enter toggles full screen.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md). The original assets belong to NVIDIA.
