# SPDX-License-Identifier: MIT
# Section Analysis and Hide Above Sketch over a feature preview
# (mitcad#943): a preview's clips and caps through the model's commands, as
# the window asks for them, without a window.
target_sources(test_app_view PRIVATE tests/section_preview.cpp framework/SectionPreview.cpp)
target_link_libraries(test_app_view PRIVATE Qt6::Core mitcad_ffi mitcad_geometry TKBO TKTopAlgo TKPrim TKBRep)
add_test(NAME app.section_preview COMMAND test_app_view section_preview)
set_tests_properties(app.section_preview PROPERTIES TIMEOUT 120
  ENVIRONMENT "MITCAD_RESULT_STORE=off"
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
