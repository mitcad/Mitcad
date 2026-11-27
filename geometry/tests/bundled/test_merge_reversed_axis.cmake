# SPDX-License-Identifier: MIT
# The face merge after booleans on faces of one cylinder whose axes point
# the other way, with the port's patch 1040 (mitcad#642).
target_sources(test_geometry_booleans PRIVATE tests/test_merge_reversed_axis.cpp)
add_test(NAME geometry.merge_reversed_axis COMMAND test_geometry_booleans merge_reversed_axis)
