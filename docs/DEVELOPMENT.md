# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing
opens a window except `run`.

The layout is a Cargo workspace (as in `bear`, `double-cross`):

- `crates/atb` — the shared ATB engine/readers this pioneer owns: strict,
  bounds-checked `BOF` / `3DF` / `3DS` readers with unit tests (synthetic
  fixtures) and integration tests against the staged files. Other ATB demos copy
  this crate.
- `crates/demoroom` — the demo: `demo.rs` (loads the room through `atb`),
  `gl.rs` (the shared immediate-mode wgpu renderer), `image.rs`, `data.rs`
  (loose folder or `.xz` pack), `player.rs` / `native.rs` / `web.rs` shells,
  `shaders/`, `examples/shot.rs`, `examples/pack.rs`, `tests/`.

`crates/demoroom` is `cdylib` + `rlib`, so the same library builds the native
binary (`main.rs` → winit) and the wasm browser bundle (`build-web.sh` →
`wasm-bindgen`). The crate is `demoroom`; cargo emits `demoroom.wasm` and the JS
is kept hyphenated via `--out-name demoroom`.

Readers refuse what they do not understand and never read past the end:
`DEMOPAK1` packs are bounds-checked, `.3DF` payloads must match the LOD
pyramid size, `.3DS` chunks must fit their parent, and `.BOF` names are
validated ASCII.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages
with the demo files packed into `dist/` (git-ignored). It is not connected to
Git and is only run explicitly, once the port is approved for deployment.

## Status

Native player, headless shots, WebGPU/WebGL2 browser path, packing, tests and
lints all work. Still future work: the original's projected-light cycling,
chromakey multipass and per-object materials; BOF object-graph extraction. The
current scene uses one `.3DF` texture and baked per-face lighting.
