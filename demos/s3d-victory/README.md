# S3D Victory

A port of ELSA's "S3D Victory" demo for the Victory 3DX, a card with S3's ViRGE (1997,
DOS/4GW with the S3D Toolkit), to current systems: browsers (WebGPU, or WebGL2), macOS and
Windows. Written in Rust with wgpu. Textured letters of ELSA and VICTORY 3DX fly in, spin
and circle in front of a starship; the second script spins textured cubes. It reads the
demo's own scripts, objects, textures and picture, and runs the scripts as the original's
interpreter does.

Play it: https://s3d-victory-port.pages.dev

The demo's files are ELSA's and are not in this repository. Put the contents of its folder
in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1, 2 | `SCRIPT.SCP`, `SCRIPT2.SCP` (the original takes the script on its command line) |
| Space | Pause |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
