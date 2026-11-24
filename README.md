# Car Paint (work in progress)

A port of ATI's Radeon 9700 "Car Paint" demo (2002) to browsers, macOS and Windows, built
on the Sushi engine from the Pipe Dream port. The demo's files are ATI's and are not in this
repository: put your `Sushi.ati` and `Audio` folder in `data/`.

    cargo run --release -p sushi-engine --example shot -- data/Sushi.ati 4 12 out.png

## Status

Done: all data parses, every shader needed at 12 s of chapters 1 and 4 translates, mirrors
(`0x415610`), frame scripts, viewports and scissors, 16-bit normal maps (as half floats),
alpha test, text and outlines. The five chapters load.

Left:
- `crates/carpaint` is still Pipe Dream's player and does not build: needs chapters with
  1 s fades and automatic advance (`cycleAllModes`), keys 1 to 5, the orbit camera.
- Music: MS ADPCM WAVs, so they need a decoder. Intro, then the loop starting 6.92 s before
  the intro ends; both restart with each chapter (0x4253a0).
- `scripts/`, `web/`, `tools/` and the crates' data tests are still Pipe Dream's copies.
- `global/gvlight.ssh` has no shader for the chapter's property in the mirror, so it is
  skipped there with a warning. Check whether the original skips it too.
- Look at every chapter along its whole path; check WebGPU, WebGL2 and native.
