# SPDX-License-Identifier: MIT
# A loft's Perpendicular end condition (mitcad#2345): the surface meets the
# face its section lies on at right angles within 1e-6 rad all along the
# section, for a circle on a plane (both ends), a cylinder's rim on its
# wall (a face section, and an open half circle as a surface of curves),
# the rim of a skewed barrel (a freeform face) and a block's top with
# upright rails; a free loft does not; a section off its face is refused.
# mitcad#2377: a cylinder's rim with two rails (sampled between them; a
# tilted rail refused) and loose chains of a box's and a post's edges.
target_sources(test_geometry_loft_shell PRIVATE tests/test_loft_perpendicular.cpp)
add_test(NAME geometry.loft_perpendicular COMMAND test_geometry_loft_shell loft_perpendicular)
set_tests_properties(geometry.loft_perpendicular PROPERTIES TIMEOUT 300)
