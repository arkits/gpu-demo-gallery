# Dangerous Curves

A Rust and wgpu port of ATI's **Dangerous Curves v1.3** Radeon technology demo.
The player reads the original Sushi archive and runs its scene, camera, shader, and frame-script data.

[Open the WebGPU player](https://dangerous-curves-port.pages.dev/)

Choose the installed demo folder in the browser, or put `Sushi.ati` in `data/` for a local build.
Add `Audio/SceneIntro.mp3` and `Logo2.tga` from the same demo for its soundtrack and logo. The original
files are supplied separately and are excluded from Git.

```sh
scripts/dev.sh run           # native player
scripts/dev.sh web           # WebGPU player at http://127.0.0.1:8080/
scripts/dev.sh check         # native and wasm builds, and tests
scripts/dev.sh shot 1 9      # render a frame to target/devtest/
```

Space pauses, the arrow keys seek, and the number keys switch chapters. Drag, scroll, or press F
to use the free camera. M toggles sound, L toggles the chip logo, and Enter toggles fullscreen.

See [the engine notes](docs/ENGINE.md) and [the file formats](docs/formats/README.md).
