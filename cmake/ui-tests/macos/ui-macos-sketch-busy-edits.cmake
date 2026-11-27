# SPDX-License-Identifier: MIT
# Sketch edits asked for while a job computes the model (mitcad#2500,
# GitHub mitcad/Mitcad#706: an uncaught ModelBusy ended the application
# here): the shared steps of tools/ui-sketch-busy-edits-test.sh in the Qt
# frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-busy-edits
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-busy-edits-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-busy-edits")
  set_tests_properties(app.ui-macos-sketch-busy-edits PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
