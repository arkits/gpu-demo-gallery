# Grand Bleu

The animals of 3dfx's underwater demo — dolphins, a shark, a manta, a turtle, schools' fish and a sunken galleon — each from its own scene file, swimming its own animation. Space moves to the next animal, as the demo's automatic camera does.

A port of 3dfx's "Grand Bleu" demo (1997) to current systems: browsers (WebGPU, or WebGL2), macOS, and Windows. Written in Rust with wgpu.

Play it: https://grand-bleu-port.pages.dev

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

| Input | Action |
|---|---|
| Space | Next animal |
| p, F8 | Freeze |
| Drag | Turn round the animal |
| w / t | Wireframe / textures |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
