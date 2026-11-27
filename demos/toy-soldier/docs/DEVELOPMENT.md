# Development

`scripts/dev.sh` lists the commands. Run `all` with the original data available to lint
native and WASM builds, verify asset parsing, render in a hidden native window and
run Chrome tests for WebGPU and WebGL2. The browser tests verify the canvas backend,
music, controls and narrow layouts. The comparison checks four frozen frames from
each backend against the native frames.

`?time=5&yaw=0.8&pitch=0.2&action=1` shows a frozen moment. Use the same arguments with
`scripts/dev.sh shot` to save a native frame.

`scripts/deploy-cloudflare.sh --prepare` stages the web build and lossless assets,
including FLAC audio, in `dist/cloudflare`. The deployment command without
`--prepare` publishes that folder to Pages. Git ignores the original files and generated output.
