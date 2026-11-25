# Development

`scripts/dev.sh` has every task; run it without arguments for the list.

This is a Cargo workspace: `crates/atb` is the shared ATB engine/readers
(BOF/3DF/3DS), `crates/<demo>` is the player. The `cdylib`+`rlib` lib builds
both the native binary (winit) and the wasm browser bundle (wasm-bindgen).

Readers refuse unknown formats and never read past the end. Demo content is
never committed: `data/`, `original/`, `target/`, `dist/` are git-ignored, and
`deploy-cloudflare.sh` bundles a packed copy only for the deployment.
