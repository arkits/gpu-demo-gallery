# Radeon 8500 and GeoForms

One Rust player for ATI Dolphin, Nature, Rachel, Island 1–3 and Treasure Chest, and NVIDIA GeoForms. The same WGSL renderer runs in a native window, WebGPU and WebGL2.

[Browse the demos in the GPU Demo Gallery](https://gpu-demo-gallery.pages.dev/)

## Local build

Extract the seven Radeon 8500 v1.1 archives into a common `ATI Demos` directory and unpack `GeoForms_Setup.exe` with an NSIS-aware extractor such as `unar`. Each ATI folder should have its original case and layout. The source packages are available from the [ATI demo archive](https://soggi.org/video-cards/tech-demos.htm) and [NVIDIA’s demo catalog](https://www.nvidia.com/en-us/geforce/community/demos/).

Run these commands from this demo folder.

```sh
python3 scripts/pack.py '/path/to/ATI Demos' /path/to/extracted/GeoForms
cargo run --release -- dolphin
cargo run --release -- geoforms --audio /path/to/music.mp3
```

The other native names are `nature`, `rachel`, `island1`, `island2`, `island3` and `treasure-chest`. With no `--audio` option GeoForms uses a built-in pulse. Its web page accepts an audio file from the device.

Drag to look, scroll to zoom, press Space to pause, use the arrow keys to seek, and press R to restart or F for fullscreen. Dolphin’s T key toggles curved triangles. In Treasure Chest, keys 0–7 select the chest or a showpiece. In GeoForms, M cycles deformations and S cycles stencil shapes.

```sh
rustup target add wasm32-unknown-unknown
scripts/build-web.sh
python3 -m http.server 8080 --directory dist
scripts/deploy-cloudflare.sh
```

Cloudflare Pages publishes the web build and the original content packs. Larger archives are served as chunks under the Pages per-file limit. Local assets and generated files are Git-ignored.

`web/index.html` is the shared player template. The build fills in the title, hardware and content identifier for each route from `scripts/stage-content.py`. All routes share the layout in `web/player.css` and the controls in `web/player.js`.

Generate a native frame with `cargo run --release -- nature --shot 39 frame.png`. Use `--inspect` to check that the packaged data loads and list its geometry and texture counts.

## Implementation and fidelity

Static analysis used `npx -y rea-agents@6.10.1 decompile-all` with Ghidra 12.2 on `SushiDX8.exe`, `TreasureChest.exe`, and `geoforms.scr`. [The engine notes](ENGINE.md) describe the recovered formats and differences from the original renderers.
