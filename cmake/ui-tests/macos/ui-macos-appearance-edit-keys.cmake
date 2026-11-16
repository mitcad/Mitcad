# SPDX-License-Identifier: MIT
# Appearance editor commit keys (mitcad#977).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  # Appearance field commits must not activate Close in either frontend.
  add_test(NAME app.ui-macos-appearance-edit-keys
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-appearance-edit-keys-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-appearance-edit-keys" qt)
  set_tests_properties(app.ui-macos-appearance-edit-keys PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-appearance-edit-keys
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-appearance-edit-keys-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-appearance-edit-keys" native)
    set_tests_properties(app.ui-macos-native-appearance-edit-keys PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
