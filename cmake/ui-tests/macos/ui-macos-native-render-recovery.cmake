# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND MITCAD_MACOS_NATIVE_UI)
  add_test(NAME app.ui-macos-native-render-recovery
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-recovery-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-render-recovery" native)
  set_tests_properties(app.ui-macos-native-render-recovery PROPERTIES
    TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
