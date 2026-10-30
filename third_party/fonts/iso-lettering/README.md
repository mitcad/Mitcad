# osifont

The technical lettering of MitCad's annotations on the model (mitcad#902;
`app/view/MeasurementDrawing.cpp`, `measurementFontFamily`): a font in the
style of ISO 3098 type B (narrow, regular strokes), with the symbols
dimensions and hole callouts need (Ø ± ° × ⌴ ⌵ ↧ and ISO 1101's).

- File: `lettering.ttf` (the upstream `osifont.ttf` under a neutral name, its contents unchanged), osifont version 0.1.20230322 (its name table says
  0.1.20221020), family `osifont`, unmodified. Taken from the Ubuntu
  archive's package `fonts-osifont` 0.1.20230322+ds-4 (Debian builds it
  from the upstream source, https://github.com/hikikomori82/osifont; the
  package's SHA-256 37d0e63d5e1450abf934ed69d773ca8aa38c0cac306a722997f8ae34853493e4
  as the archive's signed index lists it). SHA-256 of the font:
  `1459a7c4143932cec1140ea15e765908be184d07ca34d271320958349c9bc9dd`.
- Licence: osifont is offered under GPL-3.0, GPL-2.0 or LGPL-3.0, each with
  a font exception (the package's copyright file: "GPL-3-only+FE or
  LGPL-3-only+FE or GPL-2-only+FE"; the upstream README names
  `osifont-lgpl3fe.ttf` the LGPL one of the same glyphs). MitCad uses it
  under **LGPL-3.0 with the font exception** (`LICENSE.txt`: the exception,
  the LGPL-3.0 and the GPL-3.0 it adds to), the most permissive offered
  (docs/development.md#licence-policy).
- As the licence policy asks of LGPL components, it is not linked
  statically: the build copies the file beside the executables
  (`fonts/lettering.ttf`; the Windows installer's `bin\fonts`, the AppImage's
  `usr/share/mitcad/fonts`, a macOS bundle's `Contents/Resources/fonts`),
  and the application loads it at run time with
  `QFontDatabase::addApplicationFont`. Replacing the file replaces the
  lettering. Without it the annotations take the UI's font.
- The pictures the application writes with annotations (snapshots, Render
  Image) are documents in the sense of the font exception.
