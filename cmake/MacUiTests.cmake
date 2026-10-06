# SPDX-License-Identifier: MIT
# macOS UI tests (ctest): the counterpart of the Windows tests registered in
# MesaRuntime.cmake. app.ui-macos runs the application with --demo and
# --open, saves its screenshots (the app renders into an offscreen
# framebuffer; nothing is clicked, so no Accessibility or Screen Recording
# permission is needed) and checks them with tools/ui-image-stats.py.
# The window still opens, so the test needs a window server: in a session
# without one (a daemon, a CI runner without a display) it exits with 77 and
# ctest reports it as skipped. Run it in the macOS VM, not on the host.
option(MITCAD_MACOS_UI_TESTS "macOS: register the UI test app.ui-macos (opens the app's window briefly)" ON)

if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad)
  find_program(BASH_EXECUTABLE bash)
  if(BASH_EXECUTABLE)
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
    set_tests_properties(app.ui-macos-workflow PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77)
  endif()
endif()
