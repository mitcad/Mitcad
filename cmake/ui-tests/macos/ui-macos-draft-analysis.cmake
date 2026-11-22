# SPDX-License-Identifier: MIT
# Draft Analysis (mitcad#2166): the shared steps of
# tools/ui-draft-analysis-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-draft-analysis
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-draft-analysis-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-draft-analysis")
  set_tests_properties(app.ui-macos-draft-analysis PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
