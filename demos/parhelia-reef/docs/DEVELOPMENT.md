# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` draws several moments natively, in WebGPU and in
WebGL2 and compares the pixels. The packages are unsigned; the Windows one is built with
mingw-w64 and is not run by any check.

- `src/demo.rs` is the scene: actors, relationships, cameras, animation, shader constants and
  the draws. `src/script.rs` decrypts and parses the `.s` and `.w` scripts. `src/xfile.rs`
  reads DirectX .X files (text, binary and MSZIP-compressed). `src/shader.rs` translates
  vs.1.1 and ps.1.0 to 1.3 assembly to WGSL.
- `src/d3d.rs` is a Direct3D 6 to 8 pipeline on wgpu, shared as a copy with the other ports of
  this batch; here the programmable path (`draw_program`) does the work: render and texture
  stage states by D3D number, constant registers, 2D and volume textures.
- `data.rs` finds the demo's files, loose or packed. `image.rs` reads the pictures.
- `player.rs`, `native.rs` and `web.rs` run it in a window and in a page.
- `examples/`: `shot` renders one moment; `shaders` translates every shader (or prints one
  pair's WGSL); `xdump`, `xnodes`, `xframes` and `xquat` look inside .X files.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
demo's files, packed with xz and split into parts under the 25 MiB file limit
(`local/assets.json` lists them; a new pack changes its version). It is not connected to Git.
