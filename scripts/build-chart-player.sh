#!/bin/sh
set -eu

repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
bindgen="$repository/var/tools/bin/wasm-bindgen"

if [ ! -x "$bindgen" ]; then
    echo "missing repository-local wasm-bindgen 0.2.126 at $bindgen" >&2
    exit 1
fi

cd "$repository"
cargo build --locked --release --target wasm32-unknown-unknown \
    -p oracle-studio-chart-player
"$bindgen" --target no-modules --no-typescript \
    --out-dir crates/oracle-studio-chart/player-dist \
    target/wasm32-unknown-unknown/release/oracle_studio_chart_player.wasm
