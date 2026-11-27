#!/bin/sh
# Moves a port that still sits beside this repository (../NAME, with its own
# .git) into demos/NAME: merges the commits it gained since it was imported,
# keeps its old .git in ../_standalone-git, moves the folder (with its
# git-ignored data/ and target/) and stops hiding demos/NAME.
#   scripts/absorb.sh NAME...
set -eu
cd "$(dirname "$0")/.."
old=../_standalone-git
for name in "$@"; do
  src=../$name
  [ -d "$src/.git" ] || { echo "$name: no $src/.git" >&2; exit 1; }
  [ -z "$(git -C "$src" status --porcelain)" ] || {
    echo "$name: commit its changes first" >&2; exit 1; }
  [ ! -e "demos/$name" ] || { echo "$name: demos/$name already exists" >&2; exit 1; }
  git fetch -q "$src" main:refs/import/"$name" --force
  if ! git merge-base --is-ancestor refs/import/"$name" HEAD; then
    git merge -q --no-edit -X subtree="demos/$name/" refs/import/"$name" \
      -m "Merge later commits of $name from its own repository"
  fi
  mkdir -p "$old"
  mv "$src/.git" "$old/$name.git"
  mv "$src" "demos/$name"
  git sparse-checkout list | grep -vx "!/demos/$name/" > .git/absorb-patterns
  if [ "$(grep -c '^!' .git/absorb-patterns || true)" = 0 ]; then
    git sparse-checkout disable
  else
    git sparse-checkout set --no-cone --stdin < .git/absorb-patterns
  fi
  rm .git/absorb-patterns
  [ -z "$(git status --porcelain -- "demos/$name")" ] || {
    echo "$name: moved, but demos/$name differs from the merged tree:" >&2
    git status --short -- "demos/$name" >&2; exit 1; }
  echo "$name: now demos/$name"
done
