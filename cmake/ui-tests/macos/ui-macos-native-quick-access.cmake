# SPDX-License-Identifier: MIT
# The AppKit toolbar's quick-access group (mitcad#1113): the default
# buttons, Save disabled then enabled after a change, Look At added from
# its menu, Reset to Default, folding.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-quick-access
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-quick-access-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-quick-access")
  set_tests_properties(app.ui-macos-native-quick-access PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
