# SPDX-License-Identifier: MIT
# Bridged tables in the AppKit frontend (mitcad#1355): the Configurations
# Rows sheet selects a block of cells, copies it, pastes it two rows down
# and empties a cell; the Appearances headings fold by a press and by keys,
# as the Qt frontend's do (mitcad#1044, mitcad#1133).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-rows-appearances
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-rows-appearances-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-rows-appearances")
  set_tests_properties(app.ui-macos-native-rows-appearances PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
