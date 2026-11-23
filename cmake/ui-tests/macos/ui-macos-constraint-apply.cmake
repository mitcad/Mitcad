# SPDX-License-Identifier: MIT
# Constraint, one assembly constraint at a time (mitcad#2293): Apply and OK,
# each its own feature and undo step; the shared steps of
# tools/ui-constraint-apply-test.sh and
# tools/ui-windows-constraint-apply-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-constraint-apply
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-constraint-apply-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-constraint-apply")
  set_tests_properties(app.ui-macos-constraint-apply PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
