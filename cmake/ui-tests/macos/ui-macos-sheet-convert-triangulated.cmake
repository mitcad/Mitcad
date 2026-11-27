# SPDX-License-Identifier: MIT
# Convert to Sheet Metal by triangulation (mitcad#2442): the shared steps of
# tools/ui-sheet-convert-triangulated-test.sh in the Qt frontend, a shelled
# rectangle-to-round transition's four outer faces laid flat approximately.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-convert-triangulated
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-convert-triangulated-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-convert-triangulated")
  set_tests_properties(app.ui-macos-sheet-convert-triangulated PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
