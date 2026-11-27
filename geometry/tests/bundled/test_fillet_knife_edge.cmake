# SPDX-License-Identifier: MIT
# A fillet on a knife edge, two faces meeting tangentially at 0 degrees:
# the ball placed where the wedge is two radii thick, the tip beyond its
# contacts taken away, with OCCT alone, on synthetic prisms (port patch
# 1770, mitcad#1984).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_knife_edge.cpp)
add_test(NAME geometry.fillet_knife_edge COMMAND test_geometry_fillet_f_n fillet_knife_edge)
