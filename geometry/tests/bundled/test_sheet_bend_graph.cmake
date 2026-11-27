# SPDX-License-Identifier: MIT
# Sheet bends on real geometry: opposite bends, partial opening and the
# exposed-clearance tear policy.
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_bend_graph.cpp)
add_test(NAME geometry.sheet_bend_graph COMMAND test_geometry_sheet sheet_bend_graph)
