# Ogre

A native and web port of NVIDIA's 2003 GeForce FX "Ogre" demo (Spellcraft) using Rust and wgpu.
An ogre boss with subdivision-surface clothing dances while five creeps writhe around him, all
bump-mapped with soft shadows on a painted backdrop.

[Play Ogre](https://ogre-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures`, `shaders` (only read for reference) and `audio`
directories in `data/`, or set `OGRE_DATA`. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

| Input | Does |
|---|---|
| Left, middle, right drag | Orbit, pan, zoom. The wheel and pinching also zoom |
| Space | Pause |
| Home, L, M, Enter | Reset the view, NVIDIA logo, mute, full screen |

Left alone for two minutes, the demo returns to the dance.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
