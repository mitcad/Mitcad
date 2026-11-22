# SPDX-License-Identifier: MIT
# Boundary Fill (mitcad#2149): the shared steps of
# tools/ui-boundary-fill-test.sh in the Qt frontend, four surfaces and two
# planes filled as a new body, a clicked cell cut from a block, Undo, and
# the first fill edited with its cell.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-boundary-fill
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-boundary-fill-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-boundary-fill")
  set_tests_properties(app.ui-macos-boundary-fill PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
