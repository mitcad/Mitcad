# SPDX-License-Identifier: MIT
# Rename in the AppKit frontend's Browser and timeline menus (mitcad#1356):
# the body's row is named in the visible AppKit outline, the feature in the
# timeline's Rename Feature dialog as F2 does, and mitcad-cli finds the new
# names in the saved design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-rename
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-rename-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-rename")
  set_tests_properties(app.ui-macos-native-rename PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
