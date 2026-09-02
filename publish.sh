#!/bin/zsh
# Publish jhtmltopdf to crates.io. Requires: cargo login <token>.
set -e
cd "$(dirname "$0")"
cargo publish
echo "published"
