# SPDX-License-Identifier: MIT
# The shared Browser rows' Make marker, half-open eye and analysis notes in
# the AppKit outline (mitcad#1352): a Section Analysis row's later warning
# (badge, colour, tooltip) and info note, and a transparent body's eye;
# the Make marker is app.ui-macos-native-make-tab's.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-browser-markers
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-browser-markers-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-browser-markers")
  set_tests_properties(app.ui-macos-native-browser-markers PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
