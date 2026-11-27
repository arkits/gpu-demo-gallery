# Blobby Dancer

A port of NVIDIA's "Blobby Dancer" demo for the GeForce FX (2003, OpenGL with Cg) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A dancer
of a hundred blobs on springs, melted into one surface each frame, dances on a mirrored
floor, breaks up and swirls between moves, and changes its material every few seconds. It
reads the demo's own animation, pictures, music and fragment programs, which it translates
to WGSL when it starts.

Play it: https://blobby-dancer-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put the contents of its folder
(`BlobbyDancer32.exe`, the `.asm` programs, `Normal_*.tga`, `music000.mp3`) in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Drag | Turn |
| Up, down, scroll | Zoom |
| n | Next look |
| c | Next move |
| r | Turning on or off |
| Space | Pause |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
