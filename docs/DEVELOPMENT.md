# Development

`scripts/dev.sh` has every task; run it without arguments for the list. Nothing opens a
window except `run` and `open`.

| Check | What it shows |
|---|---|
| `check` | Formatting, lints (native and wasm), tests. The solver tests run on the GPU: values stay finite, smoke fades at the set rate and the logo keeps its smoke, projection lowers divergence, water stays level |
| `smoke` | The native player runs (hidden window, frames drawn offscreen) |
| `webtest` | Chrome with WebGPU and with WebGL2: frame rate in each mode, input, resizing |
| `compare` | Frames after 60 steps. WebGPU equals native exactly; WebGL2 differs by 0.01 of 255 on average |

The browser driver checks which API the canvas really uses: Chrome's flags for turning
WebGPU off have no effect, so the driver hides it from the page instead.

Deployment: `scripts/deploy-cloudflare.sh` uploads directly to Cloudflare Pages with the
demo files packed (2.8 MB). It is not connected to Git.
