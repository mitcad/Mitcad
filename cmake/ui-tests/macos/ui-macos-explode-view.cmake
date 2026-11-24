# SPDX-License-Identifier: MIT
# Exploded views (mitcad#2113): the shared steps of
# tools/ui-explode-view-test.sh in the Qt frontend, Auto Explode, a step by
# Explode Step's handle, the playback, the Browser's row and a drawing view
# with the explode view, Measure on the exploded bodies and a step along a
# picked edge (mitcad#2328), kept with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-explode-view
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-explode-view-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-explode-view")
  set_tests_properties(app.ui-macos-explode-view PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
