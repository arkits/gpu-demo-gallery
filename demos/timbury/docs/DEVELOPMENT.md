# Development

`scripts/dev.sh` lists the commands. Run `all` with the original data to lint native and
WASM builds, verify asset parsing, render in a hidden native window and run Chrome
tests for WebGPU and WebGL2. The comparison checks four frozen frames from each browser
backend against native frames.

`?time=15&yaw=0.4&pitch=-0.1&key=2` shows a frozen moment. Use the same values with
`scripts/dev.sh shot` to save a native frame. The shot draws the moment 60 times so the
measured exposure settles (`TIMBURY_FRAMES=1` shows the first frame only);
`TIMBURY_ZOOM=n` moves the free camera in by e^n for close-ups, e.g.
`TIMBURY_ZOOM=1.6 scripts/dev.sh shot 25 3.14 0 f face.png` for the lenses.
`cargo run --release --example dump -- DATA models/test.nmb renderPasses` prints any
part of a scene file as a tree.

`scripts/deploy-cloudflare.sh --prepare` stages the web build and lossless assets in
`dist/cloudflare`. Assets larger than 24 MiB are split into numbered parts, and audio
is compressed to FLAC. Git ignores the original files and generated output.

The demo data is git-ignored and linked in: `data/{models,textures,ui,audio}` are
symlinks to the unpacked original. Tests need `TIMBURY_DATA=$PWD/data` (which
`dev.sh` sets); `scripts/pack.sh` follows the symlinks with `find -L`.
