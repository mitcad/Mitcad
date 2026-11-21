# SPDX-License-Identifier: MIT
# The Start page's Tree / Cards toggle on macOS
# (tools/ui-macos-start-page-tree-test.sh: the shared scripts of
# tools/ui-start-page-tree-test.sh).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-start-page-tree
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-start-page-tree-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-start-page-tree")
  set_tests_properties(app.ui-macos-start-page-tree PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
