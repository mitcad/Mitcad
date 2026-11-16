# SPDX-License-Identifier: MIT
# Sketches through the bodies outside sketch mode (mitcad#1931): the shared
# steps of tools/ui-sketches-through-bodies-test.sh in the Qt frontend, the
# profile a block hides drawn through it, picked in Extrude and joined;
# Show All Sketches, Sketches Through Bodies off and on, Hide All Sketches.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketches-through-bodies
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketches-through-bodies-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketches-through-bodies")
  set_tests_properties(app.ui-macos-sketches-through-bodies PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
