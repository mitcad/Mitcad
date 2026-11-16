# SPDX-License-Identifier: MIT
# The AppKit Browser's rows: the row under the pointer highlights its
# bodies or its joint in the view (mitcad#1072); failed rows red,
# suppressed and hidden ones grey, condition bulbs (mitcad#1079).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-browser-rows
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-browser-rows-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-browser-rows")
  set_tests_properties(app.ui-macos-native-browser-rows PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  # The Configuration card and the shared left column with its dragged,
  # remembered boundary (mitcad#1020).
  add_test(NAME app.ui-macos-native-left-column
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-left-column-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-left-column")
  set_tests_properties(app.ui-macos-native-left-column PROPERTIES TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
