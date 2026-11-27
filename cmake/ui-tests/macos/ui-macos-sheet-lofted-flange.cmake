# SPDX-License-Identifier: MIT
# Lofted Flange (mitcad#2512): the shared steps of
# tools/ui-sheet-lofted-flange-test.sh in the Qt frontend, a rectangle and
# a circle above it picked as the profiles, OK, the press brake transition
# one body.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-lofted-flange
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-lofted-flange-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-lofted-flange")
  set_tests_properties(app.ui-macos-sheet-lofted-flange PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
