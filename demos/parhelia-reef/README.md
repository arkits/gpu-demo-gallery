# Parhelia Reef

A port of Matrox's Reef demo for the Parhelia-512 (2002, Direct3D 8, vertex shaders 1.1 and
pixel shaders 1.3) to current systems: browsers (WebGPU, or WebGL2), macOS and Windows.
Written in Rust with wgpu. A 95-second camera tour of a coral reef: schools of fish, rays, a
cuttlefish, an angler in its cave, seahorses, jellyfish and a shark, under moving caustics,
to the demo's music. Ten more cameras follow single animals. Everything comes from the demo's
own data: its encrypted scene and shader scripts, DirectX .X models with skins and animation,
TGA pictures with their mipmaps, a volume texture for the caustics, and 48 assembly shaders,
translated to WGSL when the demo starts.

Play it: https://parhelia-reef-port.pages.dev

The demo's files are Matrox's and are not in this repository. Put its `Data` folder in
`data/`. For music in the native player, add `music.flac` (or `.wav`) made from
`Data/Sounds/music.wma`, for example `ffmpeg -i music.wma music.flac`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Space | Pause, play |
| r | From the start |
| c | Next camera (as the original's SwitchCamera) |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
