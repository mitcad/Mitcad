# SPDX-License-Identifier: MIT
# The AppKit command form's check box with its text across the form and an
# inspection's result rows (mitcad#1354): Send to Slicer's Use the open
# slicer window, Measure's Distance, Δx, Δy and Δz between sketch points.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-panel-rows
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-panel-rows-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-panel-rows")
  set_tests_properties(app.ui-macos-native-panel-rows PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
