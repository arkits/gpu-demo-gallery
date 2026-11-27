# ATB-TV

A television on a marble stand turns slowly while a 73-frame movie plays on its curved screen. `atbtv.exe` builds the set in code and plays every `.3df` in `movie/` as an animated texture.

A port of 3dfx's "ATB-TV" demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://atb-tv-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Space | Turn on/off |
| a | Movie on/off |
| c | Play the movie on the screen, stand or floor |
| l | Lighting: plain, shiny, flat |
| - / = | Shininess |
| m | Untextured |
| Up / Down | Move the camera |
| Left / Right | Widen / narrow the view |
| w / t | Wireframe / textures |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
