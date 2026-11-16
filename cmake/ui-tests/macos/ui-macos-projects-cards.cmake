# SPDX-License-Identifier: MIT
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
