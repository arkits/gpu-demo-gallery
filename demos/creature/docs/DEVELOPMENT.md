# Development

`scripts/dev.sh` without arguments lists the commands. Only `run` and `open` show a
window; tests use offscreen frames or headless Chrome.

- `check`: format, lints for native and wasm, unit tests, and tests that read the demo's
  files.
- `smoke`: the native player in a hidden window, without sound.
- `webtest`: frame rate, laser, fish, music, drag, zoom, resize, phone layout. The WebGL2
  run hides `navigator.gpu`, because Chrome's flags do not turn WebGPU off, and checks
  which API the canvas uses.
- `compare`: four moments natively, in WebGPU and in WebGL2. On 2026-11-24 the mean
  difference was at most 0.04 of 255 in WebGPU and 0.06 in WebGL2, mostly edges. The
  largest is the latest moment: tiny differences in math make fish drift apart.

`?time=&yaw=&pitch=&laser=1` shows one still in the browser.

The site: `scripts/deploy-cloudflare.sh` packs the needed files with xz and the sounds
as FLAC (both lossless), puts them in the git-ignored `dist/cloudflare`, and uploads
that. Sounds load after Start. No content enters Git.

`Cargo.lock` is committed; wasm-bindgen's tool is installed under `target/`.
