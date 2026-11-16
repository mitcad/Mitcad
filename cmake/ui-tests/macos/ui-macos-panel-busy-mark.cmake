# SPDX-License-Identifier: MIT
# A preview being computed shown without resizing the panel (mitcad#1907,
# GitHub mitcad/Mitcad#361): the shared steps of
# tools/ui-panel-busy-mark-test.sh in the Qt frontend, Docked and Floating,
# a cube's Extrude arrow dragged with slowed previews; the busy mark shows
# and hides, the panel's size stays.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-panel-busy-mark
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-panel-busy-mark-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-panel-busy-mark")
  set_tests_properties(app.ui-macos-panel-busy-mark PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
