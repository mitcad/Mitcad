# SPDX-License-Identifier: MIT
# Patch takes a sketch curve for its boundary from the Browser
# (mitcad#1978): the shared steps of
# tools/ui-surface-patch-browser-curve-test.sh in the Qt frontend, on the
# reported design of the private test files (MITCAD_MITCAD_REPORTED);
# skipped without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-surface-patch-browser-curve
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-surface-patch-browser-curve-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-surface-patch-browser-curve")
  set_tests_properties(app.ui-macos-surface-patch-browser-curve PROPERTIES
    TIMEOUT 960 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
