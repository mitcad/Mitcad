# SPDX-License-Identifier: MIT
# The selection's look and the view's line widths on macOS (mitcad#1803,
# mitcad#2063): the shared selection-look script, its view-line-width steps
# reading the view's pixels back, since the core profile drew every line
# 1 device pixel wide before OCCT port patch 1840.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-selection-look
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-selection-look-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-selection-look")
  set_tests_properties(app.ui-macos-selection-look PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
