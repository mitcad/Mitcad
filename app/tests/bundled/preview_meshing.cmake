# SPDX-License-Identifier: MIT
# A dragged Extrude's preview meshes on the synthetic 83-hole fixture
# (mitcad#635): the model's commands and the preview job's meshing,
# without a window; `test_app_view preview_meshing --bench` times them.
target_sources(test_app_view PRIVATE tests/preview_meshing.cpp framework/DisplayMeshes.cpp)
target_link_libraries(test_app_view PRIVATE Qt6::Core mitcad_ffi mitcad_geometry mitcad_geometry_io
  TKMesh TKPrim TKTopAlgo TKBRep TKV3d)
# A cancelled display meshing's endless face at the process's end
# (mitcad#936): the application's end and its crash reports.
target_sources(test_app_view PRIVATE framework/ProcessEnd.cpp report/CrashHandler.cpp)
if(WIN32)
  target_link_libraries(test_app_view PRIVATE dbghelp)
endif()
add_test(NAME app.preview_meshing COMMAND test_app_view preview_meshing)
set_tests_properties(app.preview_meshing PROPERTIES TIMEOUT 300
  ENVIRONMENT "MITCAD_RESULT_STORE=off"
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
