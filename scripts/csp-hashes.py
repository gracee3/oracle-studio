#!/usr/bin/env python3
"""Generate an nginx CSP include for exact Trunk and wheel inline content."""

from __future__ import annotations

import base64
import hashlib
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
WHEEL_CSS = ROOT / "assets" / "oracle-wheel.css"
WHEEL_FONT = ROOT / "assets" / "astronomicon-v1.1" / "Astronomicon.ttf"


def sha256_source(content: bytes) -> str:
    digest = base64.b64encode(hashlib.sha256(content).digest()).decode("ascii")
    return f"'sha256-{digest}'"


def wheel_style_source() -> str:
    font = base64.b64encode(WHEEL_FONT.read_bytes())
    style = (
        b"@font-face{font-family:Astronomicon;src:url(data:font/ttf;base64,"
        + font
        + b") format('truetype');font-style:normal;font-weight:400}"
        + WHEEL_CSS.read_bytes()
    )
    return sha256_source(style)


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: csp-hashes.py INDEX_HTML OUTPUT_CONF")
    html = pathlib.Path(sys.argv[1]).read_bytes()
    scripts = []
    for match in re.finditer(rb"<script(?P<attrs>[^>]*)>(?P<body>.*?)</script>", html, re.DOTALL | re.IGNORECASE):
        if re.search(rb"\bsrc\s*=", match.group("attrs"), re.IGNORECASE):
            continue
        scripts.append(sha256_source(match.group("body")))
    if not scripts:
        raise SystemExit("Trunk output contains no inline bootstrap to authorize")
    policy = (
        "default-src 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; "
        "form-action 'self'; connect-src 'self'; img-src 'self' data: blob:; "
        f"font-src 'self' data:; style-src 'self' {wheel_style_source()}; worker-src 'self'; "
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
