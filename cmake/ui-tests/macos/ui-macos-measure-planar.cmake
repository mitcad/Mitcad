# SPDX-License-Identifier: MIT
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
