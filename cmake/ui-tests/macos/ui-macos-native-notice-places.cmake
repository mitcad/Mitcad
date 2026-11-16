# SPDX-License-Identifier: MIT
# Notice places in the AppKit frontend (mitcad#1333): the pointer over a
# warned feature's Timeline item and over the command form's message while
# it is edited paints the places its notices name, and leaving clears them,
# as tools/ui-notice-places-test.sh checks the Qt frontend (mitcad#1327).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-notice-places
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-notice-places-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-notice-places")
  set_tests_properties(app.ui-macos-native-notice-places PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
