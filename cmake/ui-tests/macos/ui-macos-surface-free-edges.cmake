# SPDX-License-Identifier: MIT
# A surface body's free edges picked with no command, and Extend's Whole
# Opening (mitcad#1955): the shared steps of
# tools/ui-surface-free-edges-test.sh in the Qt frontend, on the reported
# design of the private test files (MITCAD_MITCAD_REPORTED); skipped
# without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-surface-free-edges
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-surface-free-edges-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-surface-free-edges")
  set_tests_properties(app.ui-macos-surface-free-edges PROPERTIES
    TIMEOUT 960 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
