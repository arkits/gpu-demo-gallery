# 60Hz / 30Hz / 15Hz

A port of 3dfx's "60Hz/30Hz/15Hz" demo (1998) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu. It shows the same scene updated at different rates so the motion difference is visible.

Play it: https://sixty-hertz-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| 1-9 | Camera presets |
| r | Toggle 30/15Hz side |
| . | Step frame |
| Space | Spin on/off |
| b | Bounce on/off |
| h | Help overlay toggle |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
