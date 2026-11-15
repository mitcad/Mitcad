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
  if(BASH_EXECUTABLE AND TARGET mitcad-cli)
    add_test(NAME app.ui-macos-print-fits
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-fits-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-print-fits" qt)
    set_tests_properties(app.ui-macos-print-fits PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    # Configurations (mitcad#807): the window opens and a choice in the
    # browser's Configuration bar changes the body (the test driver's
    # script, shared with the Windows test).
    add_test(NAME app.ui-macos-configurations
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configurations-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-configurations")
    set_tests_properties(app.ui-macos-configurations PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-bindings-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-bindings-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-bindings-parity" qt)
    set_tests_properties(app.ui-macos-bindings-parity PROPERTIES TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-binding-card-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-binding-card-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-binding-card-parity" qt)
    set_tests_properties(app.ui-macos-binding-card-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-configuration-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configuration-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-configuration-parity" qt)
    set_tests_properties(app.ui-macos-configuration-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-configuration-tables-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configuration-tables-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-configuration-tables-parity" qt)
    set_tests_properties(app.ui-macos-configuration-tables-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-material-color-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-material-color-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" qt "${CMAKE_BINARY_DIR}/ui-macos-material-color-parity")
    set_tests_properties(app.ui-macos-material-color-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-sketch-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-parity" qt)
    set_tests_properties(app.ui-macos-sketch-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-sketch-new-main-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-new-main-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-new-main-parity" qt)
    set_tests_properties(app.ui-macos-sketch-new-main-parity PROPERTIES TIMEOUT 1200 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-command-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-command-parity" qt)
    set_tests_properties(app.ui-macos-command-parity PROPERTIES TIMEOUT 1200 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-print-occurrences
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-occurrences-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-print-occurrences" qt)
    set_tests_properties(app.ui-macos-print-occurrences PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-cut-sheet-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-sheet-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cut-sheet-parity" qt)
    set_tests_properties(app.ui-macos-cut-sheet-parity PROPERTIES TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-slicer-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-slicer-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-slicer-parity" qt)
    set_tests_properties(app.ui-macos-slicer-parity PROPERTIES TIMEOUT 1000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-move-proxy-parity
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-proxy-parity-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-move-proxy-parity" qt)
    set_tests_properties(app.ui-macos-move-proxy-parity PROPERTIES TIMEOUT 2000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    # One filament, its variants per printer and nozzle (mitcad#1054): the
    # driver script shared with the Linux and Windows tests, Qt frontend.
    add_test(NAME app.ui-macos-filament-variants
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-filament-variants-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-filament-variants")
    set_tests_properties(app.ui-macos-filament-variants PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    # A filament's colour is its appearance (mitcad#1055): the driver
    # script shared with the Linux and Windows tests, Qt frontend, light
    # and dark.
    add_test(NAME app.ui-macos-filament-appearance
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-filament-appearance-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-filament-appearance")
    set_tests_properties(app.ui-macos-filament-appearance PROPERTIES TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    if(MITCAD_MACOS_NATIVE_UI)
      add_test(NAME app.ui-macos-native-bindings-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-bindings-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-bindings-parity" native)
      set_tests_properties(app.ui-macos-native-bindings-parity PROPERTIES TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-formula-input
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-formula-input-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-formula-input")
      set_tests_properties(app.ui-macos-native-formula-input PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-binding-card-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-binding-card-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-binding-card-parity" native)
      set_tests_properties(app.ui-macos-native-binding-card-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-print-occurrences
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-occurrences-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-occurrences" native)
      set_tests_properties(app.ui-macos-native-print-occurrences PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-cut-sheet-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-sheet-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-cut-sheet-parity" native)
      set_tests_properties(app.ui-macos-native-cut-sheet-parity PROPERTIES TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-slicer-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-slicer-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-slicer-parity" native)
      set_tests_properties(app.ui-macos-native-slicer-parity PROPERTIES TIMEOUT 1000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-move-proxy-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-proxy-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-move-proxy-parity" native)
      set_tests_properties(app.ui-macos-native-move-proxy-parity PROPERTIES TIMEOUT 2000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-command-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-command-parity" native)
      set_tests_properties(app.ui-macos-native-command-parity PROPERTIES TIMEOUT 1200 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-sketch-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-sketch-parity" native)
      set_tests_properties(app.ui-macos-native-sketch-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-sketch-new-main-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-new-main-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-sketch-new-main-parity" native)
      set_tests_properties(app.ui-macos-native-sketch-new-main-parity PROPERTIES TIMEOUT 1200 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-print-history
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-print-history-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-history")
      set_tests_properties(app.ui-macos-native-print-history PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-print-picker
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-print-picker-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-picker")
      set_tests_properties(app.ui-macos-native-print-picker PROPERTIES TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-table-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-table-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-table-parity")
      set_tests_properties(app.ui-macos-table-parity PROPERTIES TIMEOUT 1000 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-configuration-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configuration-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-configuration-parity" native)
      set_tests_properties(app.ui-macos-native-configuration-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-configuration-tables-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-configuration-tables-parity-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-configuration-tables-parity" native)
      set_tests_properties(app.ui-macos-native-configuration-tables-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-material-color-parity
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-material-color-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" native "${CMAKE_BINARY_DIR}/ui-macos-native-material-color-parity")
      set_tests_properties(app.ui-macos-native-material-color-parity PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-print-fits
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-fits-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-fits" native)
      set_tests_properties(app.ui-macos-native-print-fits PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-issues
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-issues-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-issues")
      set_tests_properties(app.ui-macos-native-issues PROPERTIES TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-projects
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-projects-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-projects")
      set_tests_properties(app.ui-macos-native-projects PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # Card widths and secondary windows within the visible screen (mitcad#818).
      add_test(NAME app.ui-macos-native-card-widths
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-card-widths-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-card-widths")
      set_tests_properties(app.ui-macos-native-card-widths PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # Projects keys, row editor and drags in the AppKit outline (mitcad#816).
      add_test(NAME app.ui-macos-native-projects-edit
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-projects-edit-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-projects-edit")
      set_tests_properties(app.ui-macos-native-projects-edit PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # Compact selection and command cards with their actions and fx tools (mitcad#815).
      add_test(NAME app.ui-macos-native-forms
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-forms-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-forms")
      set_tests_properties(app.ui-macos-native-forms PROPERTIES TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # The frontend at start-up (mitcad#1056): Qt by default, AppKit by
      # Settings' preference, --ui and --chrome for a run; native smoke.
      add_test(NAME app.ui-macos-native-frontend
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-frontend-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-frontend")
      set_tests_properties(app.ui-macos-native-frontend PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    endif()
  endif()
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND AND BASH_EXECUTABLE)
    # No windows: real child processes verify cross-checkout GUI exclusion
    # and lock cleanup without substituting a different production path.
    add_test(NAME repo.macos-ui-lock
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/test-macos-ui-lock.py"
              --bash "${BASH_EXECUTABLE}" --lib "${CMAKE_SOURCE_DIR}/tools/ui-macos-lib.sh")
    # This protocol test repeatedly hands Aqua between its own children.
    # Audited harnesses lock each app rather than their whole CTest entry,
    # so the desktop resource alone cannot exclude them from those hand-offs.
    set_tests_properties(repo.macos-ui-lock PROPERTIES
      TIMEOUT 600 RESOURCE_LOCK macos_desktop RUN_SERIAL TRUE)
    add_test(NAME repo.macos-test-resources
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/test-macos-test-resources.py"
              --ctest "${CMAKE_CTEST_COMMAND}" --build "${CMAKE_BINARY_DIR}")
    set_tests_properties(repo.macos-test-resources PROPERTIES TIMEOUT 30)
  endif()
  if(Python3_Interpreter_FOUND AND TARGET mitcad-cli)
    if(TARGET mitcad-render)
      add_test(NAME app.render_metal_scenes
        COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/render-metal-acceptance.py"
                "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:mitcad-render>"
                "${CMAKE_BINARY_DIR}/render-metal-scenes")
      set_tests_properties(app.render_metal_scenes PROPERTIES
        TIMEOUT 1200 SKIP_RETURN_CODE 77 RUN_SERIAL TRUE ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
    endif()
    if(MITCAD_MACOS_NATIVE_UI AND BASH_EXECUTABLE)
      add_test(NAME app.ui-macos-cut-components
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-components-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cut-components")
      set_tests_properties(app.ui-macos-cut-components PROPERTIES TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    endif()
    # The Qt frontend in every build (the release's Qt suite, mitcad#1056),
    # the AppKit frontend too where it is built.
    set(mitcad_mcp_test_uis qt)
    if(MITCAD_MACOS_NATIVE_UI)
      list(APPEND mitcad_mcp_test_uis native)
    endif()
    foreach(mitcad_mcp_test_ui IN LISTS mitcad_mcp_test_uis)
      # The output folder is the test's name without "app.", as every other
      # ui-macos-* folder (mitcad#1151).
      if(mitcad_mcp_test_ui STREQUAL "native")
        set(mitcad_mcp_folder ui-macos-native-mcp)
      else()
        set(mitcad_mcp_folder ui-macos-mcp)
      endif()
      set(mitcad_mcp_test app.${mitcad_mcp_folder})
      add_test(NAME ${mitcad_mcp_test}
        COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-mcp-test.py"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/${mitcad_mcp_folder}"
                "${mitcad_mcp_test_ui}")
      # With the live debugging flow (mitcad#265, tools/ui-mcp-live.py).
      set_tests_properties(${mitcad_mcp_test} PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
        ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
    endforeach()
  endif()
  if(BASH_EXECUTABLE)
    add_test(NAME app.ui-macos-settings
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-settings-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-settings")
    set_tests_properties(app.ui-macos-settings PROPERTIES TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    if(TARGET mitcad-release AND Python3_Interpreter_FOUND)
      add_test(NAME app.ui-macos-update
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-update-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-release>" "${CMAKE_BINARY_DIR}/ui-macos-update")
      # A build without the update test hooks (mitcad#921) is checked to
      # ignore the tests' variables instead of updating.
      if(MITCAD_UPDATE_TEST_HOOKS)
        set(mitcad_update_test_hooks ON)
      else()
        set(mitcad_update_test_hooks OFF)
      endif()
      set_tests_properties(app.ui-macos-update PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
        ENVIRONMENT "UI_UPDATE_TEST_HOOKS=${mitcad_update_test_hooks}")
    endif()
    if(TARGET mitcad-cli AND MITCAD_MACOS_NATIVE_UI)
      add_test(NAME app.ui-macos-section
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-section-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-section")
      set_tests_properties(app.ui-macos-section PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    endif()
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
    if(MITCAD_MACOS_NATIVE_UI)
      add_test(NAME app.ui-macos-native
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native")
      add_test(NAME app.ui-macos-native-cancel
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-cancel" cancel)
      add_test(NAME app.ui-macos-native-secondary
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-secondary" secondary)
      add_test(NAME app.ui-macos-native-sketch
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-sketch" sketch)
      add_test(NAME app.ui-macos-native-panels
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-panels" panels)
      add_test(NAME app.ui-macos-native-gestures
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-gestures" gestures)
      add_test(NAME app.ui-macos-native-history
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-history" history)
      add_test(NAME app.ui-macos-native-context-menus
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-context-menus" context-menus)
      add_test(NAME app.ui-macos-native-replacement
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-replacement" replacement)
      add_test(NAME app.ui-macos-native-autoselect
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-autoselect" autoselect)
      add_test(NAME app.ui-macos-native-point-pattern
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-point-pattern" point-pattern)
      add_test(NAME app.ui-macos-native-toolbar
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-toolbar" toolbar)
add_test(NAME app.ui-macos-native-familiar
  COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
          "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-familiar" familiar)
set_tests_properties(app.ui-macos-native-familiar PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      set_tests_properties(app.ui-macos-native app.ui-macos-native-cancel app.ui-macos-native-secondary app.ui-macos-native-sketch app.ui-macos-native-panels app.ui-macos-native-gestures app.ui-macos-native-history app.ui-macos-native-context-menus app.ui-macos-native-replacement app.ui-macos-native-autoselect app.ui-macos-native-point-pattern app.ui-macos-native-toolbar
        PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # The navigation bar at the bottom of the view (mitcad#1099).
      add_test(NAME app.ui-macos-native-navigation-bar
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-navigation-bar" navigation-bar)
      set_tests_properties(app.ui-macos-native-navigation-bar PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      # Table cell semantics, cell edit keys and column sizing (mitcad#814),
      # with a local synthetic library, in both appearances.
      if(TARGET mitcad-cli AND Python3_Interpreter_FOUND)
        add_test(NAME app.ui-macos-native-tables
          COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-tables-test.sh"
                  "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-tables")
        set_tests_properties(app.ui-macos-native-tables PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      endif()
      # The native part catalogue and the modeless Component Picker, each with a
      # local synthetic library made by mitcad-cli (mitcad#813, mitcad#819).
      if(TARGET mitcad-cli AND Python3_Interpreter_FOUND)
        add_test(NAME app.ui-macos-native-library
          COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-library-test.sh"
                  "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-library")
        add_test(NAME app.ui-macos-native-part-picker
          COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-part-picker-test.sh"
                  "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-part-picker")
        set_tests_properties(app.ui-macos-native-library app.ui-macos-native-part-picker
          PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      endif()
      # Shared dialog sizing contract and macOS button order (mitcad#812, #817).
      add_test(NAME app.ui-macos-native-dialog-layout
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-dialog-layout-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-dialog-layout")
      set_tests_properties(app.ui-macos-native-dialog-layout PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    endif()
    add_test(NAME app.ui-macos-render-metal
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-metal-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-render-metal")
    set_tests_properties(app.ui-macos-render-metal PROPERTIES
      TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-render-acceptance
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-metal-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-render-acceptance")
    set_tests_properties(app.ui-macos-render-acceptance PROPERTIES
      TIMEOUT 720 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop ENVIRONMENT "MITCAD_RENDER_ACCEPTANCE=1")
    add_test(NAME app.ui-macos-render-recovery
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-recovery-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-render-recovery" qt)
    set_tests_properties(app.ui-macos-render-recovery PROPERTIES
      TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-dialogs
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dialogs-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-dialogs")
    set_tests_properties(app.ui-macos-dialogs PROPERTIES
      TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-physical
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-physical-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-physical" qt)
    set_tests_properties(app.ui-macos-physical PROPERTIES
      TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    add_test(NAME app.ui-macos-cut-conversion
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-conversion-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-cut-conversion")
    set_tests_properties(app.ui-macos-cut-conversion PROPERTIES
      TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    if(TARGET mitcad-cli AND Python3_Interpreter_FOUND)
      add_test(NAME app.ui-macos-drag-preview
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drag-preview-test.sh"
                "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drag-preview" qt)
      set_tests_properties(app.ui-macos-drag-preview PROPERTIES
        TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      if(MITCAD_MACOS_NATIVE_UI)
        add_test(NAME app.ui-macos-native-drag-preview
          COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drag-preview-test.sh"
                  "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-drag-preview" native)
        set_tests_properties(app.ui-macos-native-drag-preview PROPERTIES
          TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      endif()
      add_test(NAME app.ui-macos-edit-auto-operation
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-edit-auto-operation-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-edit-auto-operation")
      set_tests_properties(app.ui-macos-edit-auto-operation PROPERTIES
        TIMEOUT 960 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      if(MITCAD_MACOS_NATIVE_UI)
        add_test(NAME app.ui-macos-native-edit-auto-operation
          COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-edit-auto-operation-test.sh"
                  "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-edit-auto-operation" native)
        set_tests_properties(app.ui-macos-native-edit-auto-operation PROPERTIES
          TIMEOUT 1920 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      endif()
    endif()
    foreach(cancel_case previews cancel quit)
      add_test(NAME app.ui-macos-command-cancel-${cancel_case}
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-cancel-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-command-cancel-${cancel_case}")
      set_tests_properties(app.ui-macos-command-cancel-${cancel_case} PROPERTIES
        TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
        ENVIRONMENT "MITCAD_TEST_CANCEL_CASE=${cancel_case}")
    endforeach()
    if(MITCAD_MACOS_NATIVE_UI)
      add_test(NAME app.ui-macos-native-physical
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-physical-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-physical" native)
      set_tests_properties(app.ui-macos-native-physical PROPERTIES
        TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-render-metal
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-metal-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-render-metal" native)
      set_tests_properties(app.ui-macos-native-render-metal PROPERTIES
        TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
      add_test(NAME app.ui-macos-native-render-acceptance
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-metal-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-render-acceptance" native)
      set_tests_properties(app.ui-macos-native-render-acceptance PROPERTIES
        TIMEOUT 720 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop ENVIRONMENT "MITCAD_RENDER_ACCEPTANCE=1")
      add_test(NAME app.ui-macos-native-render-recovery
        COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-recovery-test.sh"
                "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-render-recovery" native)
      set_tests_properties(app.ui-macos-native-render-recovery PROPERTIES
        TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
    endif()
  endif()
endif()

# Selection before Sketch Dimension uses the same placement and editor as
# interactive picks (mitcad#660).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-dimension-selection
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-dimension-selection-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-dimension-selection")
  set_tests_properties(app.ui-macos-sketch-dimension-selection PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Library/part controls and the shared Qt dialog conventions (mitcad#645).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE AND Python3_Interpreter_FOUND)
  add_test(NAME app.ui-macos-dialog-gallery
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dialog-gallery-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-dialog-gallery")
  set_tests_properties(app.ui-macos-dialog-gallery PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  foreach(cancel_case previews cancel quit)
    add_test(NAME app.ui-macos-command-cancel-docked-${cancel_case}
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-cancel-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-command-cancel-docked-${cancel_case}")
    set_tests_properties(app.ui-macos-command-cancel-docked-${cancel_case} PROPERTIES
      TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
      ENVIRONMENT "MITCAD_TEST_CHROME=docked;MITCAD_TEST_CANCEL_CASE=${cancel_case}")
  endforeach()
endif()

if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-drawing-dimensions
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-drawing-dimensions-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-drawing-dimensions")
  set_tests_properties(app.ui-macos-sketch-drawing-dimensions PROPERTIES
    TIMEOUT 540 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Selection overlays keep their body's boundaries available (mitcad#669).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-pick
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-pick-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-pick")
  set_tests_properties(app.ui-macos-pick PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Profile faces wait until a sketch geometry drag ends (mitcad#680).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-profile-drag
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-profile-drag-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-profile-drag")
  set_tests_properties(app.ui-macos-sketch-profile-drag PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Projects/Browser stacking uses the native cards' actual bounds (mitcad#681).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  # Each layout variant gets its own desktop turn and deadline. Audited
  # harnesses may otherwise take Aqua between all sixteen app launches and
  # exhaust one matrix-wide CTest deadline while its assertions keep passing.
  foreach(mitcad_project_count IN ITEMS 0 1 24)
    set(mitcad_project_schemes light)
    if(mitcad_project_count EQUAL 1)
      list(APPEND mitcad_project_schemes dark)
    endif()
    foreach(mitcad_project_chrome IN ITEMS floating docked)
      foreach(mitcad_project_scale IN ITEMS 1 2)
        foreach(mitcad_project_scheme IN LISTS mitcad_project_schemes)
          set(mitcad_project_case
            "${mitcad_project_count}-${mitcad_project_chrome}-${mitcad_project_scale}x-${mitcad_project_scheme}")
          add_test(NAME "app.ui-macos-projects-cards-${mitcad_project_case}"
            COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-projects-cards-test.sh"
                    "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
                    "${CMAKE_BINARY_DIR}/ui-macos-projects-cards/${mitcad_project_case}"
                    "${mitcad_project_count}" "${mitcad_project_chrome}"
                    "${mitcad_project_scale}" "${mitcad_project_scheme}")
          set_tests_properties("app.ui-macos-projects-cards-${mitcad_project_case}" PROPERTIES
            TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
        endforeach()
      endforeach()
    endforeach()
  endforeach()
endif()

# Selection shortcuts use the shared macOS tool-button style (mitcad#683).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-selection-create
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-selection-create-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-selection-create")
  set_tests_properties(app.ui-macos-selection-create PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Opaque tree rename fields and no duplicate delegate text (mitcad#685).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-tree-rename
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-tree-rename-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-tree-rename")
  set_tests_properties(app.ui-macos-tree-rename PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Every in-window tool button shares the macOS flat rendering (mitcad#686).
# The 0.48 macOS tool button style only: the shared Qt UI has none to audit
# (docs/macos-shared-qt.md).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE AND NOT MITCAD_MACOS_SHARED_QT_UI)
  add_test(NAME app.ui-macos-tool-buttons
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-tool-buttons-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-tool-buttons")
  set_tests_properties(app.ui-macos-tool-buttons PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Finish Sketch stays in the ribbon and palette, without a title duplicate (mitcad#687).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-finish-layout
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-finish-layout-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-finish-layout")
  set_tests_properties(app.ui-macos-sketch-finish-layout PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Inspect timeline features even when their geometry is enclosed (mitcad#695).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-timeline-inspection
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-timeline-inspection-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-timeline-inspection")
  set_tests_properties(app.ui-macos-timeline-inspection PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Navigation preferences persist across processes; speeds change real camera input.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-navigation
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-navigation-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-navigation")
  set_tests_properties(app.ui-macos-navigation PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-navigation-release
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-navigation-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-navigation-release" native)
    set_tests_properties(app.ui-macos-native-navigation-release PROPERTIES
      TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Renamed controls retain their command and stored policy identifiers (mitcad#684).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-naming
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-naming-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-naming")
  set_tests_properties(app.ui-macos-naming PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Appearance editor commit keys (mitcad#977).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  # Appearance field commits must not activate Close in either frontend.
  add_test(NAME app.ui-macos-appearance-edit-keys
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-appearance-edit-keys-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-appearance-edit-keys" qt)
  set_tests_properties(app.ui-macos-appearance-edit-keys PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-appearance-edit-keys
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-appearance-edit-keys-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-appearance-edit-keys" native)
    set_tests_properties(app.ui-macos-native-appearance-edit-keys PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Texture-folder path presentation and selection (mitcad#988).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-texture-folders
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-texture-folders-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-texture-folders" qt)
  set_tests_properties(app.ui-macos-texture-folders PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-texture-folders
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-texture-folders-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-texture-folders" native)
    set_tests_properties(app.ui-macos-native-texture-folders PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Native test contract lifetime and real Cloud header resizing (mitcad#989).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-contract-lifetime
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-contract-lifetime-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-contract-lifetime")
  set_tests_properties(app.ui-macos-native-contract-lifetime PROPERTIES
    TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Native presenter destruction must not commit a dying field (mitcad#990).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-failure-shutdown
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-failure-shutdown-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-failure-shutdown")
  set_tests_properties(app.ui-macos-native-failure-shutdown PROPERTIES
    TIMEOUT 150 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The Make tab (mitcad#1049): its tabs, the moved commands from it, the
# body card (Qt); in AppKit the same commands from its toolbar's actions.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-make-tab
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-make-tab-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-make-tab" qt)
  set_tests_properties(app.ui-macos-make-tab PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-make-tab
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-make-tab-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-make-tab" native)
    set_tests_properties(app.ui-macos-native-make-tab PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# The AppKit frontend's Chat card (mitcad#1075): View › Chat, its toolbar
# button and Command+Shift+L show and hide it; with the chat's scripted test
# agent in place of Claude Code (as app.ui-macos-chat) a message adds the
# boxes as one step that one Command+Z takes back, and an approval card
# answers.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-chat
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-chat-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_chat_agent>" "${CMAKE_BINARY_DIR}/ui-macos-native-chat")
  set_tests_properties(app.ui-macos-native-chat PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The AppKit timeline's hover preview and tooltips (mitcad#1076) and its
# timeline groups (mitcad#1077).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-timeline
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-timeline-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-timeline")
  set_tests_properties(app.ui-macos-native-timeline PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The AppKit Browser's rows: the row under the pointer highlights its
# bodies or its joint in the view (mitcad#1072); failed rows red,
# suppressed and hidden ones grey, condition bulbs (mitcad#1079).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-browser-rows
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-browser-rows-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-browser-rows")
  set_tests_properties(app.ui-macos-native-browser-rows PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  # The Configuration card and the shared left column with its dragged,
  # remembered boundary (mitcad#1020).
  add_test(NAME app.ui-macos-native-left-column
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-left-column-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-left-column")
  set_tests_properties(app.ui-macos-native-left-column PROPERTIES TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The AppKit toolbar's quick-access group (mitcad#1113): the default
# buttons, Save disabled then enabled after a change, Look At added from
# its menu, Reset to Default, folding.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-quick-access
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-quick-access-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-quick-access")
  set_tests_properties(app.ui-macos-native-quick-access PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Comments in the AppKit frontend (mitcad#1120): real face picking, pins,
# the native outline and actions, chat attachments and accessibility.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-comments
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-comments-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:test_chat_agent>"
            "${CMAKE_BINARY_DIR}/ui-macos-native-comments")
  set_tests_properties(app.ui-macos-native-comments PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Face-bounded profiles and an unsplit whole face (mitcad#1138), using the
# same synthetic fixture and viewport input on both frontends.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET test_face_profiles
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-face-bounded-profiles
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-face-bounded-profiles-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_face_profiles>"
            "${CMAKE_BINARY_DIR}/ui-macos-face-bounded-profiles" qt)
  set_tests_properties(app.ui-macos-face-bounded-profiles PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-face-bounded-profiles
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-face-bounded-profiles-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_face_profiles>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-face-bounded-profiles" native)
    set_tests_properties(app.ui-macos-native-face-bounded-profiles PROPERTIES
      TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Typed polygon sizes become driving dimensions in the drawing transaction
# (mitcad#1136); both frontends use the same keyboard and model assertions.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-polygon-dimensions
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-polygon-dimensions-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-polygon-dimensions" qt)
  set_tests_properties(app.ui-macos-sketch-polygon-dimensions PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-sketch-polygon-dimensions
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-polygon-dimensions-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-sketch-polygon-dimensions" native)
    set_tests_properties(app.ui-macos-native-sketch-polygon-dimensions PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# The native print managers by place (mitcad#1158), in a synthetic project
# and in a fresh application without a design. Uses the shared Aqua lock.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-print-managers
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-print-managers-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-managers")
  set_tests_properties(app.ui-macos-native-print-managers PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Selection Delete uses one dependency question and one undo transaction
# from either collection or the canvas (mitcad#1139).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-delete-feature-selection
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-delete-feature-selection-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-delete-feature-selection" qt)
  set_tests_properties(app.ui-macos-delete-feature-selection PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-delete-feature-selection
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-delete-feature-selection-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-delete-feature-selection" native)
    set_tests_properties(app.ui-macos-native-delete-feature-selection PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Parallel planar faces keep their plane gap as Measure's main distance,
# with the distinct minimum and a perpendicular dimension (mitcad#1161).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-measure-planar
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-measure-planar-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-measure-planar")
  set_tests_properties(app.ui-macos-measure-planar PROPERTIES
    TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Independent document owners, recovery after a real crash, and one real
# MCP bridge serving two handles (mitcad#1172). Portable frame tests use
# the guarded Qt-only fixture; product macOS keeps the system tab bar.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  # The system window tabs of the 0.48 macOS presentation only; the shared
  # Qt UI's document tabs are app.ui-macos-document-frames
  # (docs/macos-shared-qt.md).
  if(NOT MITCAD_MACOS_SHARED_QT_UI)
    add_test(NAME app.ui-macos-multi-document
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-multi-document" qt all)
    set_tests_properties(app.ui-macos-multi-document PROPERTIES
      TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
  add_test(NAME app.ui-macos-document-frames
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-document-frames" qt frames)
  set_tests_properties(app.ui-macos-document-frames PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-multi-document
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-multi-document" native all)
    set_tests_properties(app.ui-macos-native-multi-document PROPERTIES
      TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Saved-state insertion from another open document, including source Save/Cancel
# and self-insertion exclusion, through the shared UI scenario (mitcad#1172).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-insert-open-design
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-insert-open-design-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-insert-open-design" qt)
  set_tests_properties(app.ui-macos-insert-open-design PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-insert-open-design
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-insert-open-design-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
              "${CMAKE_BINARY_DIR}/ui-macos-native-insert-open-design" native)
    set_tests_properties(app.ui-macos-native-insert-open-design PROPERTIES
      TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endif()
endif()

# Plates… (mitcad#1050), the Linux test's shared input script in Qt, light
# and dark: two plates added, a body picked onto Plate 2 in vase mode, the
# plate drawn beside the design; Send to Slicer writes one 3MF of both
# plates, the spiral mode on Plate 2 only.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-print-plates
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-print-plates-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-print-plates")
  set_tests_properties(app.ui-macos-print-plates PROPERTIES TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The release's native smoke subset (mitcad#1056). macOS releases are built
# with the AppKit frontend, an opt-in preference beside the default Qt one:
# their ctest runs the whole Qt suite and, of the AppKit frontend's tests,
# only those labelled app.ui-macos-native-smoke (start by the preference,
# open a design, a command with OK, the Browser, the timeline, quit). Every
# other test that runs the AppKit frontend is labelled
# app.ui-macos-native-full, which the release's run leaves out
# (-LE app.ui-macos-native-full); merge checks and native work run them all.
# Last in this file, so that it sees every test registered above.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI)
  set(mitcad_native_smoke_tests app.ui-macos-native app.ui-macos-native-frontend)
  # The AppKit frontend's tests whose names do not say so.
  set(mitcad_native_other_tests app.ui-macos-cut-components app.ui-macos-section app.ui-macos-table-parity)
  get_property(mitcad_ui_tests DIRECTORY PROPERTY TESTS)
  foreach(mitcad_ui_test IN LISTS mitcad_ui_tests)
    if(mitcad_ui_test IN_LIST mitcad_native_smoke_tests)
      set_property(TEST ${mitcad_ui_test} APPEND PROPERTY LABELS app.ui-macos-native-smoke)
    elseif(mitcad_ui_test MATCHES "^app\\.ui-macos-native" OR mitcad_ui_test IN_LIST mitcad_native_other_tests)
      set_property(TEST ${mitcad_ui_test} APPEND PROPERTY LABELS app.ui-macos-native-full)
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

# Parameter labels must follow camera commands while their value field keeps
# the keyboard, also through native menu actions and pixel-scroll input (#1179).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-dimension-names
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dimension-names-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-dimension-names" qt)
  set_tests_properties(app.ui-macos-dimension-names PROPERTIES
    TIMEOUT 210 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  if(MITCAD_MACOS_NATIVE_UI)
    add_test(NAME app.ui-macos-native-dimension-names
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dimension-names-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-dimension-names" native)
    set_tests_properties(app.ui-macos-native-dimension-names PROPERTIES
      TIMEOUT 210 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
  endif()
endif()

# HEIF image references through macOS' ImageIO (mitcad#1304): a HEIF file
# inserted upright and embedded as a PNG file, drawn again with no HEIF
# decoder after the file is gone.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-image-references-heif
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-image-references-heif-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-image-references-heif")
  set_tests_properties(app.ui-macos-image-references-heif PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Notice places in the AppKit frontend (mitcad#1333): the pointer over a
# warned feature's Timeline item and over the command form's message while
# it is edited paints the places its notices name, and leaving clears them,
# as tools/ui-notice-places-test.sh checks the Qt frontend (mitcad#1327).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-notice-places
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-notice-places-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-notice-places")
  set_tests_properties(app.ui-macos-native-notice-places PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# The arrow keys in the AppKit frontend (mitcad#1351): the turn commands'
# keys stay with the focused Browser outline and timeline, which move their
# selection, and turn the view only while the 3D view has the keyboard, as
# tools/ui-view-arrow-keys-test.sh checks the Qt frontend (mitcad#1211).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-turn-keys
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-turn-keys-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-turn-keys")
  set_tests_properties(app.ui-macos-native-turn-keys PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# Rename in the AppKit frontend's Browser and timeline menus (mitcad#1356):
# the body's row is named in the visible AppKit outline, the feature in the
# timeline's Rename Feature dialog as F2 does, and mitcad-cli finds the new
# names in the saved design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-rename
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-rename-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-rename")
  set_tests_properties(app.ui-macos-native-rename PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# The AppKit ribbon in a narrow window (mitcad#1357): a group's pinned
# buttons fold first, the last one first, before the group collapses, by
# the Qt ribbon's rule (framework/RibbonFit, mitcad#1111).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-ribbon-pins-fold
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-ribbon-pins-fold-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-ribbon-pins-fold")
  set_tests_properties(app.ui-macos-native-ribbon-pins-fold PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# The shared Browser rows' Make marker, half-open eye and analysis notes in
# the AppKit outline (mitcad#1352): a Section Analysis row's later warning
# (badge, colour, tooltip) and info note, and a transparent body's eye;
# the Make marker is app.ui-macos-native-make-tab's.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-browser-markers
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-browser-markers-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-browser-markers")
  set_tests_properties(app.ui-macos-native-browser-markers PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# The Make card and Property Sets in the AppKit frontend (mitcad#1353): the
# card's Printer opens the searchable picker with its headings, Add Print
# Value… is a sheet that adds a value, and Property Sets' process pop-up
# shows its printer headings as items that cannot be chosen.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-make-card
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-make-card-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-make-card")
  set_tests_properties(app.ui-macos-native-make-card PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# Seven small AppKit gaps (mitcad#1358): the window's title with the
# version, the Chat agent pop-up's sign-in states, the Start page's footer
# above a shown message and no empty row of groups above the page, timeline
# search results' icons and failed colour, the Center of Mass marker's
# Delete key in Sketch mode, the Import STEP Assembly answers' tooltips.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND TARGET test_chat_agent AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-minor-gaps
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-minor-gaps-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:test_chat_agent>"
            "${CMAKE_BINARY_DIR}/ui-macos-native-minor-gaps")
  set_tests_properties(app.ui-macos-native-minor-gaps PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# Bridged tables in the AppKit frontend (mitcad#1355): the Configurations
# Rows sheet selects a block of cells, copies it, pastes it two rows down
# and empties a cell; the Appearances headings fold by a press and by keys,
# as the Qt frontend's do (mitcad#1044, mitcad#1133).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-rows-appearances
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-rows-appearances-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-rows-appearances")
  set_tests_properties(app.ui-macos-native-rows-appearances PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# Stepping a value in the AppKit frontend (mitcad#1350): Up, Shift+Up,
# Option+Down and the focused wheel step a command form's value field, Up
# the sketch's tool field and dimension editor, an expression stays, as
# tools/ui-value-field-step-test.sh checks the Qt frontend (mitcad#1254).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-value-step
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-value-step-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-value-step")
  set_tests_properties(app.ui-macos-native-value-step PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# The AppKit command form's check box with its text across the form and an
# inspection's result rows (mitcad#1354): Send to Slicer's Use the open
# slicer window, Measure's Distance, Δx, Δy and Δz between sketch points.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-panel-rows
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-panel-rows-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-panel-rows")
  set_tests_properties(app.ui-macos-native-panel-rows PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop LABELS app.ui-macos-native-full)
endif()

# Fillet on the slot-and-boss part in the Qt frontend (mitcad#1429, GitHub
# mitcad/Mitcad#173): the shared steps of tools/ui-fillet-boss-tangent-crash-test.sh;
# the application stays up whatever the fillet gives.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fillet-boss-tangent-crash
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fillet-boss-tangent-crash-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-fillet-boss-tangent-crash")
  set_tests_properties(app.ui-macos-fillet-boss-tangent-crash PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The scenario of GitHub mitcad/Mitcad#202 end to end in the Qt frontend
# (mitcad#1550): tools/ui-scripts/sketch3d-202.mitcad-ui, a rectangle
# sketched on a face, 3D Sketch checked, a line and a tangent arc onto a
# vertex of another component, a Pipe along them; the pipe's volume and
# ends and the saved design checked as tools/ui-sketch3d-202-test.sh does.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d-202
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-202-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d-202")
  set_tests_properties(app.ui-macos-sketch3d-202 PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# 3D sketches in the Qt frontend (mitcad#1524-#1527, #1544, #1548-#1550):
# tools/ui-scripts/sketch3d.mitcad-ui, the main steps of
# tools/ui-sketch3d-test.sh and tools/ui-windows-sketch3d-test.ps1 (Create
# Sketch 2D and 3D, the drawing tools with typed values, editing,
# conversion to 3D and back, a fillet, the plane-bound tools and Rib).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d")
  set_tests_properties(app.ui-macos-sketch3d PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219): the shared steps of
# tools/ui-preview-transparency-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-preview-transparency
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-preview-transparency-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-preview-transparency")
  set_tests_properties(app.ui-macos-preview-transparency PROPERTIES
    TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Timber's Along an edge (mitcad#1814) in the Qt frontend: the shared steps
# of tools/ui-timber-edge-test.sh (tools/ui-scripts/timber-edge.mitcad-ui), a
# member along a plate's edge, flush, moved by its four offsets, following
# the plate when it gets shorter; a round edge refused in words. The timber
# library of tools/ui-timber-lib.sh, fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-timber-edge
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-timber-edge-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-timber-edge")
  set_tests_properties(app.ui-macos-timber-edge PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The edit panel of a derived part (mitcad#1812): the shared steps of
# tools/ui-derived-part-edit-test.sh in the Qt frontend, with a synthetic
# BUILD library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-derived-part-edit
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-derived-part-edit-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-derived-part-edit")
  set_tests_properties(app.ui-macos-derived-part-edit PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Assembly Constraint's preview with more than one row (mitcad#1813): an
# incomplete row left out of the preview, a moved part drawn once; the
# shared steps of tools/ui-constraint-rows-preview-test.sh and
# tools/ui-windows-constraint-rows-preview-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-constraint-rows-preview
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-constraint-rows-preview-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-constraint-rows-preview")
  set_tests_properties(app.ui-macos-constraint-rows-preview PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# The Properties window (mitcad#1816): the shared steps of
# tools/ui-properties-window-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-properties-window
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-properties-window-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-properties-window")
  set_tests_properties(app.ui-macos-properties-window PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# A member placed with Timber is made of its library's wood (mitcad#1817):
# the shared steps of tools/ui-timber-material-test.sh in the Qt frontend,
# Physical Properties showing Spruce and the mass its density gives.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-timber-material
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-timber-material-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-timber-material")
  set_tests_properties(app.ui-macos-timber-material PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Convert to Features (mitcad#1815): a timber member from a library made in
# the test, joined to a plate, converted from its row's menu; its sketch
# and extrusion in the timeline, the section's width changed, the joint
# holding. The shared steps of tools/ui-convert-features-test.sh and
# tools/ui-windows-convert-features-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-convert-features
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-convert-features-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-convert-features")
  set_tests_properties(app.ui-macos-convert-features PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Form and Edit Form (mitcad#1757, mitcad#1810): the shared steps of
# tools/ui-form-test.sh and tools/ui-windows-form-test.ps1 in the Qt
# frontend: a box cage's face moved and extruded, an edge creased
# semi-sharp, symmetry about a picked plane, a vertex dragged in the view,
# and a Form with Operation Cut cutting a box.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-form
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-form-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-form")
  set_tests_properties(app.ui-macos-form PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Open Cage as Form (mitcad#1757, mitcad#1810): the shared steps of
# tools/ui-form-kept-cage-test.sh and
# tools/ui-windows-form-kept-cage-test.ps1 in the Qt frontend: an imported
# body that keeps a control cage opened from its Browser row as a Form, a
# face moved, Close keeping it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-form-kept-cage
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-form-kept-cage-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-form-kept-cage")
  set_tests_properties(app.ui-macos-form-kept-cage PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# glTF export and import (mitcad#1826): the shared steps of
# tools/ui-gltf-test.sh in the Qt frontend, File > Export to .glb and .gltf
# read back by MitCad's own reader, File > Import and Open of both.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-gltf
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-gltf-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-gltf")
  set_tests_properties(app.ui-macos-gltf PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Construction planes drawn at the size and shape of what they come from
# (mitcad#1827): the shared steps of tools/ui-construction-size-test.sh in
# the Qt frontend, a plane saved with a size opening unchanged, one off a
# long narrow face a rectangle over it, a handle's drag and Reset Size.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construction-size
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construction-size-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construction-size")
  set_tests_properties(app.ui-macos-construction-size PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# 3D relations from the 3D Sketch Palette (mitcad#1661, #1836): the shared
# steps of tools/ui-sketch3d-relations-test.sh and
# tools/ui-windows-sketch3d-relations-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d-relations
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-relations-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d-relations")
  set_tests_properties(app.ui-macos-sketch3d-relations PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Repeat runs a value's reaction with the repeated value (mitcad#1844) and
# keeps typed only what was typed (mitcad#1845): the shared steps of
# tools/ui-hole-repeat-test.sh in the Qt frontend, a Hole with a custom
# thread repeated, its sizes filled for the repeated diameter and its
# countersink following a new one; a typed countersink kept after Repeat.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-hole-repeat
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-hole-repeat-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-hole-repeat")
  set_tests_properties(app.ui-macos-hole-repeat PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# A command panel set aside hides its handles (mitcad#1844): the shared
# steps of tools/ui-joint-suspended-handles-test.sh in the Qt frontend,
# Joint's Animate with its handles hidden and a drag where one was moving
# nothing, the handles back after Esc.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-suspended-handles
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-suspended-handles-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-suspended-handles")
  set_tests_properties(app.ui-macos-joint-suspended-handles PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# The Floating Command card at Text size and Icon size 200 % (mitcad#1846):
# the shared steps of tools/ui-floating-command-width-test.sh in the Qt
# frontend, the card at least 600 px wide and the Sketch Palette whole in it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-floating-command-width
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-floating-command-width-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-floating-command-width")
  set_tests_properties(app.ui-macos-floating-command-width PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Move to Component on a rigid group joined to a shaft (mitcad#1847): the
# shared steps of tools/ui-move-occurrence-group-test.sh in the Qt
# frontend, every unavailable parent's reasons under it and a rigid group
# moved as one.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-move-occurrence-group
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-occurrence-group-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-move-occurrence-group")
  set_tests_properties(app.ui-macos-move-occurrence-group PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Joint's picks under another component than the active one (mitcad#1856):
# the shared steps of tools/ui-joint-holder-test.sh in the Qt frontend, the
# question, Make it there mounting the sub-assembly whole, Don't ask again
# and Preferences > Assembly back to Ask.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-holder
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-holder-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-holder")
  set_tests_properties(app.ui-macos-joint-holder PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
# Delete Sheet while the title block is edited (mitcad#1841): the shared
# steps of tools/ui-drawing-title-block-sheets-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-title-block-sheets
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-title-block-sheets-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-title-block-sheets")
  set_tests_properties(app.ui-macos-drawing-title-block-sheets PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# A widening taper that closes a slot (mitcad#1851): the shared steps of
# tools/ui-taper-closes-slot-test.sh in the Qt frontend, Extrude
# succeeding and Properties showing the body's volume.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-taper-closes-slot
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-taper-closes-slot-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-taper-closes-slot")
  set_tests_properties(app.ui-macos-taper-closes-slot PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# A joint edited with its Offset emptied while typing (mitcad#1860): the
# shared steps of tools/ui-joint-edit-empty-offset-test.sh in the Qt
# frontend, the joint and the rigid group after it held in the edit's
# preview and nothing moved while the Offset is empty.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-edit-empty-offset
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-edit-empty-offset-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-edit-empty-offset")
  set_tests_properties(app.ui-macos-joint-edit-empty-offset PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Edit Parameters sorted by a column (mitcad#1866): the shared steps of
# tools/ui-parameters-sort-test.sh in the Qt frontend, Name's and Value's
# headers both ways and back to the creation order, the choice kept, an
# edited and an added row selected in their place.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameters-sort
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameters-sort-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-parameters-sort")
  set_tests_properties(app.ui-macos-parameters-sort PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

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

# Flange's remembered choices (mitcad#1868): the shared steps of
# tools/ui-sheet-flange-remembered-test.sh in the Qt frontend, a new Flange
# starting with the last OK's Measured and Position and an edit loading its
# own.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-flange-remembered
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-flange-remembered-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-flange-remembered")
  set_tests_properties(app.ui-macos-sheet-flange-remembered PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Fill Cavity (mitcad#1818): the shared steps of tools/ui-fill-cavity-test.sh
# in the Qt frontend, a blind slit filled up to a plane and an open groove
# as the seed refused in words.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fill-cavity
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fill-cavity-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-fill-cavity")
  set_tests_properties(app.ui-macos-fill-cavity PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Press Pull on a face continued (mitcad#1850): a bent tube's end opens
# Revolve1's edit, then goes Along the Walls with its ring dragged past the
# walls' inner radius; a T-slot bar's end Straight with a Scale taper. The
# shared steps of tools/ui-press-pull-continued-test.sh and
# tools/ui-windows-press-pull-continued-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-press-pull-continued
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-press-pull-continued-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-press-pull-continued")
  set_tests_properties(app.ui-macos-press-pull-continued PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()

# Sketches on curved faces (mitcad#1768, mitcad#1853): the shared steps of
# tools/ui-sketch-cylinder-face-test.sh (a rectangle typed on a cylinder's
# side) and tools/ui-sketch-cone-face-test.sh (one on a cone's side, no
# layout grid on the face, the dimensions' lines drawn on it, a picture in
# files/) in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  foreach(face IN ITEMS cylinder cone)
    add_test(NAME app.ui-macos-sketch-${face}-face
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-${face}-face-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-${face}-face")
    set_tests_properties(app.ui-macos-sketch-${face}-face PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endforeach()
endif()

# The Browser's component color bars (mitcad#1876): the shared steps of
# tools/ui-browser-component-colors-test.sh in the Qt frontend, every
# component, occurrence and body row in its component's timeline color.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-component-colors
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-component-colors-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-component-colors")
  set_tests_properties(app.ui-macos-browser-component-colors PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
