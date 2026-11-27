# SPDX-License-Identifier: MIT
# Surface quality analyses (mitcad#2517): the shared steps of
# tools/ui-surface-analysis-test.sh in the Qt frontend, Zebra, Curvature
# Map, Isocurve Analysis and Environment Map on a cylinder, kept and saved.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-surface-analysis
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-surface-analysis-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-surface-analysis")
  set_tests_properties(app.ui-macos-surface-analysis PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
