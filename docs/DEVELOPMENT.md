# Development

## Tools
Rust 1.92 or newer with the `wasm32-unknown-unknown` target. `scripts/build-web.sh`
installs the matching `wasm-bindgen` under `target/tools`, not globally.

## Checking frames
    cargo run --release -p sushi-engine --example shot -- Sushi.ati 60 out.png
renders one frame offscreen at 1024x768. In the browser, `?t=60` freezes the same
moment. At t=60, native Metal, Chrome WebGPU and Chrome WebGL2 differ by a mean of
0.001/255 (84 of 2.4 million channel values differ by more than 8), measured on
2026-11-24 on Apple silicon. Safari, Firefox and Windows are not tested yet.

## Inspecting data
    cargo run -p sushi-formats --example inspect_scene  -- Sushi.ati
    cargo run -p sushi-formats --example inspect_layout -- Sushi.ati
    cargo run -p sushi-formats --example inspect_anim   -- Sushi.ati

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
