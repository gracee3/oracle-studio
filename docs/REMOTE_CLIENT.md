# ThinkPad client and Supermicro service

Oracle Studio can keep its native service, encrypted vault, offline location
catalog, and Astraeus calculations on the Supermicro while a browser on the
ThinkPad runs the Rust/WASM interface. Use an SSH local-forward; do not expose
the Studio host directly on the LAN.

## Boundary

```text
ThinkPad browser
  Leptos components + presenters (Rust/WASM)
  no decrypted vault document or canonical artifacts
             |
             | same-port SSH local-forward
             v
Supermicro oracle-studio-host (127.0.0.1 only)
  authenticated JSON API + static UI files
  vault password/decrypted document + atomic persistence
  local-time resolution + offline GeoNames catalog
  pinned Astraeus library calls + immutable artifacts
```

Astraeus is currently a pinned Rust dependency compiled into the Oracle Studio
native service. It is not a separate network server. The browser talks only to
Oracle Studio's versioned protocol; Oracle Studio validates inputs, invokes
Astraeus in-process, and stores the immutable result.

## Start the service and tunnel

On the Supermicro, build the UI and start the native host:

```bash
(cd crates/oracle-studio-ui && trunk build --release)
cargo run --locked -p oracle-studio-server --bin oracle-studio-host -- \
  --dist crates/oracle-studio-ui/dist
```

The host prints a URL of the form
`http://127.0.0.1:PORT/#token=TOKEN`. Leave that process running. On the
ThinkPad, forward that exact `PORT` to the same local port:

```bash
ssh -N -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:PORT:127.0.0.1:PORT emmy@SUPERMICRO
```

Open the complete URL printed by the Supermicro in the ThinkPad browser. The
local and remote port must be identical because the Studio service verifies the
exact `Host` and `Origin`. If that local port is already occupied, stop Studio,
restart it to receive a different random port, and recreate the tunnel.

The token stays in the URL fragment until the WASM client moves it into memory
and removes it from browser history. The browser submits the vault password
through the encrypted SSH tunnel; only the native Supermicro process retains
the zeroizing password allocation and decrypted document. Closing the tunnel
does not unlock the vault, so explicitly lock Studio or stop the host when the
session ends.

Do not use SSH `-g`, `GatewayPorts`, a reverse proxy, a `0.0.0.0` bind, or a
different local forwarding port. Those variants are outside the reviewed
same-origin and loopback security model.

## Containerized browser acceptance

Chrome for Testing is validation tooling, not an Oracle Studio runtime
dependency. Its pinned browser, matching driver, Linux libraries, non-root
user, and fictional end-to-end workflow live under
`tools/browser-acceptance/`. Run the wrapper on the Supermicro after building
the native host and WASM distribution:

```bash
scripts/browser-acceptance.sh
```

The wrapper builds the validation image, starts Studio on its ordinary random
loopback port, pipes the bearer URL to the container over standard input, and
runs Chrome with dropped Linux capabilities and `no-new-privileges`. Chrome's
sandbox is disabled only inside that already isolated, unprivileged test
container. The temporary encrypted vault is removed after the native process
has been stopped; no personal chart inputs, browser profile, screenshots,
password, or bearer token are retained.
