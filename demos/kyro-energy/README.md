# Kyro Energy

A port of STMicroelectronics' Kyro Energy (version 1.0.1, 2002, Direct3D 7, Kyro II) to
current systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A
cold bottle of Kyro Energy on a beer mat on a marble table, the camera circling it.
Condensation runs down the glass: four drops with trails and ten still ones are drawn every
step into a bump map with luminance and a light map, and the bottle's label takes the light
map, then an environment-bump stage with those drops, then an added reflection. The table is
half transparent over a mirrored copy of the bottle, masked by the stencil.

Play it: https://kyro-energy-port.pages.dev

The demo is one program with everything compiled into it, packed with UPX. It is
STMicroelectronics' and is not in this repository. Unpack it (`upx -d`) and put it in `data/`
as `energy.exe`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| t, g | Luminance of the dry glass up, down (T and G in the original) |
| c | The credits |
| Space, r | Pause, from the start |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
