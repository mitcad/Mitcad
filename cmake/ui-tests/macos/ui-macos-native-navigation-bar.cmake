# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND MITCAD_MACOS_NATIVE_UI)
  # The navigation bar at the bottom of the view (mitcad#1099).
  add_test(NAME app.ui-macos-native-navigation-bar
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-navigation-bar" navigation-bar)
  set_tests_properties(app.ui-macos-native-navigation-bar PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
