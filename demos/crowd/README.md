# Crowd

A Rust and wgpu port of ATI's **Crowd v1.4** Radeon X800 technology demo (2004, Direct3D 9).
An army of skinned, instanced soldiers charges out of canyon caves past trenches and barbed
wire. The player reads the original Sushi archive and runs its scenes, fly paths, shaders and
Lua frame scripts.

[Open the WebGPU player](https://crowd-port.pages.dev/)

The player downloads the demo's files with the page (packed losslessly). The files are ATI's and
are not in this repository. For a local build, put `Sushi.ati`, `Logo2.TGA` and the `Audio`
folder (`SceneIntro.wav`, `SceneLoop.wav`) in `data/`. A build without them asks for the
installed demo's folder.

```sh
scripts/dev.sh run           # native player
scripts/dev.sh web           # WebGPU player at http://127.0.0.1:8080/
scripts/dev.sh check         # native and wasm builds, and tests
scripts/dev.sh shot 1 9      # render a frame to target/devtest/
```

The three chapters are the demo's own modes from `Sushi2.ini`: **Normal**, **AO and
VColor/Atten** (ambient occlusion and floor attenuation terms) and **Random Vectors** (each
soldier's random ID as a color). Space pauses, the arrow keys seek, and the number keys switch
chapters. Drag, scroll, or press F to use the free camera. M toggles sound, L toggles the chip
logo, and Enter toggles fullscreen.

The engine is the one in the [The Assassin](../the-assassin) port. See
[the engine notes](docs/ENGINE.md) and [the file formats](docs/formats/README.md).
