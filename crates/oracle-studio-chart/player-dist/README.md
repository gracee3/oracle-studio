# Generated chart-player assets

These files are generated from `oracle-studio-chart-player`, which keeps all
timeline sampling, interpolation, rendering, and browser interaction in Rust.
The JavaScript file is unmodified `wasm-bindgen` loader output; the chart export
adds only the generated inline bootstrap needed to instantiate the embedded
WASM bytes. No Node.js toolchain is involved.

Regenerate them from the repository root with:

```text
scripts/build-chart-player.sh
```

The source build uses Rust 1.97.1, target `wasm32-unknown-unknown`, and
`wasm-bindgen-cli` 0.2.126.
