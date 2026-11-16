# SPDX-License-Identifier: MIT
# The Floating Command card at Text size and Icon size 200 % (mitcad#1846):
# the shared steps of tools/ui-floating-command-width-test.sh in the Qt
# frontend, the card at least 600 px wide and the Sketch Palette whole in it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-floating-command-width
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-floating-command-width-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-floating-command-width")
  set_tests_properties(app.ui-macos-floating-command-width PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
