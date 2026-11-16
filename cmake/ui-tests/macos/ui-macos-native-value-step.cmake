# SPDX-License-Identifier: MIT
# Stepping a value in the AppKit frontend (mitcad#1350): Up, Shift+Up,
# Option+Down and the focused wheel step a command form's value field, Up
# the sketch's tool field and dimension editor, an expression stays, as
# tools/ui-value-field-step-test.sh checks the Qt frontend (mitcad#1254).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-value-step
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-value-step-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-value-step")
  set_tests_properties(app.ui-macos-native-value-step PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
