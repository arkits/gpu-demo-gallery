# Pipe Dream

A native port of ATI's 2003 "Pipe Dream" Radeon 9700 tech demo to modern systems,
written in Rust with wgpu: browser, macOS and Windows from one codebase.

It reads the original data files and runs the original shaders, translated from D3D9
assembly to WGSL when the data loads. This repository has code and notes only: you need
your own copy of the demo. Its art and music belong to ATI and Animusic and must not be
committed or redistributed.

## Run

Put the demo folder `ATI_Radeon-9700_PipeDream-Demo_v1.1` in the repository root, or
pass a path to `Sushi.ati` or its folder.

    cargo run --release -p pipedream                      # native window
    scripts/build-web.sh && python3 scripts/serve.py      # browser, http://127.0.0.1:8080/
    scripts/package-macos.sh                              # dist/Pipe Dream.app
    scripts/package-windows.sh                            # dist/windows/PipeDream.exe

The music, `AnimusicIntro.mp3`, is found in `Audio/` beside `Sushi.ati`; without it the
show plays silently. `--mute` starts the native player muted.

| Key | Action |
|---|---|
| Space | Pause |
| Left, Right, Home | Skip 5 s, restart |
| F | Fly-through or free camera |
| 1-4 | Free camera at each instrument; drag to turn, scroll to zoom |
| L | Logos |
| M | Mute |
| Enter (or F11, native) | Full screen |
| Esc | Quit (native) |

## Layout

| Path | What |
|---|---|
| `crates/sushi-formats` | Strict readers for the original data files |
| `crates/sushi-shader` | `.ssh` parser and D3D9 assembly to WGSL translator |
| `crates/sushi-engine` | Animation, lights, cameras and the frame |
| `crates/pipedream` | Show logic, native window and browser player |
| `web`, `scripts`, `tools` | Web page, builds, Ghidra script |
| `docs` | [Plan and status](docs/PORT_PLAN.md), [engine behavior](docs/ENGINE.md), [file formats](docs/formats/README.md), [shaders](docs/SHADERS.md), [development](docs/DEVELOPMENT.md) |
