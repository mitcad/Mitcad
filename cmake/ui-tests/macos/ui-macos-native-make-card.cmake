# SPDX-License-Identifier: MIT
# The Make card and Property Sets in the AppKit frontend (mitcad#1353): the
# card's Printer opens the searchable picker with its headings, Add Print
# Value… is a sheet that adds a value, and Property Sets' process pop-up
# shows its printer headings as items that cannot be chosen.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-make-card
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-make-card-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-make-card")
  set_tests_properties(app.ui-macos-native-make-card PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
