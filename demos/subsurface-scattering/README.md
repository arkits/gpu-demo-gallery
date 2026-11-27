# Subsurface Scattering

A Rust and wgpu port of ATI's **Subsurface Scattering v1.4** Radeon X800 technology demo
(2004, Direct3D 9). A marble statue of a winged figure stands in a domed temple at sunset,
lit with precomputed radiance transfer: light that scatters inside the marble, and the
temple's indoor and outdoor light stored per texel. The player reads the original Sushi
archive and runs its scenes, fly paths, shaders and Lua frame scripts.

[Open the WebGPU player](https://subsurface-scattering-port.pages.dev/)

The player downloads the demo's files with the page (packed losslessly). The files are ATI's and
are not in this repository. For a local build, put `Sushi.ati` and the `Audio` folder
(`SceneIntro.wav`, `SceneLoop.wav`) in `data/`. A build without them asks for the installed
demo's folder.

```sh
scripts/dev.sh run           # native player
scripts/dev.sh web           # WebGPU player at http://127.0.0.1:8080/
scripts/dev.sh check         # native and wasm builds, and tests
scripts/dev.sh shot 1 9      # render a frame to target/devtest/
```

The three chapters are the demo's modes from `Sushi2.ini`: **Normal** (with depth of field),
**Subsurface scattering** (with and without, side by side) and **Outdoor/indoor light**. Space
pauses, the arrow keys seek, and the number keys switch chapters. F takes the demo's own
interactive camera for the chapter (the `control` lines of `Sushi2.ini`) and C moves to the
next one; drag to turn it and scroll to move in and out. M toggles sound, L the chip logo,
and Enter fullscreen.

The engine is the one in the [The Assassin](../the-assassin) and [Crowd](../crowd) ports. See
[the engine notes](docs/ENGINE.md) and [the file formats](docs/formats/README.md).
