# SPDX-License-Identifier: MIT
# The arrow keys in the AppKit frontend (mitcad#1351): the turn commands'
# keys stay with the focused Browser outline and timeline, which move their
# selection, and turn the view only while the 3D view has the keyboard, as
# tools/ui-view-arrow-keys-test.sh checks the Qt frontend (mitcad#1211).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-turn-keys
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-turn-keys-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-turn-keys")
  set_tests_properties(app.ui-macos-native-turn-keys PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
