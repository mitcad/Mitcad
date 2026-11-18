# SPDX-License-Identifier: MIT
# Measure's main distance and axis distance drawn apart where they overlap,
# each row painting its own segment (mitcad#1979).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-measure-guides
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-measure-guides-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-measure-guides")
  set_tests_properties(app.ui-macos-measure-guides PROPERTIES
    TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
