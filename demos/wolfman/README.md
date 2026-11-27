# Wolfman

A native and web port of NVIDIA’s 2002 graphics demo. A werewolf with eight shells of fur walks, runs and howls down a night street.

[Play Wolfman](https://wolfman-port.pages.dev/) · [Original download](https://www.nvidia.com/en-us/geforce/community/demos/)

Put the extracted original `models`, `textures`, `UserInterface`, and `audio` directories in
`data/`, or set `WOLFMAN_DATA`. The original content belongs to NVIDIA.

```sh
scripts/dev.sh run       # native player
scripts/dev.sh web       # local web build
scripts/dev.sh all       # native, WebGPU and WebGL2 checks
scripts/dev.sh package   # macOS app and Windows executable
```

He goes through the original's routine until a key picks an action: 1 breathe, 2 scratch,
3 howl, 4 walk, 5 sidestep, 6 run. Actions follow what he is doing, with the original's
transitions between them. + and − change the number of fur shells, [ and ] the fur's length,
and O slow motion. Drag to orbit and scroll or pinch to zoom. Space or P pauses, Home resets
the view, L toggles the NVIDIA logo, M mutes, and Enter toggles full screen.

See [engine notes](docs/ENGINE.md) and [development](docs/DEVELOPMENT.md).
