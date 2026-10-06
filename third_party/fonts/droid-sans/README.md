# Droid Sans

The font Mitcad bundles for sketch text (`geometry/src/text.cpp`): the
default font of new texts, and the fallback for a font that is not
installed, so that text outlines are the same on every machine.

- Files: `DroidSans.ttf` and `DroidSans-Bold.ttf`, Droid Sans version 1.00,
  "Digitized data copyright 2007, Google Corporation", designed by Ascender
  Corporation. Taken unmodified from the JetBrains Runtime of Android Studio
  (`jbr/lib/fonts/`). SHA-256:
  - `DroidSans.ttf`: f51b88945f4c1b236f44b8d55a2d304316869127e95248c435c23f1e4142a7db
  - `DroidSans-Bold.ttf`: 2f529a3e60c007979d95d29794c3660694217fb882429fb33919d2245fe969e9
- Licence: Apache License, Version 2.0 (`LICENSE.txt`), as the fonts'
  own name tables state ("Licensed under the Apache License, Version 2.0").
  "Droid" is a trademark of Google; Mitcad does not rename or modify the
  fonts.
- The build embeds the files in the geometry library
  (`geometry/cmake/embed.cmake`); italic is synthesized by slanting.
