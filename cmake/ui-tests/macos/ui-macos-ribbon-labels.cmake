# SPDX-License-Identifier: MIT
# Preferences › Display › Toolbar labels (mitcad#1936): the shared steps of
# tools/ui-ribbon-labels-test.sh in the Qt frontend, the labels under the
# ribbon's icons in the Floating and the Docked layout.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-ribbon-labels
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-ribbon-labels-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-ribbon-labels")
  set_tests_properties(app.ui-macos-ribbon-labels PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
