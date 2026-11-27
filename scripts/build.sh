#!/bin/sh
# Assembles dist/ from web/ and frames that the demos' own tests rendered (run
# `scripts/dev.sh compare` in each first). Images never enter Git.
set -eu
cd "$(dirname "$0")/.."
D=${DEMOS:-$PWD/demos}
# A port not yet moved into demos/ is still beside this repository.
at() { if [ -d "$D/$1" ]; then echo "$D/$1"; else echo "$PWD/../$1"; fi; }
mkdir -p dist/img
cp web/index.html dist/
# make NAME SOURCE CROP. Some frames are pre-rendered by another session's
# `dev.sh compare`; when that PNG is absent, keep the image already in dist/img.
make() {
  src=$(at "${2%%/*}")/${2#*/}
  if [ -f "$src" ]; then
    ffmpeg -v error -y -i "$src" -vf "crop=$3,scale=1600:-2:flags=lanczos" -frames:v 1 -q:v 4 "dist/img/$1.jpg"
  elif [ -f "dist/img/$1.jpg" ]; then
    : # keep the previously rendered frame
  else
    echo "skip $1: no source $2 and no existing image" >&2
  fi
}
# shot REPOSITORY ARGUMENTS... renders a moment with the demo's own script.
# A sibling repo may be mid-edit and fail to render; in that case keep the
# frame already in dist/img instead of aborting the whole gallery build.
shot() {
  demo=$1; shift
  dir=$(at "$demo")
  out=$dir/target/devtest/gallery.png
  if sh "$dir/scripts/dev.sh" shot "$@" "$out" >/dev/null 2>&1 && [ -f "$out" ]; then
    ffmpeg -v error -y -i "$out" -vf "crop=$crop,scale=1600:-2:flags=lanczos" -frames:v 1 -q:v 4 "dist/img/$demo.jpg"
  elif [ -f "dist/img/$demo.jpg" ]; then
    echo "keep $demo: render failed, using existing image" >&2
  else
    echo "skip $demo: render failed and no existing image" >&2
  fi
}
crop=in_w:in_h
shot borg 10 68
shot bubble 1 3p
shot lightning 2 " "
shot npr-hatching 1 1
shot natural-light 1 24
shot bear 1 5
shot double-cross 1 5.5
shot dangerous-curves 1 20
shot the-assassin 1 82
crop=800:768:224:0
shot grove 3 g8
crop=in_w:600:0:0
shot grass 6 ""
crop=in_w:468:0:300
shot pond 1.2 xxx
make donut donut/target/devtest/webgpu-show.png 640:640:192:64
make principles-of-shading principles-of-shading/target/devtest/native-15.png 720:768:304:0
make creature creature/target/devtest/native-a.png 1024:700:0:0
make car-paint car-paint/target/devtest/webgpu-chapter3.png 504:620:514:72
make pipe-dream pipedream/target/devtest/native-still.png 1024:768:0:0
make box-of-smoke box-of-smoke/target/devtest/native-s.png 840:768:150:0
crop=in_w:in_h
shot toy-soldier 3 2.1 0.05 1
shot crystal-ball 5 0 0 0
shot bugs 113 0 0 0
shot wolfman 8 0.8 -0.3 6
shot squid 5 0 0 0
shot chameleon 8 0.8 -0.3 1
shot tidepool 5 0 0 0
shot zoltar 5 0 0 1
shot dawn 28 0 0 -
shot ogre 15 0 0 -
crop=in_w:in_h
shot surf 4 ''
shot sixty-hertz 8 4r
shot demoroom 1 nnnnnn
shot atb-tv 3 ''
shot flip 1 3
shot grand-bleu 1 ''
shot racing 5 ''
shot valley-of-ra 0.3 o
shot wizards-tower 1 2
for demo in dolphin nature rachel island1 island2 island3 treasure-chest geoforms; do
  make "$demo" "radeon-8500/target/shots/$demo.png" in_w:in_h
done
echo '<!doctype html><title>Not found</title><p>Not found.' > dist/404.html
ls -l dist/img
