# SPDX-License-Identifier: MIT
# Comments in the AppKit frontend (mitcad#1120): real face picking, pins,
# the native outline and actions, chat attachments and accessibility.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-comments
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-comments-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:test_chat_agent>"
            "${CMAKE_BINARY_DIR}/ui-macos-native-comments")
  set_tests_properties(app.ui-macos-native-comments PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
