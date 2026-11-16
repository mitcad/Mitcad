# SPDX-License-Identifier: MIT
# Seven small AppKit gaps (mitcad#1358): the window's title with the
# version, the Chat agent pop-up's sign-in states, the Start page's footer
# above a shown message and no empty row of groups above the page, timeline
# search results' icons and failed colour, the Center of Mass marker's
# Delete key in Sketch mode, the Import STEP Assembly answers' tooltips.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-minor-gaps
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-minor-gaps-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:test_chat_agent>"
            "${CMAKE_BINARY_DIR}/ui-macos-native-minor-gaps")
  set_tests_properties(app.ui-macos-native-minor-gaps PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()
