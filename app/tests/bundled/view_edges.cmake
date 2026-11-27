# SPDX-License-Identifier: MIT
# The view's lines (mitcad#1633): detached copies of a body's edges cut
# finely, the body's own edges unwritten, a highlight's copy the body's.
# The bundle links Qt: on Windows its DLLs are found through PATH.
target_sources(test_app_view PRIVATE tests/view_edges.cpp view/ViewEdges.cpp)
target_link_libraries(test_app_view PRIVATE mitcad_geometry_io mitcad_geometry TKV3d TKPrim TKBO)
add_test(NAME app.view_edges COMMAND test_app_view view_edges)
set_tests_properties(app.view_edges PROPERTIES
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
