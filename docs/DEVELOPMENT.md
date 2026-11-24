# Development

## Tools
Rust 1.92 or newer with the `wasm32-unknown-unknown` target. `scripts/build-web.sh`
installs the matching `wasm-bindgen` under `target/tools`, not globally.

## Checking frames
    cargo run --release -p sushi-engine --example shot  -- Sushi.ati 60 out.png
    cargo run --release -p sushi-engine --example sweep -- Sushi.ati 0.25
`shot` renders one frame offscreen at 1024x768. `sweep` renders the whole timeline and
reports warnings (missing shader variants or pipelines) and cost. In the browser, `?t=60`
freezes the same moment and `window.pipedream` exposes the player.

## Native packages
`package-macos.sh` makes an ad-hoc signed app (universal if `x86_64-apple-darwin` is
installed). `package-windows.sh` cross-builds with mingw-w64 and `x86_64-pc-windows-gnu`.
Without a path the player looks beside itself, then shows a file dialog.

## Inspecting data
    cargo run -p sushi-formats --example inspect_scene  -- Sushi.ati
    cargo run -p sushi-formats --example inspect_layout -- Sushi.ati
    cargo run -p sushi-formats --example inspect_anim   -- Sushi.ati
    cargo run -p sushi-formats --example inspect_mesh   -- Sushi.ati marimba-00
    cargo run -p sushi-formats --example glow_times     -- Sushi.ati

## Studying the original program
Addresses in the code and docs refer to `SushiDX.exe` (image base 0x400000).

    JAVA_HOME=<jdk 25> <ghidra 12.2>/support/analyzeHeadless <project dir> pd \
      -import SushiDX.exe -scriptPath tools/ghidra -postScript DumpAll.java all.c

`DumpAll.java` decompiles everything below 0x470000: the engine, without most of the
statically linked D3DX and C runtime. Do not commit its output.

REA (`rea-agents`) is an alternative front end to the same Ghidra.

## Git
One branch per milestone, pushed as work lands. Never commit demo data; `.gitignore`
covers the demo folder, `.ati` and `.mp3`.
