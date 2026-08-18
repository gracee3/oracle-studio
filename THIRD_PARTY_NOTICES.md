# Third-party notices

## AstroChart SVG geometry

Oracle Studio's transit biwheel adapts selected planetary SVG path geometry and
the idea of treating 359°/0° as adjacent during label-collision resolution from
AstroChart:

- Project: `AstroDraw/AstroChart`
- Source: <https://github.com/AstroDraw/AstroChart>
- Exact commit: `d8fb56fc7855ec4ea089710dba99f728c9b01918`
- Adapted files: `project/src/svg.ts` and selected behavior from
  `project/src/utils.ts`
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
