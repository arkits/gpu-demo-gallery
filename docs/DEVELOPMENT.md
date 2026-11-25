# Development

The native and WebGPU players use the same Sushi engine. Run `scripts/dev.sh` for build,
test, render, and package commands. `SUSHI_ATI` can point to a different local copy of the
original archive. The shader and format tests use `data/Sushi.ati` when present.

Browser checks use an installed Chrome and Playwright. `scripts/dev.sh compare` compares a
representative frame from WebGPU and the native renderer. `scripts/dev.sh sweep` samples the
timeline and reports renderer warnings and near-black frames, including intentional fades.

`scripts/deploy-cloudflare.sh` publishes the static WebGPU player to Cloudflare Pages.
Native build scripts produce macOS and Windows packages.
