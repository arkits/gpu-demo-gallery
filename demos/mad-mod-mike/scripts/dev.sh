#!/bin/sh
# Development and testing in one place. Nothing here opens a window except
# `run` and `open`. Needs the unpacked demo in data/, or MADMODMIKE_DATA.
#
#   scripts/dev.sh run [args]      build and open the native player in front
#   scripts/dev.sh open            build and open the macOS app
#   scripts/dev.sh web [port]      build and serve the browser version (8080)
#   scripts/dev.sh check           format, lints (native and wasm) and tests
#   scripts/dev.sh shot [args]     render one frame (see examples/shot.rs)
#   scripts/dev.sh smoke [secs]    run the native player in a hidden window
#   scripts/dev.sh webtest         drive the browser version (WebGPU and WebGL2)
#   scripts/dev.sh compare         same frames natively, in WebGPU and in WebGL2
#   scripts/dev.sh package         macOS app and Windows exe
#   scripts/dev.sh all             check, smoke, webtest, compare
set -eu
cd "$(dirname "$0")/.."
MADMODMIKE_DATA=${MADMODMIKE_DATA:-$PWD/data}
export MADMODMIKE_DATA
OUT=target/devtest
PORT=8112
mkdir -p "$OUT"
# Take, seconds and keys of the moments that `compare` looks at.
STILLS="0:1: 1:3: 2:1.5: 3:1: 4:2: 5:2: 6:1.5: 7:21: 8:2: 9:2:"

need_data() {
  [ -f "$MADMODMIKE_DATA/models/geom.nmb" ] || { echo "demo not found: $MADMODMIKE_DATA" >&2; exit 1; }
}
shot() { cargo run -q --release --example shot -- "$MADMODMIKE_DATA" "$@"; }
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
  sh scripts/pack.sh target/package/mad-mod-mike.xz >"$OUT/pack.log" 2>&1 || { cat "$OUT/pack.log"; exit 1; }
}

command=${1:-help}
[ $# -eq 0 ] || shift
case $command in
run) need_data; exec cargo run -q --release -- "$@" "$MADMODMIKE_DATA" ;;
open) sh scripts/package-macos.sh >"$OUT/package.log" 2>&1 || { cat "$OUT/package.log"; exit 1; }
  open "dist/Mad Mod Mike.app" ;;
web) need_data; build_web; exec python3 scripts/serve.py "${1:-8080}" ;;
check)
  need_data
  cargo fmt --check
  cargo clippy -q --all-targets -- -D warnings
  cargo clippy -q --lib --target wasm32-unknown-unknown -- -D warnings
  cargo test -q --release
  ;;
shot) need_data; shot "$@" ;;
smoke)
  need_data
  cargo build -q --release --bin mad-mod-mike
  "${CARGO_TARGET_DIR:-target}/release/mad-mod-mike" --smoke "${1:-5}" "$MADMODMIKE_DATA" | tee "$OUT/smoke.log"
  # A hidden window cannot present, so frames are drawn offscreen.
  grep -q '^frames [1-9][0-9]' "$OUT/smoke.log"
  ;;
webtest) need_data; driver; build_web; browser "show webgpu" "show webgl" ;;
compare)
  need_data; driver; build_web
  shot() { cargo run -q --release --example shot -- "$MADMODMIKE_DATA" "$@"; }
  for s in $STILLS; do
    take=${s%%:*}; rest=${s#*:}; secs=${rest%%:*}; keys=${rest#*:}
    shot "$secs" "$take" 0 0 "$keys" "$OUT/native-$take.png" >/dev/null
    browser "still webgpu $secs $take $keys" "still webgl $secs $take $keys"
    python3 tools/compare_png.py --max-mean 2.5 "$OUT/native-$take.png" "$OUT/webgpu-still-$take.png" "$OUT/webgl-still-$take.png"
  done
  ;;
package) sh scripts/package-macos.sh; sh scripts/package-windows.sh ;;
all) for c in check smoke webtest compare; do echo "== $c"; sh "$0" $c; done; echo "all passed" ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' ;;
esac
