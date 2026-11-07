<!-- SPDX-License-Identifier: MIT -->
# OCCT 8 compiler diagnostics: 1480

The native macOS dependency build of 0.49.0 RC4 reports five distinct
AppleClang warnings in both Debug and Release: one unused contact-curve
binding, three uses of a deprecated segment-construction alias and one
use of the deprecated knot-array output overload.

## 1480: current API names and a mutable knot copy

### 1480-chfi3d-and-brepoffset-use-current-occt-apis

- Remove the unused `CB` binding in `trimCrossingStripes`. The curve is
  never read by that function.
- Include and name `GC_MakeSegment2d` directly in the widened support
  helper. OCCT 8's `GCE2d_MakeSegment` is a deprecated alias of that same
  class, so constructors and resulting curves are unchanged.
- Assign the const array returned by `Geom_BSplineCurve::Knots()` into
  the existing local `aKnots` array before reparametrising it. This keeps
  the mutable copy; it does not modify the curve through a reference to
  its stored knots.

No compiler diagnostic is disabled, and no geometric test or tolerance
changes. These are compiler-compatibility changes, without a change to
CAD results.

## Reports and tests

The regression check compiles each of the three affected translation
units with its native Debug and Release build command and `-Werror`.
The original source produces the five warnings above. The next new RC
still requires the complete native build and all registered tests.

All six changed-source native translation-unit builds passed with `-Werror`
and no compiler diagnostics. The port patch was applied cleanly to the
original sources; the three resulting files match the compiled files byte
for byte. This scoped check does not accept the failed RC4 or replace the
next RC's full build and test gate.
