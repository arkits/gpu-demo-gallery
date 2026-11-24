# Development

`scripts/dev.sh` without arguments lists the commands. Only `run` and `open` show a
window; tests use offscreen frames or headless Chrome.

- `check`: format, lints for native and wasm, unit tests, and tests that read the demo's
  files.
- `smoke`: the native player in a hidden window, without sound.
- `webtest`: show, stages, models, music, drag, zoom, resize, phone layout. The WebGL2
  run hides `navigator.gpu`, because Chrome's flags do not turn WebGPU off, and checks
  which API the canvas uses.
- `compare`: all 18 stages natively, in WebGPU and in WebGL2. On 2026-11-24 WebGPU was
  identical to native, and WebGL2 differed by at most 0.08 of 255 on average, along
  reflections and edges.

`?model=&stage=&yaw=&pitch=` shows one still in the browser.

The site: `scripts/deploy-cloudflare.sh` packs the needed files with xz and the sounds
as FLAC (both lossless), puts them in the git-ignored `dist/cloudflare`, and uploads
that. Sounds load after Start. No content enters Git.

`Cargo.lock` is committed; wasm-bindgen's tool is installed under `target/`.
