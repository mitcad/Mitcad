# SPDX-License-Identifier: MIT
# Roundings whose contacts run off a narrow face: up to a sliver at the
# edge's end, and over a hole's rounded mouth, with the port's patches 0890
# and 0891 (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_end_overflow.cpp)
add_test(NAME geometry.fillet_end_overflow COMMAND test_geometry_fillet_a_e fillet_end_overflow)
