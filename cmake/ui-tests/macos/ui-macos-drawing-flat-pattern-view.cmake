# SPDX-License-Identifier: MIT
# A base view of a sheet-metal body's Flat Pattern placed from the Shows
# list, a dimension on its bend line and its bend notes turned off
# (mitcad#2280): the shared steps of
# tools/ui-drawing-flat-pattern-view-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-flat-pattern-view
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-flat-pattern-view-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-flat-pattern-view")
  set_tests_properties(app.ui-macos-drawing-flat-pattern-view PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
