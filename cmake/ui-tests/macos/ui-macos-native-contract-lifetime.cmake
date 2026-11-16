# SPDX-License-Identifier: MIT
# Native test contract lifetime and real Cloud header resizing (mitcad#989).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-contract-lifetime
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-contract-lifetime-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-contract-lifetime")
  set_tests_properties(app.ui-macos-native-contract-lifetime PROPERTIES
    TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
