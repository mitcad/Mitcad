# SPDX-License-Identifier: MIT
# Patch closes part of an opening with two single free edges and a sketch
# line (mitcad#1972): the shared steps of
# tools/ui-surface-patch-curves-test.sh in the Qt frontend, on the reported
# design of the private test files (MITCAD_MITCAD_REPORTED); skipped
# without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-surface-patch-curves
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-surface-patch-curves-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-surface-patch-curves")
  set_tests_properties(app.ui-macos-surface-patch-curves PROPERTIES
    TIMEOUT 960 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
