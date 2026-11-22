# SPDX-License-Identifier: MIT
# Selection sets (mitcad#2192): the shared steps of
# tools/ui-selection-sets-test.sh in the Qt frontend, a set created,
# renamed, selected, its members found and removed, deleted and undone,
# kept with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-selection-sets
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-selection-sets-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-selection-sets")
  set_tests_properties(app.ui-macos-selection-sets PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
