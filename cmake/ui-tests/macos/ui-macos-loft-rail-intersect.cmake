# SPDX-License-Identifier: MIT
# A loft from a circle to a square through a rail tied to Intersect points,
# and a loose rail's miss in words (mitcad#1944): the shared steps of
# tools/ui-loft-rail-intersect-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-loft-rail-intersect
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-loft-rail-intersect-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-loft-rail-intersect")
  set_tests_properties(app.ui-macos-loft-rail-intersect PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
