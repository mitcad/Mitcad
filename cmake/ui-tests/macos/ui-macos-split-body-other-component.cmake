# SPDX-License-Identifier: MIT
# Split Body with a face of another component as the Splitting Tool
# (mitcad#2299): the steps tools/ui-scripts/split-body-other-component.mitcad-ui
# shares with the Linux test and app.ui-windows-split-body-other-component,
# in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-split-body-other-component
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-split-body-other-component-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-split-body-other-component")
  set_tests_properties(app.ui-macos-split-body-other-component PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
