# SPDX-License-Identifier: MIT
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
