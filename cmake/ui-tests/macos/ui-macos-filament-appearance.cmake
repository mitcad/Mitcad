# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli)
  # A filament's colour is its appearance (mitcad#1055): the driver
  # script shared with the Linux and Windows tests, Qt frontend, light
  # and dark.
  add_test(NAME app.ui-macos-filament-appearance
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-filament-appearance-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-filament-appearance")
  set_tests_properties(app.ui-macos-filament-appearance PROPERTIES TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
