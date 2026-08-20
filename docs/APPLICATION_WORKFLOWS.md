# Browser workflows

The application opens to the vault library with no authentication gate. “New
chart” creates a scratch workspace immediately. Scratch mutations are volatile;
saving prompts for a public title and password, commits envelope-v2 bytes, then
converts the same document into a mounted vault.

Vaults are independently unlocked, activated, locked, exported, unloaded, and
removed. Import uses the File API and rejects duplicate public IDs until whole-
vault replacement is explicitly confirmed. Export uses a Blob download and
never obtains a filesystem handle.

People, manual/catalog locations, local-time/DST resolution, chart definitions,
immutable calculation history, comparison presets, current-result pointers,
and SVG/animated-HTML presentations use typed worker operations. Production
calculation stops with provider unavailable; deterministic results are limited
to tests and acceptance builds.
