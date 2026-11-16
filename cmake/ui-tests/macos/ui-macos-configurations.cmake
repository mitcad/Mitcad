# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli)
  # Configurations (mitcad#807): the window opens and a choice in the
  # browser's Configuration bar changes the body (the test driver's
  # script, shared with the Windows test).
  add_test(NAME app.ui-macos-configurations
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configurations-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-configurations")
  set_tests_properties(app.ui-macos-configurations PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
