# Demoroom

A port of 3dfx's "Demoroom" ATB demo to current systems. This is the pioneer ATB port in this workspace and owns `crates/atb`, the shared ATB engine/readers for later ATB demos.

The demo's files are 3dfx's and are not in this repository. Put its `data` folder here.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh check    format, lints (native + wasm), tests
    scripts/dev.sh shot T KEYS [out]   render one moment to a PNG
    scripts/dev.sh smoke [secs]        run in a hidden window
    scripts/dev.sh all      check, smoke, webtest (WebGPU+WebGL2), compare

This is the **ATB pioneer**: it owns `crates/atb`, the shared engine/readers that
the other ATB demos build on. It renders real ATB content — the `DEMOLITE.3DS`
room geometry textured with a `.3DF` texture, with the `demoroom.bof` name table
loaded through those readers. See [docs/ENGINE.md](docs/ENGINE.md).

Status: BOF / 3DF / 3DS readers are validated against the staged files; native
player, WebGPU/WebGL2 browser version, headless shots and `dev.sh all` all work.
The original's projected-light cycling, chromakey multipass and per-object
materials are still to come.

| Input | Action |
|---|---|
| w | Wireframe |
| p | Pause spin |
| Drag | Turn camera |
| Scroll | Move in / out |
| Enter, Esc | Full screen, quit (native) |
