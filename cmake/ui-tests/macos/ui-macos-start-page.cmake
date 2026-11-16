# SPDX-License-Identifier: MIT
# The Start tab on macOS (tools/ui-macos-start-page-test.sh: the shared
# scripts of tools/ui-start-page-test.sh), the Qt frontend and AppKit's.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-start-page
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-start-page-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-start-page" qt)
  set_tests_properties(app.ui-macos-start-page PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-start-page
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-start-page-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-start-page" native)
    set_tests_properties(app.ui-macos-native-start-page PROPERTIES
      TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
  endif()
endif()
