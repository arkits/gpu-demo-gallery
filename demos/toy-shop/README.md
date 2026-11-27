# Toy Shop

A Rust and wgpu port of ATI's **Toy Shop v1.2** Radeon X1800 technology demo (2005, Direct3D 9).
A rainy night in a city street outside a toy shop: rain, splashes and ripples simulated on the
GPU, water streaking down the shop window, wet reflective streets and puddles, parallax
occlusion mapped walls, lightning, and a taxi with dynamic cube-map reflections. The player
reads the original Sushi archive and runs its scenes, fly paths, shaders and Lua frame scripts.

[Open the WebGPU player](https://toy-shop-port.pages.dev/)

The player downloads the demo's files with the page (packed losslessly). The files are ATI's and
are not in this repository. For a local build, put `Sushi.ati` and `Audio/SceneIntro.wav` in
`data/`. A build without them asks for the installed demo's folder.

```sh
scripts/dev.sh run           # native player
scripts/dev.sh web           # WebGPU player at http://127.0.0.1:8080/
scripts/dev.sh check         # native and wasm builds, and tests
scripts/dev.sh shot 1 9      # render a frame to target/devtest/
```

The nine chapters are the demo's own modes from `Sushi2.ini`: Normal, The City, Following the
Taxi, Geometry, POM vs Normal Mapping, POM vs Offset Limiting, LOD Visualization, Water
Simulation Breakdown and Droplets Breakdown. Space pauses, the arrow keys seek, and the number
keys switch chapters. F takes the demo's interactive cameras (the `control` lines of
`Sushi2.ini`) and C moves to the next; drag to turn and scroll to move in and out. M toggles
sound, L the chip logo, and Enter fullscreen.

The engine is the one in the [The Assassin](../the-assassin), [Crowd](../crowd) and
[Subsurface Scattering](../subsurface-scattering) ports. See [the engine notes](docs/ENGINE.md)
and [the file formats](docs/formats/README.md).
