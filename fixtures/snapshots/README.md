# Renderer snapshots

`transit-biwheel.sha256` is the SHA-256 snapshot of the deterministic SVG made
by `svg_is_deterministic_accessible_escaped_and_oriented`. Keeping the digest
instead of duplicating the large single-line SVG makes intentional renderer
changes reviewable while still detecting every byte-level output change.
