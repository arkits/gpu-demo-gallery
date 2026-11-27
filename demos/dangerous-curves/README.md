# Dangerous Curves

A Rust and wgpu port of ATI's **Dangerous Curves v1.3** Radeon technology demo.
The player reads the original Sushi archive and runs its scene, camera, shader, and frame-script data.

[Open the WebGPU player](https://dangerous-curves-port.pages.dev/)

The player downloads the demo's files with the page (packed losslessly). The files are ATI's and
are not in this repository. For a local build, put `Sushi.ati`, `Logo2.tga` and
`Audio/SceneIntro.mp3` in `data/`. A build without them asks for the installed demo's folder.

```sh
scripts/dev.sh run           # native player
scripts/dev.sh web           # WebGPU player at http://127.0.0.1:8080/
scripts/dev.sh check         # native and wasm builds, and tests
scripts/dev.sh shot 1 9      # render a frame to target/devtest/
```

Space pauses, the arrow keys seek, and the number keys switch chapters. Drag, scroll, or press F
to use the free camera. M toggles sound, L toggles the chip logo, and Enter toggles fullscreen.

See [the engine notes](docs/ENGINE.md) and [the file formats](docs/formats/README.md).
