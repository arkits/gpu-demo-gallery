# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` draws several moments natively, in WebGPU and in
WebGL2 and compares the pixels. The packages are unsigned; the Windows one is built with
mingw-w64 and is not run by any check.

`src/demo.rs` is the demo and `src/kyro.rs` the framework's tables, read from the program.
`src/d3d.rs` and `shaders/ff.wgsl` are a Direct3D 6 to 8 fixed-function pipeline: render
states, texture stage states, lights and materials by their D3D numbers, ProcessVertices on
the CPU, stencil and blending as pipeline state. It is shared, as a copy, with the other ports
of this batch.

- `data.rs` finds the demo's files, loose or packed. `image.rs` reads the pictures.
- `player.rs`, `native.rs` and `web.rs` run it in a window and in a page.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
demo's files. It is not connected to Git.
