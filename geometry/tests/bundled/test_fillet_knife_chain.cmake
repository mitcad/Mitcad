# SPDX-License-Identifier: MIT
# A knife edge running on into an ordinary edge on the same line (a bite
# cut part of the way along a wall): OCCT's fillet refuses the corner
# where the ball jumps (port patch 1790), MitCad's rounds the knife edge
# first and the other edge on its result (mitcad#1988).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_knife_chain.cpp)
add_test(NAME geometry.fillet_knife_chain COMMAND test_geometry_fillet_f_n fillet_knife_chain)
