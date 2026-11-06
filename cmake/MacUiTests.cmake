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
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
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
  add_test(NAME app.ui-macos-multi-document
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-multi-document" qt all)
  add_test(NAME app.ui-macos-document-frames
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-multi-document-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-document-frames" qt frames)
  set_tests_properties(app.ui-macos-multi-document app.ui-macos-document-frames PROPERTIES
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

mitcad_macos_headless_tests()
