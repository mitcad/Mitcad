# SPDX-License-Identifier: MIT
# LaunchServices file opens must reach the same process and retained document
# owners after native application-delegate forwarding (mitcad#1172).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-document-open
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-document-open-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-document-open" qt)
  set_tests_properties(app.ui-macos-document-open PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-document-open
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-document-open-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-document-open" native)
    set_tests_properties(app.ui-macos-native-document-open PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
  endif()
endif()
