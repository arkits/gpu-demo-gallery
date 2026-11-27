# Development

`scripts/dev.sh` lists the commands. Run `all` with the original data to lint native and
WASM builds, verify asset parsing, render in a hidden native window and run Chrome
tests for WebGPU and WebGL2. The comparison checks four frozen frames from each browser
backend against native frames.

`?time=5&yaw=0.8&pitch=0.2&action=1` shows a frozen moment. Use the same values with
`scripts/dev.sh shot` to save a native frame.

`scripts/deploy-cloudflare.sh --prepare` stages the web build and lossless assets in
`dist/cloudflare`. Assets larger than 24 MiB are split into numbered parts, and audio
is compressed to FLAC. Git ignores the original files and generated output.
