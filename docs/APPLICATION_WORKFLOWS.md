# Browser workflows

The application opens to the full-viewport Workbench with no authentication
gate. Workbench, Settings, and Files are hash-addressable views. “New scratch”
in Files creates a workspace immediately. Scratch mutations are volatile;
saving prompts for a public title and password, commits envelope-v2 bytes, then
converts the same document into a mounted vault.

Vaults are independently unlocked, activated, locked, exported, unloaded, and
removed. Import uses the File API and rejects duplicate public IDs until whole-
vault replacement is explicitly confirmed. Export uses a Blob download and
never obtains a filesystem handle.

People, manual/catalog locations, local-time/DST resolution, chart definitions,
immutable calculation history, comparison presets, current-result pointers,
and SVG/animated-HTML presentations use typed worker operations. The production
worker calculates with the Moshier adapter. Workbench stepping is preview-only;
Update Chart or Save As is required to append an immutable calculation.

The inner chart stays fixed. Single arrows move the outer cursor once; held
double arrows repeat after a delay. Minute/hour columns use elapsed time while
day/year columns preserve local civil time, including explicit DST-gap notices,
overlap offset retention, and leap-day clamping. Rapid input coalesces behind a
single worker request and stale results never replace the newest wheel.
