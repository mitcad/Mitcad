# SPDX-License-Identifier: MIT
# The order of components in the Browser (mitcad#2104): the shared steps of
# tools/ui-browser-reorder-test.sh in the Qt frontend, a drag before
# another row, Move Down, Undo, kept with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-reorder
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-reorder-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-reorder")
  set_tests_properties(app.ui-macos-browser-reorder PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
