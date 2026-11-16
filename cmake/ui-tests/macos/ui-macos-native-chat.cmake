# SPDX-License-Identifier: MIT
# The AppKit frontend's Chat card (mitcad#1075): View › Chat, its toolbar
# button and Command+Shift+L show and hide it; with the chat's scripted test
# agent in place of Claude Code (as app.ui-macos-chat) a message adds the
# boxes as one step that one Command+Z takes back, and an approval card
# answers.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-chat
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-chat-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_chat_agent>" "${CMAKE_BINARY_DIR}/ui-macos-native-chat")
  set_tests_properties(app.ui-macos-native-chat PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
