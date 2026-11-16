# SPDX-License-Identifier: MIT
# Native presenter destruction must not commit a dying field (mitcad#990).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-failure-shutdown
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-failure-shutdown-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-failure-shutdown")
  set_tests_properties(app.ui-macos-native-failure-shutdown PROPERTIES
    TIMEOUT 150 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
