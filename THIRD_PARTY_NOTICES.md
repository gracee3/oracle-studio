# Third-party notices

## swisseph-rs

Oracle Studio uses `swisseph-rs` 0.2.0 through Astraeus's Moshier adapter with
default and file features disabled.

- Project: <https://github.com/ninthhousestudios/swisseph-rs>
- Version: 0.2.0
- License: AGPL-3.0-or-later
- Use: pure-Rust Moshier planetary and house calculations in the Web Worker

## Swiss Ephemeris and sweph-sys

The native, non-WASM `astraeus-swiss` adapter uses `sweph-sys` 0.3.0, which
vendors Swiss Ephemeris C source. No Swiss Ephemeris data file or compiled
binary is included in this repository.

- Swiss Ephemeris: <https://www.astro.com/swisseph/swephinfo_e.htm>
- Rust binding: <https://crates.io/crates/sweph-sys>
- Binding version: 0.3.0
- Use: explicitly selected native validation and CLI calculation only
- License choice for this repository: GNU AGPL; a professional Swiss Ephemeris
  license is an alternative upstream option and is not granted by this project

Callers provide any file-backed ephemeris data separately and must follow its
provenance and licensing requirements. See
[`docs/astraeus/SWISS_EPHEMERIS.md`](docs/astraeus/SWISS_EPHEMERIS.md).

## Astronomicon font

Oracle Studio embeds the original, unmodified Astronomicon font:

- Project and published character map: <https://astronomicon.co/en/astronomicon-fonts/>
- Author: Roberto Corona
- Version: 1.1, published February 7, 2023
- Distribution: <https://astronomicon.co/AstronomiconFonts_1.1.zip>
- Distribution SHA-256: `56418e63a0def63ac3ac77e889c34682b5158d505607d154a313f6c9c2f43c9a`
- `Astronomicon.ttf` SHA-256: `917b86291ef4ded5cbdc2f1514667c73f7efea13cdebaabe9e03b4455211b0f8`
- `OFL-License.txt` SHA-256: `7cce6fa1c3e011d2794b8f480470c06150db940a90ea3d11fcbf18b2f892e0c9`
- License: SIL Open Font License 1.1
- Reserved Font Name: `Astronomicon`

The supplied license text is preserved byte-for-byte at
`assets/astronomicon-v1.1/OFL-License.txt`; the original TTF is beside it.
Oracle Studio uses the author-published map rather than a copied local map from
another project.

## AstroChart collision behavior

Oracle Studio's transit biwheel adapts the idea of treating 359°/0° as adjacent
during label-collision resolution from AstroChart:

- Project: `AstroDraw/AstroChart`
- Source: <https://github.com/AstroDraw/AstroChart>
- Exact commit: `d8fb56fc7855ec4ea089710dba99f728c9b01918`
- Adapted file: selected behavior from `project/src/utils.ts`
- Copyright: Copyright (c) 2015-2025 Arthur Fücher
- License: MIT

Oracle Studio does not include AstroChart's DOM wrapper, settings system,
dignity logic, aspect calculator, transit calculator, or animation system.

The full upstream license follows.

```text
The MIT License (MIT)

Copyright (c) 2015-2025 Arthur Fücher

Permission is hereby granted, free of charge, to any person obtaining a copy

of this software and associated documentation files (the "Software"), to deal

in the Software without restriction, including without limitation the rights

to use, copy, modify, merge, publish, distribute, sublicense, and/or sell

copies of the Software, and to permit persons to whom the Software is

furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all

copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR

IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,

FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE

AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER

LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,

OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE

SOFTWARE.
```
