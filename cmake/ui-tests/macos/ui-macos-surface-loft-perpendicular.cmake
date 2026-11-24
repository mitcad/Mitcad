# SPDX-License-Identifier: MIT
# Surface Loft's Start and End Condition (mitcad#2345): Perpendicular to
# Face at both ends, the surface one side face: the shared steps of
# tools/ui-surface-loft-perpendicular-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-surface-loft-perpendicular
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-surface-loft-perpendicular-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-surface-loft-perpendicular")
  set_tests_properties(app.ui-macos-surface-loft-perpendicular PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
