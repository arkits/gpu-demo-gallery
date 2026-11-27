# Still Life

A port of STMicroelectronics' Still Life (version 1.0, 2001, Direct3D 7, Kyro II) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. Fruit, a
bowl and a lamp on a table under a moving spotlight. A window in the middle of the screen shows
the same scene as a pencil sketch on paper: six hatching pictures of rising darkness, each with
its own mipmaps so that the strokes keep their width up close, picked by how much light each
vertex gets, with outlines from a silhouette picture. The window can be moved, resized and
bounced.

Play it: https://still-life-port.pages.dev

The demo is one program with everything compiled into it, packed with UPX. It is
STMicroelectronics' and is not in this repository. Unpack it (`upx -d`) and put it in `data/`
as `still.exe`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Arrows | Move the sketch window |
| i, d | Larger, smaller window |
| a | Let the window bounce |
| c | The credits |
| Space, r | Pause, from the start |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
