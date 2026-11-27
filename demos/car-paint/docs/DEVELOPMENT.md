# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`. `compare` covers a scissored and a split chapter: frame rates
alone did not notice that WebGL's resolve obeyed the scissor and left half the picture black.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
repacked `Sushi.ati` and the music. It is not connected to Git.
