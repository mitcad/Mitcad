# SPDX-License-Identifier: MIT
# Parameter labels must follow camera commands while their value field keeps
# the keyboard, also through native menu actions and pixel-scroll input (#1179).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-dimension-names
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dimension-names-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-dimension-names" qt)
  set_tests_properties(app.ui-macos-dimension-names PROPERTIES
    TIMEOUT 210 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-dimension-names
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dimension-names-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-dimension-names" native)
    set_tests_properties(app.ui-macos-native-dimension-names PROPERTIES
      TIMEOUT 210 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
  endif()
endif()
