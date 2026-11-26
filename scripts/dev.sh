#!/bin/sh
# Development and checks for the ATB pioneer. Nothing here opens a window except
# `run`. Needs the demo's files in data/ (or FLIP_DATA).
#
#   scripts/dev.sh run [args]     build and open the native player
#   scripts/dev.sh check          format, lints (native and wasm) and tests
#   scripts/dev.sh shot T KEYS [out]  render the moment T after pressing KEYS
#   scripts/dev.sh smoke [secs]   run the native player in a hidden window
#   scripts/dev.sh web [port]     build and serve the browser version (8080)
#   scripts/dev.sh webtest        drive the browser version (WebGPU and WebGL2)
#   scripts/dev.sh compare        same frames natively, in WebGPU and in WebGL2
#   scripts/dev.sh all            check, smoke, webtest, compare
set -eu
cd "$(dirname "$0")/.."
FLIP_DATA=${FLIP_DATA:-$PWD/data}
export FLIP_DATA
OUT=target/devtest
PORT=8107
mkdir -p "$OUT"
# Seconds and keys of the moments that `compare` looks at.
STILLS="2: 5:t 8:w 11:p"

need_data() {
  [ -d "$FLIP_DATA" ] || { echo "demo not found: $FLIP_DATA" >&2; exit 1; }
}
shot() { cargo run -q --release -p flip --example shot -- "$FLIP_DATA" "$@"; }
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
  sh scripts/pack.sh target/package/flip.xz >"$OUT/pack.log" 2>&1 || { cat "$OUT/pack.log"; exit 1; }
}

command=${1:-help}
[ $# -eq 0 ] || shift
case $command in
run) need_data; exec cargo run -q --release -p flip --bin flip -- "$@" "$FLIP_DATA" ;;
web) need_data; build_web; exec python3 scripts/serve.py "${1:-8080}" ;;
check)
  need_data
  cargo fmt --check
  cargo clippy -q --workspace --all-targets -- -D warnings
  cargo clippy -q -p flip --lib --target wasm32-unknown-unknown -- -D warnings
  cargo test -q --workspace
  ;;
shot) need_data; shot "$1" "${2:-}" "${3:-$OUT/shot.png}" ;;
smoke)
  need_data
  cargo run -q --release -p flip --bin flip -- --smoke "${1:-5}" | tee "$OUT/smoke.log"
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
all) for c in check smoke webtest compare; do echo "== $c"; sh "$0" $c; done; echo "all passed" ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' ;;
esac
