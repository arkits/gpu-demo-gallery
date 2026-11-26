# Development

`scripts/dev.sh` has every task; run it without arguments for the list.

This is a Cargo workspace: `crates/atb` reads ATB's files (`.BOF` scenes,
Glide pictures and materials, `.3DF`, `.3DS`, `.VQ`) and walks scene graphs;
`crates/<demo>` is the player. `src/render.rs` draws ATB shapes as ATB's
Glide renderer did, through the shared `gl.rs`; `shaders/demo.wgsl` holds the
Glide combine program. `crates/atb`, `render.rs` and `demo.wgsl` are the
same in every ATB port: change them in `demoroom` and copy them over.

Readers refuse unknown formats and never read past the end. Demo content is
never committed: `data/`, `original/`, `target/`, `dist/` are git-ignored, and
`deploy-cloudflare.sh` bundles a packed copy only for the deployment.
