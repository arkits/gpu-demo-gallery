# Bugs

A native and web port of NVIDIA’s 2002 graphics demo. Sixty alien bugs swarm across a height-mapped junkyard.

[Play Bugs](https://bugs-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures`, `UserInterface`, and `sounds` directories in
`data/`, or set `BUGS_DATA`. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

The opening starts on an empty stage. One bug arrives first, followed by the swarm.

| Control | Action |
| --- | --- |
| Drag, arrow keys, scroll or pinch | Look around and walk |
| 1, 2, 3 | Normal, Night Vision, Depth Vision |
| S, W, P | Solid, lines, points |
| L, N | Steer the spotlight, return to walking |
| A | Toggle the aliased inset |
| +, − | Add or remove bugs |
| Space, Home | Pause, reset the view |
| Tab (native), Controls (web) | Open the controls |
| M, Enter | Mute, toggle full screen |

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
