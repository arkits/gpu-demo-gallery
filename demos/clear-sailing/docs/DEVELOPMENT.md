# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` draws several moments natively, in WebGPU and in
WebGL2 and compares the pixels. The packages are unsigned; the Windows one is built with
mingw-w64 and is not run by any check.

`src/demo.rs` and `shaders/demo.wgsl` are the demo. The rest is shared, as a copy, with the
other small ports:

- `gl.rs` draws as the old APIs did: a program, up to four pictures, a cube map, blending,
  depth. Vertices come already transformed (or ride the ship's motion in `vsmotion`).
  Four samples a pixel.
- `data.rs` finds the demo's files, loose or packed (split for the 25 MiB Pages limit).
  `image.rs` reads the pictures, including DXT1/3/5. Both refuse what they do not
  know, and never read past the end.
- `nma.rs` and `nmb.rs` read the demo's text and binary scenes; every file must parse
  to the last byte. `scene.rs` holds the Maya math, mesh readers and triangle
  expansion shared by both.
- `player.rs`, `native.rs` and `web.rs` run it in a window and in a page.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
demo's files. It is not connected to Git.

## Unpacking the original

The task starts from `clear_sailing.exe` (InstallShield). The files used here came from
an `unpack4`-style extraction to `.../T/unpack4/cs/files` (elsewhere: any folder with
`bin/PirateShip.exe`, `models/`, `shaders/`, `textures/`, `audio/`, `ui/` works):

```sh
shasum bin/PirateShip.exe   # c6ccac9873e6501fd043a693767179d54ab12e67
```

Set `CLEAR_SAILING_DATA` to that folder (or copy it to `data/`, which is git-ignored),
then `sh scripts/dev.sh all`. The pack splits above ~28 MB raw so every
`target/package/clear-sailing-N.xz` stays under the 25 MiB Pages limit; the page
fetches `clear-sailing.parts` first and then each part, so no 404 is ever logged.
