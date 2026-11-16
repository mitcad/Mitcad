# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND MITCAD_MACOS_NATIVE_UI AND TARGET mitcad-cli AND Python3_Interpreter_FOUND)
  # The native part catalogue and the modeless Component Picker, each with a
  # local synthetic library made by mitcad-cli (mitcad#813, mitcad#819).
  add_test(NAME app.ui-macos-native-library
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-library-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-library")
  add_test(NAME app.ui-macos-native-part-picker
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-part-picker-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-part-picker")
  set_tests_properties(app.ui-macos-native-library app.ui-macos-native-part-picker
    PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
