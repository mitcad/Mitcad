# SPDX-License-Identifier: MIT
# Joins of treads and helical stringers on shared cylinders (mitcad#400).
target_sources(test_geometry_booleans PRIVATE tests/test_helical_joins.cpp)
add_test(NAME geometry.helical_joins COMMAND test_geometry_booleans helical_joins)
