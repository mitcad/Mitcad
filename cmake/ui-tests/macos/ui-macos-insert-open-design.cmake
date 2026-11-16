# SPDX-License-Identifier: MIT
# Saved-state insertion from another open document, including source Save/Cancel
# and self-insertion exclusion, through the shared UI scenario (mitcad#1172).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-insert-open-design
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-insert-open-design-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-insert-open-design" qt)
  set_tests_properties(app.ui-macos-insert-open-design PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-insert-open-design
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-insert-open-design-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-insert-open-design" native)
    set_tests_properties(app.ui-macos-native-insert-open-design PROPERTIES
      TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()
