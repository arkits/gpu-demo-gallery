#!/bin/sh
# Development and testing in one place. Nothing here opens a window except
# `run` and `open`. Needs the demo folder at the repository root, or SUSHI_ATI.
#
#   scripts/dev.sh run [args]      build and open the native player in front
#   scripts/dev.sh open            build and open the macOS app
#   scripts/dev.sh web [port]      build and serve the browser version (8080)
#   scripts/dev.sh check           format, lints (native and wasm) and tests
#   scripts/dev.sh shot T [out]    render one frame at scene second T
#   scripts/dev.sh sweep [step]    render the whole timeline, report problems
#   scripts/dev.sh smoke [secs]    run the native player in a hidden window, muted
#   scripts/dev.sh webtest         drive the browser version (WebGPU and WebGL2)
#   scripts/dev.sh compare [T]     same frame natively, in WebGPU and in WebGL2
#   scripts/dev.sh full            play the whole show in headless Chrome (4 min)
#   scripts/dev.sh package         macOS app and Windows exe
#   scripts/dev.sh all             check, sweep, smoke, webtest, compare
set -eu
cd "$(dirname "$0")/.."
DEMO="ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream"
SUSHI_ATI=${SUSHI_ATI:-$PWD/$DEMO/Sushi.ati}
export SUSHI_ATI
OUT=target/devtest
PORT=8099
mkdir -p "$OUT"

need_data() {
  [ -f "$SUSHI_ATI" ] || { echo "Sushi.ati not found: $SUSHI_ATI" >&2; exit 1; }
}
shot() {
  cargo run -q --release -p sushi-engine --example shot -- "$SUSHI_ATI" "$1" "$2"
}
# Installs the browser driver under target/, not globally; uses installed Chrome.
driver() {
  if [ ! -d target/tools/webtest/node_modules/playwright-core ]; then
    npm install --silent --no-fund --no-audit --ignore-scripts \
      --prefix target/tools/webtest playwright-core@1.64.1
  fi
  PLAYWRIGHT_FROM=$PWD/target/tools/webtest
  export PLAYWRIGHT_FROM
}
# Runs webtest.mjs against a private server that always stops afterwards.
browser() {
  python3 scripts/serve.py "$PORT" >"$OUT/serve.log" 2>&1 &
  server=$!
  trap 'kill $server 2>/dev/null || true' EXIT INT TERM
  i=0
  until curl -sf "http://127.0.0.1:$PORT/" >/dev/null; do
    i=$((i + 1)); [ $i -lt 50 ] || { echo "server did not start" >&2; exit 1; }
    sleep 0.1
  done
  status=0
  for run in "$@"; do
    # shellcheck disable=SC2086 # words are separate arguments
    node tools/webtest/webtest.mjs "http://127.0.0.1:$PORT/" "$OUT" $run || status=1
  done
  { kill $server && wait $server; } 2>/dev/null || true
  trap - EXIT INT TERM
  return $status
}
build_web() { sh scripts/build-web.sh >"$OUT/build-web.log" 2>&1 || { cat "$OUT/build-web.log"; exit 1; }; }

command=${1:-help}
[ $# -eq 0 ] || shift
case $command in
run) need_data; exec cargo run -q --release -p pipedream -- "$@" "$SUSHI_ATI" ;;
open) sh scripts/package-macos.sh >"$OUT/package.log" 2>&1 || { cat "$OUT/package.log"; exit 1; }
  open "dist/Pipe Dream.app" ;;
web) build_web; exec python3 scripts/serve.py "${1:-8080}" ;;
check)
  need_data
  cargo fmt --check
  cargo clippy -q --workspace --all-targets -- -D warnings
  cargo clippy -q -p pipedream --lib --target wasm32-unknown-unknown -- -D warnings
  cargo test -q --workspace
  ;;
shot) need_data; shot "$1" "${2:-$OUT/shot-$1.png}" ;;
sweep)
  need_data
  cargo run -q --release -p sushi-engine --example sweep -- "$SUSHI_ATI" "${1:-0.25}" | tee "$OUT/sweep.log"
  ! grep -q '^warning' "$OUT/sweep.log"
  ;;
smoke)
  need_data
  cargo build -q --release -p pipedream
  target/release/pipedream --smoke "${1:-5}" "$SUSHI_ATI" | tee "$OUT/smoke.log"
  # A hidden window cannot present, so frames are drawn offscreen.
  grep -q '^frames [1-9][0-9]' "$OUT/smoke.log"
  ;;
webtest) need_data; driver; build_web; browser "show webgpu" "show webgl" ;;
compare)
  need_data; driver; build_web
  t=${1:-100}
  shot "$t" "$OUT/native-still.png"
  browser "still webgpu $t" "still webgl $t"
  python3 tools/compare_png.py --max-mean 0.1 "$OUT/native-still.png" "$OUT/webgpu-still.png" "$OUT/webgl-still.png"
  ;;
full) need_data; driver; build_web; browser "full webgpu" ;;
package) sh scripts/package-macos.sh; sh scripts/package-windows.sh ;;
all) for c in check sweep smoke webtest compare; do echo "== $c"; sh "$0" $c; done; echo "all passed" ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' ;;
esac
