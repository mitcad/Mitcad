# SPDX-License-Identifier: MIT
# Repeated parts: copies of a body moved by rigid motions found as one
# part, its mirror image apart (mitcad#1277).
target_sources(test_geometry_booleans PRIVATE tests/test_repeated_parts.cpp)
add_test(NAME geometry.repeated_parts COMMAND test_geometry_booleans repeated_parts)
