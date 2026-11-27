# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` draws several moments natively, in WebGPU and in
WebGL2 and compares the pixels. The packages are unsigned; the Windows one is built with
mingw-w64 and is not run by any check.

- `src/demo.rs` is the flight: rooms by bucket, mirrors, tanks, pools, characters, fades.
  `src/ati.rs` reads radiATIon worlds, `src/path.rs` camera paths, `src/proteus.rs` the
  characters and their animations, `src/water.rs` the pools.
- `src/d3d.rs` and `shaders/ff.wgsl` are a Direct3D 6 to 8 fixed-function pipeline on wgpu
  (render and texture stage states by D3D number, texture coordinate generation and
  transforms, lights, and here user clip planes), shared as a copy with the other ports of
  this batch.
- `data.rs` finds the demo's files, loose or packed. `image.rs` reads the pictures.
- `player.rs`, `native.rs` and `web.rs` run it in a window and in a page.
- `examples/`: `shot` renders one moment; `pack` makes and checks packs.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
demo's files, packed with xz and split into parts under the 25 MiB file limit
(`local/assets.json` lists them; a new pack changes its version). It is not connected to Git.
