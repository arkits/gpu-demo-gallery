# Development

## Tools
Rust 1.92 or newer with the `wasm32-unknown-unknown` target. `scripts/build-web.sh`
installs the matching `wasm-bindgen` under `target/tools`, not globally.

## One script
`scripts/dev.sh` (no arguments for help) runs everything. Results go to `target/devtest`.

| Command | What it proves |
|---|---|
| `check` | Formatting, lints as errors (native and wasm), all tests |
| `sweep` | Every moment draws with exact shader variants and no missing pipeline |
| `smoke` | The native player starts, opens audio (muted) and draws, in a hidden window |
| `webtest` | Sync, seeking, frame rate and pause in Chrome, WebGPU and WebGL2 |
| `compare` | One frame matches between native, WebGPU and WebGL2 |
| `full` | The whole show in real time in Chrome: sync, late frames, restart |
| `all` | All of these except `full` |

macOS gives a hidden window no surface, so `smoke` draws its frames offscreen and does not
test showing them. The browser driver (playwright-core) installs under `target/tools` and
uses the installed Chrome. `?t=60` in the browser freezes one moment, and
`window.pipedream` exposes the player.

## Native packages
`package-macos.sh` makes an ad-hoc signed app (universal if `x86_64-apple-darwin` is
installed). `package-windows.sh` cross-builds with mingw-w64 and `x86_64-pc-windows-gnu`.
Without a path the player looks beside and above itself, then uses the last file chosen,
and only then shows a file dialog. Errors show a dialog only outside a terminal.

## Vercel
`scripts/deploy-vercel.sh` builds the browser version, adds the demo content in the
git-ignored `dist/vercel`, and deploys it publicly. It stops if that folder is not ignored.
The content is served from `/local/`, where `serve.py` puts it too. The account's
"Vercel Authentication" covers only per-deployment URLs, not the production one. The
content belongs to ATI and Animusic.

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
