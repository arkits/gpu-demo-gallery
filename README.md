# Pipe Dream

A native port of ATI's 2003 "Pipe Dream" Radeon 9700 tech demo to modern systems,
written in Rust with wgpu: browser, macOS and Windows from one codebase.

This repository has code and notes only. You need your own copy of the original
demo; its art and music belong to ATI and Animusic and must not be committed.

## Status

See [docs/PORT_PLAN.md](docs/PORT_PLAN.md). Today: all data files are read and the
static room renders along the original camera path, in a browser and natively.
Stage lights, animation, shadows and music are not in yet.

## Run

Put the demo folder `ATI_Radeon-9700_PipeDream-Demo_v1.1` in the repository root.

    cargo run --release -p pipedream -- "path/to/Sushi.ati"   # native; Esc quits, f fullscreen
    scripts/build-web.sh && python3 scripts/serve.py          # browser, http://127.0.0.1:8080/
    cargo test                                                # needs the demo data

More in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Layout

| Path | What |
|---|---|
| `crates/sushi-formats` | Strict readers for the original data files |
| `crates/sushi-engine` | Camera, rendering, (later) simulation |
| `crates/pipedream` | Native window and browser player |
| `web`, `scripts` | Web page, build and local server |
| `tools/ghidra` | Scripts used to study the original program |
| `docs/formats` | File formats |
| `docs/ENGINE.md` | How the original engine behaves, with addresses |
