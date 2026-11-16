# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  # $<TARGET_FILE:mitcad> is .../mitcad.app/Contents/MacOS/mitcad for a
  # bundle.
  add_test(NAME app.ui-macos
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos")
  # Five runs of up to 120 s each at the most; 77: no window server.
  set_tests_properties(app.ui-macos PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77)
  # The workflow (sketch, extrude, fillet, autosave and recovery, save, open,
  # export, import, version history) driven from inside the application by
  # tools/ui-scripts/*.mitcad-ui (app/framework/TestDriver.hpp): synthesised
  # Qt events, so no Accessibility permission; three runs of the application.
  add_test(NAME app.ui-macos-workflow
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-workflow-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-workflow")
  set_tests_properties(app.ui-macos app.ui-macos-workflow PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
