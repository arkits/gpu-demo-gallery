# Tidepool

A native and web port of NVIDIA’s 2002 graphics demo. A rocky pool with animated ripples and reflections.

[Play Tidepool](https://tidepool-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures`, `UserInterface`, and `audio` directories in
`data/`, or set `TIDEPOOL_DATA`. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

Z toggles the water motion and plays a drop. Drag to orbit and scroll or pinch to zoom. P pauses, Home resets the view,
L toggles the NVIDIA logo, M mutes, and Enter toggles full screen.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
