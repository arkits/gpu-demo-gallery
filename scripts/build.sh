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
make principles-of-shading principles-of-shading/target/devtest/native-15.png 720:768:304:0
make creature creature/target/devtest/native-a.png 1024:700:0:0
make car-paint car-paint/target/devtest/webgpu-chapter3.png 504:620:514:72
make pipe-dream pipedream/target/devtest/native-still.png 1024:768:0:0
make box-of-smoke box-of-smoke/target/devtest/native-s.png 840:768:150:0
echo '<!doctype html><title>Not found</title><p>Not found.' > dist/404.html
ls -l dist/img
