# ThinkPad access to the static product

Publish the container only on the Supermicro loopback address:

```bash
docker run --rm --read-only --tmpfs /tmp \
  --publish 127.0.0.1:8080:8080 oracle-studio:browser-local
```

On the ThinkPad, maintain the stable local forward:

```bash
ssh -N -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:8080:127.0.0.1:8080 emmy@SUPERMICRO
```

Open `http://127.0.0.1:8080`. There is no token, random port, remote vault
process, or Astraeus listener. All vault data remains in the ThinkPad browser
profile; the Supermicro serves identical public static bytes. Do not use
`GatewayPorts` or a non-loopback bind. Non-loopback browser origins require
HTTPS outside the container.
