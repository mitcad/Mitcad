# SPDX-License-Identifier: MIT
# Pattern on Path along a body's edges with Tangent Chain (mitcad#2105,
# GitHub mitcad/Mitcad#322) in the Qt frontend: the shared steps of
# tools/ui-path-pattern-edges-test.sh; the copies follow the rounded
# outline (checked by mitcad-cli).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-path-pattern-edges
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-path-pattern-edges-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-path-pattern-edges")
  set_tests_properties(app.ui-macos-path-pattern-edges PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
