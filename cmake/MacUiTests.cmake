# SPDX-License-Identifier: MIT
# macos_desktop protects the shared Aqua focus, menu bar and clipboard.
# Unlike RUN_SERIAL it lets geometry, model and offscreen unit tests run
# beside a UI test. The shell harness keeps its cross-checkout GUI lock.
# macOS UI tests (ctest): the counterpart of the Windows tests registered in
# MesaRuntime.cmake. app.ui-macos runs the application with --demo and
# --open, saves its screenshots (the app renders into an offscreen
# framebuffer; nothing is clicked, so no Accessibility or Screen Recording
# permission is needed) and checks them with tools/ui-image-stats.py.
# The window still opens, so the test needs a window server: in a session
# without one (a daemon, a CI runner without a display) it exits with 77 and
# ctest reports it as skipped. Run it in the macOS VM, not on the host.
option(MITCAD_MACOS_UI_TESTS "macOS: register the UI test app.ui-macos (opens the app's window briefly)" ON)
option(MITCAD_MACOS_PARALLEL_HARNESSES "macOS: overlap isolated UI fixture setup and verification; application launches still own the shared Aqua lock" ON)

if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad)
  find_program(BASH_EXECUTABLE bash)
  find_package(Python3 COMPONENTS Interpreter QUIET)
endif()
# Each macOS UI test is registered by a file of its own in
# cmake/ui-tests/macos/, with its own conditions, included here in sorted
# order (mitcad#1925): a new test adds a file and edits no shared list.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS)
  file(GLOB mitcad_macos_ui_test_files CONFIGURE_DEPENDS
       "${CMAKE_CURRENT_LIST_DIR}/ui-tests/macos/*.cmake")
  list(SORT mitcad_macos_ui_test_files)
  foreach(mitcad_macos_ui_test_file IN LISTS mitcad_macos_ui_test_files)
    include("${mitcad_macos_ui_test_file}")
  endforeach()
endif()

# The release's native smoke subset (mitcad#1056). macOS releases are built
# with the AppKit frontend, an opt-in preference beside the default Qt one:
# their ctest runs the whole Qt suite and, of the AppKit frontend's tests,
# only those labelled app.ui-macos-native-smoke (start by the preference,
# open a design, a command with OK, the Browser, the timeline, quit). Every
# other test that runs the AppKit frontend is labelled
# app.ui-macos-native-full, which the release's run leaves out
# (-LE app.ui-macos-native-full); merge checks and native work run them all.
# After the registrations, so that it sees every test registered above; a
# test whose own file already gives it the label keeps it once.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI)
  set(mitcad_native_smoke_tests app.ui-macos-native app.ui-macos-native-frontend)
  # The AppKit frontend's tests whose names do not say so.
  set(mitcad_native_other_tests app.ui-macos-cut-components app.ui-macos-section app.ui-macos-table-parity)
  get_property(mitcad_ui_tests DIRECTORY PROPERTY TESTS)
  foreach(mitcad_ui_test IN LISTS mitcad_ui_tests)
    set(mitcad_ui_label "")
    if(mitcad_ui_test IN_LIST mitcad_native_smoke_tests)
      set(mitcad_ui_label app.ui-macos-native-smoke)
    elseif(mitcad_ui_test MATCHES "^app\\.ui-macos-native" OR mitcad_ui_test IN_LIST mitcad_native_other_tests)
      set(mitcad_ui_label app.ui-macos-native-full)
    endif()
    if(mitcad_ui_label)
      get_property(mitcad_ui_labels TEST ${mitcad_ui_test} PROPERTY LABELS)
      if(NOT mitcad_ui_label IN_LIST mitcad_ui_labels)
        set_property(TEST ${mitcad_ui_test} APPEND PROPERTY LABELS ${mitcad_ui_label})
      endif()
    endif()
  endforeach()
endif()

# These audited harnesses use ui_init's separate data root and ui_run_app
# for EVERY application launch. Their CLI setup and saved-file checks need
# no desktop. Keep the same tests, scripts, assertions and deadlines; move
# only Aqua ownership to the actual app lifetime in ui-macos-lib.sh.
# The native runner admits at most two such harnesses. All other desktop
# tests keep their CTest lock (and timed Metal checks keep RUN_SERIAL).
include("${CMAKE_CURRENT_LIST_DIR}/MacTestScheduling.cmake")
mitcad_macos_gui_harnesses()
