# Zoltar

A native and web port of NVIDIA’s 2001 graphics demo for the GeForce 3: an animated fortune teller, drawn
with the original meshes, vertex programs, combiner setups and voice recordings.

[Play Zoltar](https://zoltar-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `programs`, `textures`, `UserInterface`, and `audio` directories in
`data/`, or set `ZOLTAR_DATA`. The original content belongs to NVIDIA.

```sh
sh scripts/dev.sh run       # native player
sh scripts/dev.sh web       # local web build
sh scripts/dev.sh all       # native, WebGPU and WebGL2 checks
sh scripts/dev.sh package   # macOS app and Windows executable
```

Press the Gem o’ Truth or Space for the next fortune. Left alone, he greets you, and later grows bored.

| Input | |
| --- | --- |
| Drag, scroll or pinch | Turn, zoom. A drag that is let go keeps spinning. |
| 1–7, G, I | A certain fortune, the greeting, an idle clip |
| X | Stop him |
| N, L | Drags move the light; put the light back |
| Home | Reset the view |
| P, M, V | Pause, mute, NVIDIA logo |
| Enter | Full screen |

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
