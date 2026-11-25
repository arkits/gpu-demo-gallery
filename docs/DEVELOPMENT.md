# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` draws several moments natively, in WebGPU and in
WebGL2 and compares the pixels.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
repacked `Sushi.ati`. It is not connected to Git.
