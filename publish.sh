#!/bin/zsh
# Publish jhtmltopdf crates to crates.io in dependency order.
# Requires: cargo login <token> first.
set -e
cd "$(dirname "$0")"
for c in jhtml-parse jhtml-text jhtml-css jhtml-layout jhtml-pdf jhtml-js jhtmltopdf; do
  echo "== publishing $c"
  cargo publish -p "$c"
  # crates.io index needs a moment to see each new crate
  sleep 10
done
echo "all published"
