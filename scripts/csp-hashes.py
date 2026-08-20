#!/usr/bin/env python3
"""Generate an nginx CSP include for the exact Trunk inline bootstrap."""

from __future__ import annotations

import base64
import hashlib
import pathlib
import re
import sys


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: csp-hashes.py INDEX_HTML OUTPUT_CONF")
    html = pathlib.Path(sys.argv[1]).read_bytes()
    scripts = []
    for match in re.finditer(rb"<script(?P<attrs>[^>]*)>(?P<body>.*?)</script>", html, re.DOTALL | re.IGNORECASE):
        if re.search(rb"\bsrc\s*=", match.group("attrs"), re.IGNORECASE):
            continue
        digest = base64.b64encode(hashlib.sha256(match.group("body")).digest()).decode("ascii")
        scripts.append(f"'sha256-{digest}'")
    if not scripts:
        raise SystemExit("Trunk output contains no inline bootstrap to authorize")
    policy = (
        "default-src 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; "
        "form-action 'self'; connect-src 'self'; img-src 'self' data: blob:; "
        "font-src 'self' data:; style-src 'self'; worker-src 'self'; "
        f"script-src 'self' 'wasm-unsafe-eval' {' '.join(scripts)}"
    )
    if "'unsafe-inline'" in policy or "'unsafe-eval'" in policy:
        raise SystemExit("refusing an unsafe CSP")
    pathlib.Path(sys.argv[2]).write_text(
        f'add_header Content-Security-Policy "{policy}" always;\n', encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
