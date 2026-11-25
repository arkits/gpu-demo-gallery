# Lightning

A port of NVIDIA's "Lightning" demo for the GeForce 256 (April 2000, OpenGL) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A bolt
strikes a steel plate, lights the bumps in it and in the walls, heats it and throws sparks.
It reads the pictures and sounds inside the demo's own program.

Play it: https://lightning-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put your
`lightning000419.exe` in `data/`. It is only read, never run.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Hold the plate | Lightning |
| Space | Lightning that stays (not in the original) |
| b, w | Bumps, wireframe |
| x | More sparks (`m` in the original) |
| Arrows, scroll | Around, nearer and farther |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
