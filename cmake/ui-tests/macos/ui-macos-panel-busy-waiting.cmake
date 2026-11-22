# SPDX-License-Identifier: MIT
# A panel's preview held while the model finishes another job shows the
# busy mark (mitcad#2118): the shared steps of
# tools/ui-panel-busy-waiting-test.sh in the Qt frontend, Docked and
# Floating.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-panel-busy-waiting
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-panel-busy-waiting-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-panel-busy-waiting")
  set_tests_properties(app.ui-macos-panel-busy-waiting PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
