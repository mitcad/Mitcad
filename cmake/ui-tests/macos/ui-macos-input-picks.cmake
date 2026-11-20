# SPDX-License-Identifier: MIT
# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064): the shared steps of
# tools/ui-input-picks-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-input-picks
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-input-picks-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-input-picks")
  set_tests_properties(app.ui-macos-input-picks PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
