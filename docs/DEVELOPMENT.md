# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a window except `run` and `open`.

`src/demo.rs` and `shaders/demo.wgsl` are the demo. The rest is shared, as a copy, with the other small ports:

- `gl.rs` draws as the old APIs did: a program, up to four pictures, blending, depth. Vertices come already transformed. Four samples a pixel.
- `data.rs` finds the demo's files, loose or packed. `image.rs` reads the pictures. Both refuse what they do not know, and never read past the end.
- `player.rs`, `native.rs` and `web.rs` run it in a window and in a page.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the demo's files. It is not connected to Git.
