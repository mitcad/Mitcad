# SPDX-License-Identifier: MIT
# A drag zoom follows the pointer's vertical motion (mitcad#1867): the shared
# steps of tools/ui-navigation-drag-zoom-test.sh in the Qt frontend, Shift +
# middle in the middle-orbit scheme, in the Floating and the Docked layout.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-navigation-drag-zoom
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-navigation-drag-zoom-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-navigation-drag-zoom")
  set_tests_properties(app.ui-macos-navigation-drag-zoom PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
