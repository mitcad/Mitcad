# SPDX-License-Identifier: MIT
# The DRAWING tab's groups at 1280 px (mitcad#2066): the shared steps of
# tools/ui-drawing-ribbon-fit-test.sh in the Qt frontend, in the Floating
# and the Docked layout.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-ribbon-fit
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-ribbon-fit-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-ribbon-fit")
  set_tests_properties(app.ui-macos-drawing-ribbon-fit PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
