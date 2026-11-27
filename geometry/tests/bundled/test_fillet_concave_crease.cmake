# SPDX-License-Identifier: MIT
# A rounding whose contact crosses a concave crease of a few degrees on
# its side, with the port's patch 0980 (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_concave_crease.cpp)
add_test(NAME geometry.fillet_concave_crease COMMAND test_geometry_fillet_a_e fillet_concave_crease)
