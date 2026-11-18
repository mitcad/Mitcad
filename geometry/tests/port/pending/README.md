# Pending port patch tests

These regression tests were written for the OCCT port's patches 0013, 0074,
0130, 0173, 0271, 0341, 0430, 0581, 0750, 0821, 1000, 1210 and 1523
(mitcad#1986). They do not prove their patch yet. Patch 0074's test passes
without the patch. The other tests fail with every patch applied, because
their geometry does not reach the patch's case.

They live here so that `geometry/CMakeLists.txt`, which globs
`tests/port/p*.cpp`, neither builds nor registers them. mitcad#2003 reworks
each one until it fails without its patch and passes with it, then moves it
back to `geometry/tests/port/`. The patch headers say `Test: pending
(mitcad#2003)` until then.
