# Last Chance Gas

A Rust and wgpu port of NVIDIA's **Last Chance Gas** (Gas Station) GeForce FX demo (2003, OpenGL
and Cg). A gas station in the desert from dawn to dusk: shadows that soften with the sun's height,
taken from precomputed per-texel rise and fall times, a Preetham sky, billboard clouds and an HDR
glow chain. The player reads the demo's own scene files (`.nmb`) and runs its Cg programs, compiled
by the demo's own Cg runtime and translated from NV assembly to WGSL.

[Open the player](https://gas-station-port.pages.dev/) (WebGPU, or WebGL2 where there is no WebGPU)

The page downloads the demo's files with it (packed losslessly). The files are NVIDIA's and are
not in this repository. For a local build, link or copy the installed demo's folder (`bin`,
`models`, `shaders`, `textures`, `UserInterface`, `sounds`) to `data/`, then compile its programs
once with its own `cg.dll`, run under Unicorn (`pip install unicorn pefile`):

```sh
python3 tools/nv_compile.py data data/asm   # the programs, as the viewer compiles them
cargo run --release -- data                 # native player
cargo run --release --example shot -- 20 target/shot.png   # a frame, 20 s into the camera tour
sh scripts/build-web.sh && sh scripts/pack.sh target/package/gas-station.xz
python3 scripts/serve.py 8080               # browser player at http://127.0.0.1:8080/
sh scripts/deploy-cloudflare.sh             # Cloudflare Pages, with the files
```

It plays as the original does. The menus at the top left are the demo's: Navigation (Play the
camera tour, Pause, Walk, Sliders for the day's speed, shadow softness and cloud size) and
Drawstyle (normal or wireframe). The arrow keys or a left drag walk, kept off the walls by the
demo's collision map; a right drag looks around. After 30 seconds without a drag the camera tour
starts, and it starts again at dusk. Keys: Space pause, `*` and `/` faster and slower, `-`
backwards, `s` and `S` softer and sharper shadows, `w` wireframe, `c` camera tour, Esc quit (native).
The port adds M for mute and Enter for full screen. Both sound loops play throughout, as in the
original.

The engine (`crates/nv-engine`) is NVIDIA's 2003 demo framework rebuilt from `GasStation.exe`:
the scene graph, render passes and render list, Maya transforms and curves, GL-style program and
texture state over wgpu, and the menus. `crates/nv-asm` translates NV vertex and fragment programs
(vp20/vp30, fp20/fp30) to WGSL. See [the engine notes](docs/ENGINE.md).

Differences: the original asked for a 2-sample window; only the menus' edges were drawn to it (the
scene renders to a single-sample texture), and the port draws them without multisampling. The
screenshot key (G) is not ported.
