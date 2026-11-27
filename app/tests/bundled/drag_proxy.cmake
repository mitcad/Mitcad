# SPDX-License-Identifier: MIT
# A dragged Extrude's provisional tool (mitcad#636): the owned mesh copy,
# its stretches and moves, and Extrude's rules, without a window; Move/
# Copy's provisional motion (mitcad#838).
target_sources(test_app_view PRIVATE tests/drag_proxy.cpp framework/DragProxy.cpp commands/ExtrudeDragProxy.cpp
  framework/Command.cpp commands/MoveDragMotion.cpp)
target_link_libraries(test_app_view PRIVATE Qt6::Core Qt6::Gui mitcad_geometry TKMesh TKPrim TKTopAlgo TKBRep)
add_test(NAME app.drag_proxy COMMAND test_app_view drag_proxy)
set_tests_properties(app.drag_proxy PROPERTIES
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
