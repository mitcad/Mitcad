# SPDX-License-Identifier: MIT
# A rounding vanishing where its faces come tangent, its walk stopped on
# its own edge short of the vertex, with the port's patch 0961
# (mitcad#343).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_vanishing_own_edge.cpp)
add_test(NAME geometry.fillet_vanishing_own_edge COMMAND test_geometry_fillet_t_z fillet_vanishing_own_edge)
