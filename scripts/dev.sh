#!/bin/sh
# Development and testing in one place. Nothing here opens a window except
# `run` and `open`. Needs the demo's files in data/, or BUBBLE_DATA.
#
#   scripts/dev.sh run [args]        build and open the native player in front
#   scripts/dev.sh open              build and open the macOS app
#   scripts/dev.sh web [port]        build and serve the browser version (8080)
#   scripts/dev.sh check             format, lints (native and wasm) and tests
#   scripts/dev.sh shot T KEYS [out] render the moment T after pressing KEYS
#   scripts/dev.sh smoke [secs]      run the native player in a hidden window
#   scripts/dev.sh webtest           drive the browser version (WebGPU and WebGL2)
#   scripts/dev.sh compare           same frames natively, in WebGPU and in WebGL2
#   scripts/dev.sh package           macOS app and Windows exe
#   scripts/dev.sh all               check, smoke, webtest, compare
set -eu
cd "$(dirname "$0")/.."
BUBBLE_DATA=${BUBBLE_DATA:-$PWD/data}
export BUBBLE_DATA
OUT=target/devtest
PORT=8103
mkdir -p "$OUT"
# Seconds and keys of the moments that `compare` looks at.
STILLS="2: 3:s 2:t 2:st 1:w2 1:3p"

need_data() {
  [ -d "$BUBBLE_DATA" ] || { echo "demo not found: $BUBBLE_DATA" >&2; exit 1; }
}
shot() { cargo run -q --release --example shot -- "$BUBBLE_DATA" "$@"; }
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
build_web() {
  sh scripts/build-web.sh >"$OUT/build-web.log" 2>&1 || { cat "$OUT/build-web.log"; exit 1; }
  sh scripts/pack.sh target/package/bubble.xz >"$OUT/pack.log" 2>&1 || { cat "$OUT/pack.log"; exit 1; }
}

command=${1:-help}
[ $# -eq 0 ] || shift
case $command in
run) need_data; exec cargo run -q --release -- "$@" "$BUBBLE_DATA" ;;
open) sh scripts/package-macos.sh >"$OUT/package.log" 2>&1 || { cat "$OUT/package.log"; exit 1; }
  open "dist/Bubble.app" ;;
web) need_data; build_web; exec python3 scripts/serve.py "${1:-8080}" ;;
check)
  need_data
  cargo fmt --check
  cargo clippy -q --all-targets -- -D warnings
  cargo clippy -q --lib --target wasm32-unknown-unknown -- -D warnings
  cargo test -q --release
  ;;
shot) need_data; shot "$1" "${2:-}" "${3:-$OUT/shot.png}" ;;
smoke)
  need_data
  cargo build -q --release --bin bubble
  target/release/bubble --smoke "${1:-5}" "$BUBBLE_DATA" | tee "$OUT/smoke.log"
  # A hidden window cannot present, so frames are drawn offscreen.
  grep -q '^frames [1-9][0-9]' "$OUT/smoke.log"
  ;;
webtest) need_data; driver; build_web; browser "show webgpu" "show webgl" ;;
compare)
  need_data; driver; build_web
  for s in $STILLS; do
    shot "${s%%:*}" "${s#*:}" "$OUT/native-still.png" >/dev/null
    browser "still webgpu $s" "still webgl $s"
    python3 tools/compare_png.py --max-mean 1.5 "$OUT/native-still.png" "$OUT/webgpu-still.png" "$OUT/webgl-still.png"
  done
  ;;
package) sh scripts/package-macos.sh; sh scripts/package-windows.sh ;;
all) for c in check smoke webtest compare; do echo "== $c"; sh "$0" $c; done; echo "all passed" ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' ;;
esac
