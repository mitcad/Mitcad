# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli AND MITCAD_MACOS_NATIVE_UI)
  add_test(NAME app.ui-macos-native-slicer-parity
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-slicer-parity-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-slicer-parity" native)
  set_tests_properties(app.ui-macos-native-slicer-parity PROPERTIES TIMEOUT 1000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
