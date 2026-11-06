# SPDX-License-Identifier: MIT
# Shared by the real registrations and the isolated registration check.
include_guard(GLOBAL)
option(MITCAD_MACOS_HEADLESS_TESTS "Run Qt component workflows without system windows or a viewport" ON)

function(mitcad_macos_cargo_build test_name)
  if(APPLE)
    # The native runner reserves at least 6 GiB and samples live tree RSS.
    # Direct CTest runs also keep at most two compiler jobs.
    set_tests_properties(${test_name} PROPERTIES RUN_SERIAL FALSE RESOURCE_LOCK cargo_build)
    set_property(TEST ${test_name} APPEND PROPERTY LABELS macos_cargo_build)
    set_property(TEST ${test_name} APPEND PROPERTY ENVIRONMENT MITCAD_CARGO_MAX_JOBS=2)
  endif()
endfunction()

function(mitcad_macos_headless_tests)
  if(APPLE AND MITCAD_MACOS_HEADLESS_TESTS)
    # These workflows inspect Qt controls, model values and saved files.
    # They never pick/render the 3D view or use native system dialogs;
    # each passed with the same assertions on Qt's offscreen platform.
    set(workflows settings dialogs naming configuration-tables-parity
        filament-variants make-tab dialog-gallery
        configurations material-color-parity update cut-conversion edit-auto-operation
        sketch-finish-layout appearance-edit-keys texture-folders insert-open-design
        print-plates selection-owners assistant-setup bom-grouping timber-seating)
    # The Projects layout cases: Qt slots and bounds at 1x and 2x. Two of
    # them stay on the desktop for the window server's own bounds and glass.
    foreach(count IN ITEMS 0 1 24)
      foreach(chrome IN ITEMS floating docked)
        foreach(scale IN ITEMS 1 2)
          foreach(scheme IN ITEMS light dark)
            set(project_case "${count}-${chrome}-${scale}x-${scheme}")
            if(NOT project_case MATCHES "^1-(floating|docked)-2x-light$")
              list(APPEND workflows "projects-cards-${project_case}")
            endif()
          endforeach()
        endforeach()
      endforeach()
    endforeach()
    foreach(workflow IN LISTS workflows)
      set(test_name "app.ui-macos-${workflow}")
      if(TEST ${test_name})
        set_tests_properties(${test_name} PROPERTIES RESOURCE_LOCK "")
        get_property(labels TEST ${test_name} PROPERTY LABELS)
        list(REMOVE_ITEM labels macos_gui_harness)
        set_property(TEST ${test_name} PROPERTY LABELS ${labels} macos_headless)
        set_property(TEST ${test_name} APPEND PROPERTY ENVIRONMENT
          QT_QPA_PLATFORM=offscreen MITCAD_TEST_HEADLESS=1)
      endif()
    endforeach()
  endif()
endfunction()

function(mitcad_macos_gui_harnesses)
  if(APPLE AND MITCAD_MACOS_PARALLEL_HARNESSES)
    # Every launch in these audited workflows goes through ui_run_app's
    # shared Aqua lock. ui_init gives each harness its own files/settings.
    foreach(frontend IN ITEMS "" "native-")
      foreach(workflow IN ITEMS bindings-parity binding-card-parity configuration-parity configuration-tables-parity slicer-parity)
        set(test_name "app.ui-macos-${frontend}${workflow}")
        if(TEST ${test_name})
          set_tests_properties(${test_name} PROPERTIES RESOURCE_LOCK "")
          set_property(TEST ${test_name} APPEND PROPERTY LABELS macos_gui_harness)
        endif()
      endforeach()
    endforeach()
  endif()
endfunction()
