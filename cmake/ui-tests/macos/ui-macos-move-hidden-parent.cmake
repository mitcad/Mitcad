# SPDX-License-Identifier: MIT
# Move on a component hidden with its parent (mitcad#1995): the shared
# steps of tools/ui-move-hidden-parent-test.sh in the Qt frontend, on the
# reported design of the private test files (MITCAD_MITCAD_REPORTED);
# skipped without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-move-hidden-parent
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-hidden-parent-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-move-hidden-parent")
  set_tests_properties(app.ui-macos-move-hidden-parent PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
