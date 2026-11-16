# SPDX-License-Identifier: MIT
# The AppKit ribbon in a narrow window (mitcad#1357): a group's pinned
# buttons fold first, the last one first, before the group collapses, by
# the Qt ribbon's rule (framework/RibbonFit, mitcad#1111).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-ribbon-pins-fold
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-ribbon-pins-fold-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-ribbon-pins-fold")
  set_tests_properties(app.ui-macos-native-ribbon-pins-fold PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
