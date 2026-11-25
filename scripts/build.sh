#!/bin/sh
# Assembles dist/ from web/ and frames that the demos' own tests rendered (run
# `scripts/dev.sh compare` in each first). Images never enter Git.
set -eu
cd "$(dirname "$0")/.."
D=${DEMOS:-$PWD/..}
mkdir -p dist/img
cp web/index.html dist/
# make NAME SOURCE CROP
make() { ffmpeg -v error -y -i "$D/$2" -vf "crop=$3,scale=1600:-2:flags=lanczos" -frames:v 1 -q:v 4 "dist/img/$1.jpg"; }
# shot REPOSITORY ARGUMENTS... renders a moment with the demo's own script.
shot() {
  demo=$1; shift
  out=$(cd "$D/$demo" && pwd)/target/devtest/gallery.png
  sh "$D/$demo/scripts/dev.sh" shot "$@" "$out" >/dev/null
  ffmpeg -v error -y -i "$out" -vf "crop=$crop,scale=1600:-2:flags=lanczos" -frames:v 1 -q:v 4 "dist/img/$demo.jpg"
}
crop=in_w:in_h
shot borg 10 68
shot bubble 1 3p
shot lightning 2 " "
shot npr-hatching 1 1
shot natural-light 1 24
shot bear 1 5
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
shot bugs 5 0 0 0
shot wolfman 8 0.8 -0.3 1
shot squid 5 0 0 0
shot chameleon 8 0.8 -0.3 1
shot tidepool 5 0 0 0
shot zoltar 5 0 0 0
echo '<!doctype html><title>Not found</title><p>Not found.' > dist/404.html
ls -l dist/img
