# SPDX-License-Identifier: MIT
# The view's OCCT meshing of bodies, on synthetic STEP round trips.
# The bundle links Qt: on Windows its DLLs are found through PATH.
target_sources(test_app_view PRIVATE tests/view_tessellation.cpp)
target_link_libraries(test_app_view PRIVATE mitcad_geometry_io mitcad_geometry TKV3d TKPrim TKBO)
add_test(NAME app.view_tessellation COMMAND test_app_view view_tessellation
  "${CMAKE_CURRENT_BINARY_DIR}/view-tessellation-files")
set_tests_properties(app.view_tessellation PROPERTIES
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
