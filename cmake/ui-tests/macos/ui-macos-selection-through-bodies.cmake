# SPDX-License-Identifier: MIT
# The selection through the bodies (mitcad#1956): the shared steps of
# tools/ui-selection-through-bodies-test.sh in the Qt frontend, a profile a
# block hides and a face turned away drawn through it; with Selection
# Through Bodies off, neither.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-selection-through-bodies
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-selection-through-bodies-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-selection-through-bodies")
  set_tests_properties(app.ui-macos-selection-through-bodies PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
