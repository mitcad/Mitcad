# SPDX-License-Identifier: MIT
# Form and Edit Form (mitcad#1757, mitcad#1810): the shared steps of
# tools/ui-form-test.sh and tools/ui-windows-form-test.ps1 in the Qt
# frontend: a box cage's face moved and extruded, an edge creased
# semi-sharp, symmetry about a picked plane, a vertex dragged in the view,
# and a Form with Operation Cut cutting a box.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-form
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-form-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-form")
  set_tests_properties(app.ui-macos-form PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
