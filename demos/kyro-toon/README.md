# KyroToon

A port of STMicroelectronics' KyroToon (version 1.0, 2001, Direct3D 7, Kyro II) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A cartoon
figure walks among orange arches, a Kyro II chip and bouncing balls. Every object is drawn with
two small textures whose coordinates the program makes each frame: the light a vertex receives
picks a flat tone, and how far its normal turns from the eye picks a black outline. The figure
and the arches throw flat shadows on the floor through the stencil buffer.

Play it: https://kyro-toon-port.pages.dev

The demo is one program with everything compiled into it, packed with UPX. It is
STMicroelectronics' and is not in this repository. Unpack it (`upx -d`) and put it in `data/`
as `toon.exe`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Space | Pause, play |
| r | From the start |
| c | The credits (as the original's C key) |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
